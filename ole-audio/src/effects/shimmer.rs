//! Shimmer reverb effect - reverb with pitch-shifted feedback
//!
//! Combines a 4-line feedback delay network with granular pitch shifting
//! in the feedback path to create ethereal, evolving reverb tails.
//! The pitch shifter uses two crossfading grains with Hann windowing
//! for smooth, artifact-free transposition.

use super::Effect;
use std::f32::consts::PI;

/// Pitch shift interval for the shimmer feedback path
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShimmerPitch {
    #[default]
    Oct1, // +1 octave (2x speed)
    Oct2,     // +2 octaves (4x speed)
    Fifth,    // Perfect fifth (1.5x speed)
    Oct1Down, // -1 octave (0.5x speed)
}

impl ShimmerPitch {
    pub fn ratio(self) -> f32 {
        match self {
            Self::Oct1 => 2.0,
            Self::Oct2 => 4.0,
            Self::Fifth => 1.5,
            Self::Oct1Down => 0.5,
        }
    }
}

/// Base delay line sizes at 48kHz (prime-ish for minimal comb coloration)
const BASE_DELAY_SIZES: [usize; 4] = [4799, 4999, 5399, 5801];
const BASE_SAMPLE_RATE: f32 = 48000.0;

/// Base grain size at 48kHz (~30ms)
const BASE_GRAIN_SIZE: usize = 1440;

/// Simple delay line with circular buffer
struct DelayLine {
    buffer: Vec<f32>,
    size: usize,
    write_pos: usize,
}

impl DelayLine {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size],
            size,
            write_pos: 0,
        }
    }

    #[inline]
    fn read(&self) -> f32 {
        self.buffer[self.write_pos]
    }

    #[inline]
    fn write(&mut self, value: f32) {
        self.buffer[self.write_pos] = value;
        self.write_pos = (self.write_pos + 1) % self.size;
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
    }
}

/// Granular pitch shifter with two crossfading windows
struct GranularPitchShifter {
    buffer: Vec<f32>,
    buffer_size: usize,
    write_pos: usize,
    read_pos_a: f32,
    read_pos_b: f32,
    grain_size: usize,
    phase: f32,
    pitch_ratio: f32,
}

impl GranularPitchShifter {
    fn new(grain_size: usize, pitch_ratio: f32) -> Self {
        let buffer_size = grain_size * 4;
        Self {
            buffer: vec![0.0; buffer_size],
            buffer_size,
            write_pos: 0,
            read_pos_a: 0.0,
            read_pos_b: grain_size as f32 * 0.5,
            grain_size,
            phase: 0.0,
            pitch_ratio,
        }
    }

    fn set_pitch_ratio(&mut self, ratio: f32) {
        self.pitch_ratio = ratio;
    }

    #[inline]
    fn read_interpolated(&self, pos: f32) -> f32 {
        let idx = pos as usize % self.buffer_size;
        let frac = pos - pos.floor();
        let next_idx = (idx + 1) % self.buffer_size;
        self.buffer[idx] * (1.0 - frac) + self.buffer[next_idx] * frac
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        // Write input to circular buffer
        self.buffer[self.write_pos] = input;
        self.write_pos = (self.write_pos + 1) % self.buffer_size;

        // Read from two grains
        let sample_a = self.read_interpolated(self.read_pos_a);
        let sample_b = self.read_interpolated(self.read_pos_b);

        // Crossfade with Hann window (sin^2)
        let fade_a = (self.phase * PI).sin().powi(2);
        let fade_b = ((self.phase + 0.5).fract() * PI).sin().powi(2);

        let output = sample_a * fade_a + sample_b * fade_b;

        // Advance read positions by pitch ratio
        self.read_pos_a += self.pitch_ratio;
        self.read_pos_b += self.pitch_ratio;

        // Wrap read positions within buffer
        if self.read_pos_a >= self.buffer_size as f32 {
            self.read_pos_a -= self.buffer_size as f32;
        }
        if self.read_pos_b >= self.buffer_size as f32 {
            self.read_pos_b -= self.buffer_size as f32;
        }

        // Advance phase
        self.phase += 1.0 / self.grain_size as f32;

        // When a grain wraps, reset its read position near write position
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            // Grain A just completed a cycle, reset it
            self.read_pos_a = if self.write_pos >= 2 {
                (self.write_pos - 2) as f32
            } else {
                (self.buffer_size + self.write_pos - 2) as f32
            };
        } else if self.phase >= 0.5 && self.phase - 1.0 / (self.grain_size as f32) < 0.5 {
            // Grain B just crossed the midpoint, reset it
            self.read_pos_b = if self.write_pos >= 2 {
                (self.write_pos - 2) as f32
            } else {
                (self.buffer_size + self.write_pos - 2) as f32
            };
        }

