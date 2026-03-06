//! Wash Out effect - one-knob transition macro
//!
//! Combines highpass filter sweep, reverb, and delay for smooth
//! DJ transitions. The single `wash` knob controls all three
//! components simultaneously, sweeping from dry signal to a
//! washed-out, reverberant, echoing texture.

use super::Effect;
use std::f32::consts::PI;

/// Wash color preset - tints the wet signal character
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WashColor {
    /// Slight high boost - airy, bright wash
    #[default]
    Bright,
    /// Cut highs slightly - warm, smooth wash
    Warm,
    /// Heavy high cut - dark, submerged wash
    Dark,
}

// ---------------------------------------------------------------------------
// Internal DSP components (not Effect trait impls, just simple structs)
// ---------------------------------------------------------------------------

/// Simple one-pole highpass filter
struct SimpleHPF {
    prev_input: f32,
    prev_output: f32,
    coeff: f32,
}

impl SimpleHPF {
    fn new() -> Self {
        Self {
            prev_input: 0.0,
            prev_output: 0.0,
            coeff: 1.0, // pass-through until set
        }
    }

    /// Update coefficient from cutoff frequency
    fn set_cutoff(&mut self, cutoff_hz: f32, sample_rate: f32) {
        let w = (PI * cutoff_hz / sample_rate).tan();
        self.coeff = 1.0 / (1.0 + w);
    }

    /// Process a single sample
    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let output = self.coeff * (self.prev_output + input - self.prev_input);
        self.prev_input = input;
        self.prev_output = output;
        output
    }

    fn reset(&mut self) {
        self.prev_input = 0.0;
        self.prev_output = 0.0;
    }
}

/// Lowpass-feedback comb filter for reverb
struct SimpleComb {
    buffer: Vec<f32>,
    size: usize,
    pos: usize,
    filter_store: f32,
}

impl SimpleComb {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size],
            size,
            pos: 0,
            filter_store: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, input: f32, feedback: f32, damping: f32) -> f32 {
        let output = self.buffer[self.pos];
        self.filter_store = output * (1.0 - damping) + self.filter_store * damping;
        self.buffer[self.pos] = input + self.filter_store * feedback;
        self.pos = (self.pos + 1) % self.size;
        output
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.filter_store = 0.0;
        self.pos = 0;
    }
}

/// Schroeder allpass filter for reverb diffusion
struct SimpleAllpass {
    buffer: Vec<f32>,
    size: usize,
    pos: usize,
}

impl SimpleAllpass {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size],
            size,
            pos: 0,
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let buffered = self.buffer[self.pos];
        let output = -input + buffered;
        self.buffer[self.pos] = input + buffered * 0.5;
        self.pos = (self.pos + 1) % self.size;
        output
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.pos = 0;
    }
}

/// Simple mono delay line
struct SimpleDelay {
    buffer: Vec<f32>,
    size: usize,
    write_pos: usize,
    delay_samples: usize,
    feedback: f32,
}

impl SimpleDelay {
    fn new(max_samples: usize, delay_samples: usize, feedback: f32) -> Self {
        Self {
            buffer: vec![0.0; max_samples],
            size: max_samples,
            write_pos: 0,
            delay_samples: delay_samples.min(max_samples - 1),
            feedback,
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let read_pos = if self.write_pos >= self.delay_samples {
            self.write_pos - self.delay_samples
        } else {
            self.size - (self.delay_samples - self.write_pos)
        };
        let delayed = self.buffer[read_pos];
        self.buffer[self.write_pos] = input + delayed * self.feedback;
        self.write_pos = (self.write_pos + 1) % self.size;
        delayed
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
    }
}

/// One-pole lowpass filter for color tinting
struct OnePoleLPF {
    state: f32,
    coeff: f32,
}

impl OnePoleLPF {
    fn new() -> Self {
        Self {
            state: 0.0,
            coeff: 0.5,
        }
    }

    fn set_cutoff(&mut self, cutoff_hz: f32, sample_rate: f32) {
        let w = (PI * cutoff_hz / sample_rate).tan();
        self.coeff = w / (1.0 + w);
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        self.state += self.coeff * (input - self.state);
        self.state
    }