        output
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
        self.read_pos_a = 0.0;
        self.read_pos_b = self.grain_size as f32 * 0.5;
        self.phase = 0.0;
    }
}

/// Shimmer reverb effect - FDN reverb with pitch-shifted feedback
pub struct ShimmerReverb {
    enabled: bool,
    sample_rate: f32,

    // Parameters
    decay: f32,        // 0.1 - 30.0 seconds
    shimmer: f32,      // 0.0 - 1.0 blend of pitch-shifted vs normal feedback
    pitch: ShimmerPitch,
    damping: f32,      // 0.0 - 1.0
    mix: f32,          // 0.0 - 1.0
    pre_delay_ms: f32, // 0.0 - 100.0 ms

    // FDN delay lines
    delay_lines: [DelayLine; 4],
    delay_sizes: [usize; 4],

    // Damping LPF state (one per delay line)
    damp_state: [f32; 4],

    // Pitch shifters (L and R)
    pitch_shifter_l: GranularPitchShifter,
    pitch_shifter_r: GranularPitchShifter,

    // Pre-delay buffer
    pre_delay_buffer: Vec<f32>,
    pre_delay_size: usize,
    pre_delay_write_pos: usize,
    pre_delay_read_offset: usize,

    // Cached decay factor
    decay_factor: f32,

    // Wet envelope for click-free enable/disable
    wet_target: f32,
    wet_current: f32,
}

impl ShimmerReverb {
    /// Wet envelope smoothing coefficient (~10ms at 48kHz)
    const WET_SMOOTH_COEFF: f32 = 0.9995;

    /// Create a new shimmer reverb effect
    pub fn new(sample_rate: f32) -> Self {
        let scale = sample_rate / BASE_SAMPLE_RATE;

        // Scale delay line sizes for sample rate
        let delay_sizes = [
            (BASE_DELAY_SIZES[0] as f32 * scale) as usize,
            (BASE_DELAY_SIZES[1] as f32 * scale) as usize,
            (BASE_DELAY_SIZES[2] as f32 * scale) as usize,
            (BASE_DELAY_SIZES[3] as f32 * scale) as usize,
        ];

        let delay_lines = [
            DelayLine::new(delay_sizes[0]),
            DelayLine::new(delay_sizes[1]),
            DelayLine::new(delay_sizes[2]),
            DelayLine::new(delay_sizes[3]),
        ];

        // Scale grain size for sample rate
        let grain_size = (BASE_GRAIN_SIZE as f32 * scale) as usize;
        let pitch_ratio = ShimmerPitch::Oct1.ratio();

        let pitch_shifter_l = GranularPitchShifter::new(grain_size, pitch_ratio);
        let pitch_shifter_r = GranularPitchShifter::new(grain_size, pitch_ratio);

        // Pre-delay buffer: max 100ms
        let pre_delay_max = (sample_rate * 0.1) as usize;
        let pre_delay_buffer = vec![0.0; pre_delay_max * 2]; // stereo

        let decay = 3.0;
        let avg_delay_size =
            (delay_sizes[0] + delay_sizes[1] + delay_sizes[2] + delay_sizes[3]) as f32 / 4.0;
        let decay_factor = (-6.9 / (decay * sample_rate / avg_delay_size)).exp();

        let pre_delay_ms = 20.0;
        let pre_delay_read_offset =
            (((pre_delay_ms / 1000.0) * sample_rate) as usize).min(pre_delay_max.saturating_sub(1));

        Self {
            enabled: false,
            sample_rate,
            decay,
            shimmer: 0.5,
            pitch: ShimmerPitch::Oct1,
            damping: 0.5,
            mix: 0.4,
            pre_delay_ms,
            delay_lines,
            delay_sizes,
            damp_state: [0.0; 4],
            pitch_shifter_l,
            pitch_shifter_r,
            pre_delay_buffer,
            pre_delay_size: pre_delay_max,
            pre_delay_write_pos: 0,
            pre_delay_read_offset,
            decay_factor,
            wet_target: 0.0,
            wet_current: 0.0,
        }
    }

    /// Recompute the cached decay factor
    fn update_decay_factor(&mut self) {
        let avg_delay_size = (self.delay_sizes[0]
            + self.delay_sizes[1]
            + self.delay_sizes[2]
            + self.delay_sizes[3]) as f32
            / 4.0;
        self.decay_factor =
            (-6.9 / (self.decay * self.sample_rate / avg_delay_size)).exp();
    }

    /// Set decay time in seconds (0.1 - 30.0)
    pub fn set_decay(&mut self, decay: f32) {
        self.decay = decay.clamp(0.1, 30.0);
        self.update_decay_factor();
    }

    /// Get decay time in seconds
    pub fn decay(&self) -> f32 {
        self.decay
    }

    /// Set shimmer amount (0.0 - 1.0)
    pub fn set_shimmer(&mut self, shimmer: f32) {
        self.shimmer = shimmer.clamp(0.0, 1.0);
    }

    /// Get shimmer amount
    pub fn shimmer(&self) -> f32 {
        self.shimmer
    }

    /// Set pitch shift interval
    pub fn set_pitch(&mut self, pitch: ShimmerPitch) {
        self.pitch = pitch;
        let ratio = pitch.ratio();
        self.pitch_shifter_l.set_pitch_ratio(ratio);
        self.pitch_shifter_r.set_pitch_ratio(ratio);
    }

    /// Get pitch shift interval
    pub fn pitch(&self) -> ShimmerPitch {
        self.pitch
    }

    /// Set damping (0.0 - 1.0)
    pub fn set_damping(&mut self, damping: f32) {
        self.damping = damping.clamp(0.0, 1.0);
    }

    /// Get damping
    pub fn damping(&self) -> f32 {
        self.damping
    }

    /// Set wet/dry mix (0.0 - 1.0)
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    /// Get wet/dry mix
    pub fn mix(&self) -> f32 {
        self.mix
    }

    /// Set pre-delay in milliseconds (0.0 - 100.0)
    pub fn set_pre_delay_ms(&mut self, ms: f32) {
        self.pre_delay_ms = ms.clamp(0.0, 100.0);
        self.pre_delay_read_offset =
            ((self.pre_delay_ms / 1000.0) * self.sample_rate) as usize;
        // Clamp to valid buffer range to prevent out-of-bounds reads
        if self.pre_delay_size > 0 {
            self.pre_delay_read_offset = self.pre_delay_read_offset.min(self.pre_delay_size - 1);
        }
    }

    /// Get pre-delay in milliseconds
    pub fn pre_delay_ms(&self) -> f32 {
        self.pre_delay_ms
    }

    /// Read a stereo frame from the pre-delay buffer
    #[inline]
    fn read_pre_delay(&self) -> (f32, f32) {
        let read_pos = if self.pre_delay_write_pos >= self.pre_delay_read_offset {
            self.pre_delay_write_pos - self.pre_delay_read_offset
        } else {
            self.pre_delay_size - (self.pre_delay_read_offset - self.pre_delay_write_pos)
        };
        let idx = (read_pos % self.pre_delay_size) * 2;
        (self.pre_delay_buffer[idx], self.pre_delay_buffer[idx + 1])
    }

    /// Write a stereo frame to the pre-delay buffer and advance
    #[inline]
    fn write_pre_delay(&mut self, left: f32, right: f32) {
        let idx = self.pre_delay_write_pos * 2;
        self.pre_delay_buffer[idx] = left;
        self.pre_delay_buffer[idx + 1] = right;
        self.pre_delay_write_pos = (self.pre_delay_write_pos + 1) % self.pre_delay_size;
    }
}