    fn reset(&mut self) {
        self.state = 0.0;
    }
}

// ---------------------------------------------------------------------------
// Comb / allpass tunings at 44.1kHz (subset of Freeverb for lighter reverb)
// ---------------------------------------------------------------------------

const COMB_TUNINGS: [usize; 4] = [1116, 1188, 1277, 1356];
const ALLPASS_TUNINGS: [usize; 2] = [556, 441];

// ---------------------------------------------------------------------------
// Wash Out effect
// ---------------------------------------------------------------------------

/// Wash Out effect - one-knob transition macro
///
/// The `wash` parameter (0.0 - 1.0) simultaneously drives:
/// - Highpass filter sweep (20Hz -> 4kHz)
/// - Reverb send (off -> 80%)
/// - Delay send (off -> 60%)
/// - Dry level reduction (100% -> 15%)
///
/// This creates the classic "wash out" transition used by DJs
/// to smoothly exit a track into a reverberant, filtered texture.
pub struct WashOut {
    enabled: bool,
    sample_rate: f32,

    // Parameters
    wash: f32,           // 0.0 - 1.0, master control knob
    reverb_size: f32,    // 0.0 - 1.0, reverb room size
    color: WashColor,

    // Internal smoothing for wash knob
    current_wash: f32,
    target_wash: f32,

    // HPF (stereo - one per channel)
    hpf_l: SimpleHPF,
    hpf_r: SimpleHPF,

    // Reverb (mono - 4 comb + 2 allpass)
    combs: [SimpleComb; 4],
    allpasses: [SimpleAllpass; 2],

    // Delay (mono)
    delay: SimpleDelay,

    // Color tinting LPF (applied to wet signal)
    color_lpf: OnePoleLPF,

    // Wet envelope for click-free enable/disable
    wet_target: f32,
    wet_current: f32,
}

/// Wet envelope smoothing coefficient (~10ms at 48kHz)
const WET_SMOOTH_COEFF: f32 = 0.9995;

/// Wash knob smoothing coefficient
const WASH_SMOOTH_COEFF: f32 = 0.999;

impl WashOut {
    /// Create a new wash out effect
    pub fn new(sample_rate: f32) -> Self {
        let scale = sample_rate / 44100.0;

        // Create comb filters scaled for sample rate
        let combs = std::array::from_fn(|i| {
            SimpleComb::new((COMB_TUNINGS[i] as f32 * scale) as usize)
        });

        // Create allpass filters scaled for sample rate
        let allpasses = std::array::from_fn(|i| {
            SimpleAllpass::new((ALLPASS_TUNINGS[i] as f32 * scale) as usize)
        });

        // Delay: ~500ms max, default ~300ms
        let max_delay_samples = (sample_rate * 0.5) as usize; // 500ms
        let delay_samples = (sample_rate * 0.3) as usize;     // 300ms
        let delay = SimpleDelay::new(max_delay_samples, delay_samples, 0.3);

        let mut color_lpf = OnePoleLPF::new();
        // Default Bright: high cutoff (mostly pass-through)
        color_lpf.set_cutoff(12000.0, sample_rate);

        Self {
            enabled: false,
            sample_rate,
            wash: 0.0,
            reverb_size: 0.7,
            color: WashColor::Bright,
            current_wash: 0.0,
            target_wash: 0.0,
            hpf_l: SimpleHPF::new(),
            hpf_r: SimpleHPF::new(),
            combs,
            allpasses,
            delay,
            color_lpf,
            wet_target: 0.0,
            wet_current: 0.0,
        }
    }

    /// Set wash amount (0.0 - 1.0)
    ///
    /// This is the master control knob that drives the entire effect.
    /// At 0.0 the signal is dry; at 1.0 the signal is fully washed out.
    pub fn set_wash(&mut self, wash: f32) {
        self.wash = wash.clamp(0.0, 1.0);
        self.target_wash = self.wash;
    }

    /// Get wash amount
    pub fn wash(&self) -> f32 {
        self.wash
    }

    /// Set reverb room size (0.0 - 1.0)
    pub fn set_reverb_size(&mut self, size: f32) {
        self.reverb_size = size.clamp(0.0, 1.0);
    }

    /// Get reverb room size
    pub fn reverb_size(&self) -> f32 {
        self.reverb_size
    }

    /// Set wash color
    pub fn set_color(&mut self, color: WashColor) {
        self.color = color;
        let cutoff = match color {
            WashColor::Bright => 12000.0,
            WashColor::Warm => 6000.0,
            WashColor::Dark => 2000.0,
        };
        self.color_lpf.set_cutoff(cutoff, self.sample_rate);
    }

    /// Get wash color
    pub fn color(&self) -> WashColor {
        self.color
    }

    /// Map wash parameter to HPF cutoff frequency (Hz)
    ///
    /// Piecewise linear mapping:
    /// - wash 0.0-0.3: 20Hz -> 200Hz
    /// - wash 0.3-0.7: 200Hz -> 1500Hz
    /// - wash 0.7-1.0: 1500Hz -> 4000Hz
    #[inline]
    fn wash_to_hpf_cutoff(wash: f32) -> f32 {
        if wash < 0.3 {
            let t = wash / 0.3;
            20.0 + t * (200.0 - 20.0)
        } else if wash < 0.7 {
            let t = (wash - 0.3) / 0.4;
            200.0 + t * (1500.0 - 200.0)
        } else {
            let t = (wash - 0.7) / 0.3;
            1500.0 + t * (4000.0 - 1500.0)
        }
    }

    /// Map wash parameter to reverb send level
    ///
    /// - wash 0.0-0.2: 0.0 (no reverb)
    /// - wash 0.2-0.7: ramp 0.0 -> 0.8
    /// - wash 0.7-1.0: 0.8 (plateau)
    #[inline]
    fn wash_to_reverb_send(wash: f32) -> f32 {
        if wash < 0.2 {
            0.0
        } else if wash < 0.7 {
            let t = (wash - 0.2) / 0.5;
            t * 0.8
        } else {
            0.8
        }
    }

    /// Map wash parameter to delay send level
    ///
    /// - wash 0.0-0.3: 0.0 (no delay)
    /// - wash 0.3-0.8: ramp 0.0 -> 0.6
    /// - wash 0.8-1.0: 0.6 (plateau)
    #[inline]
    fn wash_to_delay_send(wash: f32) -> f32 {
        if wash < 0.3 {
            0.0
        } else if wash < 0.8 {
            let t = (wash - 0.3) / 0.5;
            t * 0.6
        } else {
            0.6
        }
    }

    /// Map wash parameter to dry signal level
    ///
    /// - wash 0.0-0.3: 1.0 (full dry)
    /// - wash 0.3-1.0: ramp 1.0 -> 0.15
    #[inline]
    fn wash_to_dry_level(wash: f32) -> f32 {
        if wash < 0.3 {
            1.0
        } else {
            let t = (wash - 0.3) / 0.7;
            1.0 - t * 0.85
        }
    }