impl Effect for ShimmerReverb {
    fn process(&mut self, samples: &mut [f32]) {
        // Skip processing only if fully disabled and envelope has settled
        if !self.enabled && self.wet_current < 0.0001 {
            return;
        }

        let damping = self.damping;
        let shimmer = self.shimmer;
        let decay_factor = self.decay_factor;
        let mix = self.mix;

        for frame in samples.chunks_mut(2) {
            if frame.len() < 2 {
                continue;
            }

            // Smooth wet envelope toward target
            self.wet_current = Self::WET_SMOOTH_COEFF * self.wet_current
                + (1.0 - Self::WET_SMOOTH_COEFF) * self.wet_target;

            let dry_l = frame[0];
            let dry_r = frame[1];

            // Write to pre-delay and read delayed signal
            self.write_pre_delay(dry_l, dry_r);
            let (pd_l, pd_r) = self.read_pre_delay();
            let input_mono = (pd_l + pd_r) * 0.5;

            // Read from all 4 delay lines
            let d0 = self.delay_lines[0].read();
            let d1 = self.delay_lines[1].read();
            let d2 = self.delay_lines[2].read();
            let d3 = self.delay_lines[3].read();

            // Hadamard-like mixing matrix
            let out0 = (d0 + d1 + d2 + d3) * 0.5;
            let out1 = (d0 + d1 - d2 - d3) * 0.5;
            let out2 = (d0 - d1 + d2 - d3) * 0.5;
            let out3 = (d0 - d1 - d2 + d3) * 0.5;

            // Apply damping LPF to each output
            self.damp_state[0] =
                self.damp_state[0] * damping + out0 * (1.0 - damping);
            self.damp_state[1] =
                self.damp_state[1] * damping + out1 * (1.0 - damping);
            self.damp_state[2] =
                self.damp_state[2] * damping + out2 * (1.0 - damping);
            self.damp_state[3] =
                self.damp_state[3] * damping + out3 * (1.0 - damping);

            let damped0 = self.damp_state[0];
            let damped1 = self.damp_state[1];
            let damped2 = self.damp_state[2];
            let damped3 = self.damp_state[3];

            // Pitch-shift blend for feedback (L uses lines 0+1, R uses lines 2+3)
            let fb_input_l = (damped0 + damped1) * 0.5;
            let fb_input_r = (damped2 + damped3) * 0.5;

            let shifted_l = self.pitch_shifter_l.process(fb_input_l);
            let shifted_r = self.pitch_shifter_r.process(fb_input_r);

            // Blend normal and pitch-shifted feedback
            let feedback_l =
                fb_input_l * (1.0 - shimmer) + shifted_l * shimmer;
            let feedback_r =
                fb_input_r * (1.0 - shimmer) + shifted_r * shimmer;

            // Apply decay and write back to delay lines with input
            // Each line gets a unique Hadamard-derived feedback combination
            // to maximize diffusion (avoid paired lines converging)
            let fb_sum = (feedback_l + feedback_r) * 0.5;
            let fb_diff = (feedback_l - feedback_r) * 0.5;
            self.delay_lines[0]
                .write(input_mono + fb_sum * decay_factor);
            self.delay_lines[1]
                .write(input_mono + fb_diff * decay_factor);
            self.delay_lines[2]
                .write(input_mono + feedback_r * decay_factor);
            self.delay_lines[3]
                .write(input_mono + feedback_l * decay_factor);

            // Mix to stereo: L = line0 + line1, R = line2 + line3
            let wet_l = damped0 + damped1;
            let wet_r = damped2 + damped3;

            // Apply wet/dry mix with envelope
            let effective_mix = mix * self.wet_current;
            frame[0] = dry_l * (1.0 - effective_mix) + wet_l * effective_mix;
            frame[1] = dry_r * (1.0 - effective_mix) + wet_r * effective_mix;
        }
    }

    fn reset(&mut self) {
        for dl in &mut self.delay_lines {
            dl.reset();
        }
        self.damp_state = [0.0; 4];
        self.pitch_shifter_l.reset();
        self.pitch_shifter_r.reset();
        self.pre_delay_buffer.fill(0.0);
        self.pre_delay_write_pos = 0;
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.wet_target = if enabled { 1.0 } else { 0.0 };
        // Don't reset on disable - let shimmer tails naturally fade out
    }

    fn name(&self) -> &'static str {
        "Shimmer"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shimmer_creation() {
        let shimmer = ShimmerReverb::new(48000.0);
        assert!(!shimmer.is_enabled());
        assert_eq!(shimmer.decay(), 3.0);
        assert_eq!(shimmer.shimmer(), 0.5);
        assert_eq!(shimmer.pitch(), ShimmerPitch::Oct1);
        assert_eq!(shimmer.damping(), 0.5);
        assert_eq!(shimmer.mix(), 0.4);
        assert_eq!(shimmer.pre_delay_ms(), 20.0);
        assert_eq!(shimmer.name(), "Shimmer");
    }

    #[test]
    fn test_shimmer_parameter_clamping() {
        let mut shimmer = ShimmerReverb::new(48000.0);

        shimmer.set_decay(0.01);
        assert_eq!(shimmer.decay(), 0.1);
        shimmer.set_decay(50.0);
        assert_eq!(shimmer.decay(), 30.0);

        shimmer.set_shimmer(-0.5);
        assert_eq!(shimmer.shimmer(), 0.0);
        shimmer.set_shimmer(1.5);
        assert_eq!(shimmer.shimmer(), 1.0);

        shimmer.set_damping(-0.1);
        assert_eq!(shimmer.damping(), 0.0);
        shimmer.set_damping(1.2);
        assert_eq!(shimmer.damping(), 1.0);

        shimmer.set_mix(-0.1);
        assert_eq!(shimmer.mix(), 0.0);
        shimmer.set_mix(1.5);
        assert_eq!(shimmer.mix(), 1.0);

        shimmer.set_pre_delay_ms(-10.0);
        assert_eq!(shimmer.pre_delay_ms(), 0.0);
        shimmer.set_pre_delay_ms(200.0);
        assert_eq!(shimmer.pre_delay_ms(), 100.0);
    }