    /// Soft clipper to prevent output from exceeding ceiling
    #[inline]
    fn soft_clip(x: f32) -> f32 {
        if x > 1.0 {
            1.0 - 1.0 / (1.0 + (x - 1.0) * 2.0)
        } else if x < -1.0 {
            -1.0 + 1.0 / (1.0 + (-x - 1.0) * 2.0)
        } else {
            x
        }
    }
}

impl Effect for WashOut {
    fn process(&mut self, samples: &mut [f32]) {
        // Skip processing only if fully disabled and envelope has settled
        if !self.enabled && self.wet_current < 0.0001 {
            return;
        }

        // Pre-compute HPF cutoff once per buffer (tan() is expensive)
        // Use the smoothed wash value at buffer start — the per-sample smoothing
        // still runs inside the loop, but HPF coefficient updates once per buffer
        {
            let wash_preview = self.current_wash * WASH_SMOOTH_COEFF
                + self.target_wash * (1.0 - WASH_SMOOTH_COEFF);
            let hpf_cutoff = Self::wash_to_hpf_cutoff(wash_preview);
            self.hpf_l.set_cutoff(hpf_cutoff, self.sample_rate);
            self.hpf_r.set_cutoff(hpf_cutoff, self.sample_rate);
        }

        for frame in samples.chunks_mut(2) {
            if frame.len() < 2 {
                continue;
            }

            // Smooth wet envelope toward target
            self.wet_current =
                WET_SMOOTH_COEFF * self.wet_current + (1.0 - WET_SMOOTH_COEFF) * self.wet_target;

            // Smooth wash parameter
            self.current_wash = self.current_wash * WASH_SMOOTH_COEFF
                + self.target_wash * (1.0 - WASH_SMOOTH_COEFF);

            let wash = self.current_wash;

            // Derive send/dry parameters from wash
            let reverb_send = Self::wash_to_reverb_send(wash);
            let delay_send = Self::wash_to_delay_send(wash);
            let dry_level = Self::wash_to_dry_level(wash);

            // 1. Apply HPF to both channels
            let filtered_l = self.hpf_l.process(frame[0]);
            let filtered_r = self.hpf_r.process(frame[1]);

            // 2. Create mono mix for reverb and delay
            let mono = (filtered_l + filtered_r) * 0.5;

            // 3. Process through reverb (4 parallel combs + 2 series allpasses)
            let feedback = self.reverb_size * 0.24 + 0.6;
            let damping = 0.5;
            let mut reverb_out = 0.0;
            for comb in &mut self.combs {
                reverb_out += comb.process(mono * 0.25, feedback, damping);
            }
            reverb_out *= 0.25; // Scale down (4 combs summed)
            for allpass in &mut self.allpasses {
                reverb_out = allpass.process(reverb_out);
            }

            // 4. Process through delay
            let delay_out = self.delay.process(mono);

            // 5. Color tinting on wet signals
            let wet_sum = reverb_out * reverb_send + delay_out * delay_send;
            let colored_wet = match self.color {
                WashColor::Bright => {
                    // Slight high boost: blend raw with LPF output to emphasize highs
                    let lpf_out = self.color_lpf.process(wet_sum);
                    wet_sum * 1.1 - lpf_out * 0.1
                }
                WashColor::Warm | WashColor::Dark => {
                    // LPF applied to wet signal (cutoff already set by set_color)
                    self.color_lpf.process(wet_sum)
                }
            };

            // 6. Mix: filtered * dry_level + colored wet
            let out_l = filtered_l * dry_level + colored_wet;
            let out_r = filtered_r * dry_level + colored_wet;

            // 7. Apply overall wet/dry envelope and soft clip
            let dry_l = frame[0];
            let dry_r = frame[1];
            frame[0] = Self::soft_clip(dry_l * (1.0 - self.wet_current) + out_l * self.wet_current);
            frame[1] = Self::soft_clip(dry_r * (1.0 - self.wet_current) + out_r * self.wet_current);
        }
    }

    fn reset(&mut self) {
        self.hpf_l.reset();
        self.hpf_r.reset();
        for comb in &mut self.combs {
            comb.reset();
        }
        for allpass in &mut self.allpasses {
            allpass.reset();
        }
        self.delay.reset();
        self.color_lpf.reset();
        self.current_wash = 0.0;
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.wet_target = if enabled { 1.0 } else { 0.0 };
        // Note: don't reset on disable - let tails naturally fade out
    }

    fn name(&self) -> &'static str {
        "Wash Out"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_washout_creation() {
        let washout = WashOut::new(48000.0);
        assert!(!washout.is_enabled());
        assert_eq!(washout.wash(), 0.0);
        assert_eq!(washout.reverb_size(), 0.7);
        assert_eq!(washout.color(), WashColor::Bright);
        assert_eq!(washout.name(), "Wash Out");
    }

    #[test]
    fn test_washout_parameter_clamping() {
        let mut washout = WashOut::new(48000.0);

        washout.set_wash(1.5);
        assert_eq!(washout.wash(), 1.0);

        washout.set_wash(-0.5);
        assert_eq!(washout.wash(), 0.0);

        washout.set_reverb_size(2.0);
        assert_eq!(washout.reverb_size(), 1.0);

        washout.set_reverb_size(-1.0);
        assert_eq!(washout.reverb_size(), 0.0);
    }

    #[test]
    fn test_washout_disabled_passthrough() {
        let mut washout = WashOut::new(48000.0);
        // Disabled with envelope settled - should not modify samples
        let mut samples = vec![0.5, -0.5, 0.3, -0.3];
        let original = samples.clone();
        washout.process(&mut samples);
        assert_eq!(samples, original);
    }

    #[test]
    fn test_washout_processes_audio() {
        let mut washout = WashOut::new(48000.0);
        washout.set_enabled(true);
        washout.set_wash(0.8);
        washout.wet_current = 1.0; // Force wet for test
        washout.current_wash = 0.8; // Skip smoothing for test

        let mut samples = vec![0.5, 0.5, 0.3, 0.3, 0.1, 0.1, -0.2, -0.2];
        washout.process(&mut samples);

        // Output should be finite and modified
        assert!(samples.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn test_washout_no_nan_or_inf() {
        let mut washout = WashOut::new(44100.0);
        washout.set_enabled(true);
        washout.set_wash(1.0);
        washout.wet_current = 1.0;
        washout.current_wash = 1.0;

        // Process a longer buffer to exercise comb/allpass/delay
        let mut samples = vec![0.0; 4096];
        // Add an impulse
        samples[0] = 1.0;
        samples[1] = 1.0;

        washout.process(&mut samples);

        assert!(
            samples.iter().all(|s| s.is_finite()),
            "Output contains NaN or Inf"
        );
    }

    #[test]
    fn test_washout_color_variants() {
        for color in [WashColor::Bright, WashColor::Warm, WashColor::Dark] {
            let mut washout = WashOut::new(48000.0);
            washout.set_enabled(true);
            washout.set_wash(0.7);
            washout.set_color(color);
            washout.wet_current = 1.0;
            washout.current_wash = 0.7;

            let mut samples = vec![0.5; 512];
            washout.process(&mut samples);

            assert!(
                samples.iter().all(|s| s.is_finite()),
                "Color {:?} produced NaN/Inf",
                color
            );
        }
    }

    #[test]
    fn test_washout_reset() {
        let mut washout = WashOut::new(48000.0);
        washout.set_enabled(true);
        washout.set_wash(1.0);
        washout.wet_current = 1.0;
        washout.current_wash = 1.0;

        // Process some audio to fill internal buffers
        let mut samples = vec![1.0; 256];
        washout.process(&mut samples);

        // Reset
        washout.reset();
        assert_eq!(washout.current_wash, 0.0);

        // After reset, processing silence should yield silence (once envelope settles)
        washout.wet_current = 1.0;
        washout.current_wash = 0.0;
        washout.target_wash = 0.0;
        let mut silence = vec![0.0; 2048];
        washout.process(&mut silence);

        // With wash at 0 and input silence, output should be near-zero
        let max_val = silence.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);
        assert!(
            max_val < 0.01,
            "Expected near-silence after reset, got max amplitude {}",
            max_val
        );
    }

    #[test]
    fn test_hpf_cutoff_mapping() {
        // wash=0.0 -> 20Hz
        assert!((WashOut::wash_to_hpf_cutoff(0.0) - 20.0).abs() < 0.01);
        // wash=0.3 -> 200Hz
        assert!((WashOut::wash_to_hpf_cutoff(0.3) - 200.0).abs() < 0.01);
        // wash=0.7 -> 1500Hz
        assert!((WashOut::wash_to_hpf_cutoff(0.7) - 1500.0).abs() < 0.01);
        // wash=1.0 -> 4000Hz
        assert!((WashOut::wash_to_hpf_cutoff(1.0) - 4000.0).abs() < 0.01);
    }

    #[test]
    fn test_send_levels_mapping() {
        // Reverb send
        assert_eq!(WashOut::wash_to_reverb_send(0.0), 0.0);
        assert_eq!(WashOut::wash_to_reverb_send(0.1), 0.0);
        assert!(WashOut::wash_to_reverb_send(0.5) > 0.0);
        assert!((WashOut::wash_to_reverb_send(0.8) - 0.8).abs() < 0.01);

        // Delay send
        assert_eq!(WashOut::wash_to_delay_send(0.0), 0.0);
        assert_eq!(WashOut::wash_to_delay_send(0.2), 0.0);
        assert!(WashOut::wash_to_delay_send(0.6) > 0.0);
        assert!((WashOut::wash_to_delay_send(0.9) - 0.6).abs() < 0.01);

        // Dry level
        assert_eq!(WashOut::wash_to_dry_level(0.0), 1.0);
        assert_eq!(WashOut::wash_to_dry_level(0.2), 1.0);
        assert!((WashOut::wash_to_dry_level(1.0) - 0.15).abs() < 0.01);
    }

    #[test]
    fn test_washout_soft_clip() {
        assert!(WashOut::soft_clip(2.0) < 1.0);
        assert!(WashOut::soft_clip(2.0) > 0.5);
        assert!(WashOut::soft_clip(-2.0) > -1.0);
        assert!(WashOut::soft_clip(-2.0) < -0.5);
        assert_eq!(WashOut::soft_clip(0.5), 0.5);
        assert_eq!(WashOut::soft_clip(-0.5), -0.5);
    }

    #[test]
    fn test_washout_default_color() {
        assert_eq!(WashColor::default(), WashColor::Bright);
    }
}