    #[test]
    fn test_shimmer_pitch_ratios() {
        assert_eq!(ShimmerPitch::Oct1.ratio(), 2.0);
        assert_eq!(ShimmerPitch::Oct2.ratio(), 4.0);
        assert_eq!(ShimmerPitch::Fifth.ratio(), 1.5);
        assert_eq!(ShimmerPitch::Oct1Down.ratio(), 0.5);
    }

    #[test]
    fn test_shimmer_bypass_when_disabled() {
        let mut shimmer = ShimmerReverb::new(48000.0);
        let mut samples = vec![0.5, 0.5, 0.3, 0.3, 0.1, 0.1];
        let original = samples.clone();
        shimmer.process(&mut samples);

        // Disabled with wet_current at 0 should pass through unchanged
        assert_eq!(samples, original);
    }

    #[test]
    fn test_shimmer_processes_audio() {
        let mut shimmer = ShimmerReverb::new(48000.0);
        shimmer.set_enabled(true);
        shimmer.wet_current = 1.0; // Force wet for test

        // Process a chunk of audio
        let mut samples = vec![1.0; 2048];
        shimmer.process(&mut samples);

        // Should not produce NaN or infinity
        assert!(
            samples.iter().all(|s| s.is_finite()),
            "Output contains non-finite values"
        );
    }

    #[test]
    fn test_shimmer_all_pitch_modes() {
        for pitch in [
            ShimmerPitch::Oct1,
            ShimmerPitch::Oct2,
            ShimmerPitch::Fifth,
            ShimmerPitch::Oct1Down,
        ] {
            let mut shimmer = ShimmerReverb::new(48000.0);
            shimmer.set_pitch(pitch);
            shimmer.set_enabled(true);
            shimmer.wet_current = 1.0;

            let mut samples = vec![0.5; 1024];
            shimmer.process(&mut samples);

            assert!(
                samples.iter().all(|s| s.is_finite()),
                "Non-finite output with pitch {:?}",
                pitch
            );
        }
    }

    #[test]
    fn test_shimmer_reset() {
        let mut shimmer = ShimmerReverb::new(48000.0);
        shimmer.set_enabled(true);
        shimmer.wet_current = 1.0;

        // Process some audio to fill buffers
        let mut samples = vec![1.0; 4096];
        shimmer.process(&mut samples);

        shimmer.reset();

        // After reset, processing silence should produce silence
        let mut silence = vec![0.0; 2048];
        shimmer.process(&mut silence);

        // All outputs should be very close to zero (within envelope smoothing)
        let max_val = silence.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(
            max_val < 0.01,
            "Expected near-silence after reset, got max {}",
            max_val
        );
    }

    #[test]
    fn test_shimmer_different_sample_rates() {
        for &sr in &[44100.0, 48000.0, 96000.0] {
            let mut shimmer = ShimmerReverb::new(sr);
            shimmer.set_enabled(true);
            shimmer.wet_current = 1.0;

            let mut samples = vec![0.5; 1024];
            shimmer.process(&mut samples);

            assert!(
                samples.iter().all(|s| s.is_finite()),
                "Non-finite output at sample rate {}",
                sr
            );
        }
    }

    #[test]
    fn test_shimmer_wet_envelope() {
        let mut shimmer = ShimmerReverb::new(48000.0);

        // Enable - wet_target should be 1.0
        shimmer.set_enabled(true);
        assert_eq!(shimmer.wet_target, 1.0);
        assert!(shimmer.is_enabled());

        // Disable - wet_target should be 0.0
        shimmer.set_enabled(false);
        assert_eq!(shimmer.wet_target, 0.0);
        assert!(!shimmer.is_enabled());
    }

    #[test]
    fn test_shimmer_extreme_decay() {
        let mut shimmer = ShimmerReverb::new(48000.0);
        shimmer.set_decay(0.1); // Very short
        shimmer.set_enabled(true);
        shimmer.wet_current = 1.0;

        let mut samples = vec![1.0; 2048];
        shimmer.process(&mut samples);
        assert!(samples.iter().all(|s| s.is_finite()));

        shimmer.reset();
        shimmer.set_decay(30.0); // Very long
        let mut samples = vec![1.0; 2048];
        shimmer.process(&mut samples);
        assert!(samples.iter().all(|s| s.is_finite()));
    }
}
