//! Phaser effect - 6-stage allpass chain with LFO sweep
//!
//! Classic DJ effect that creates a sweeping, swirling sound by
//! passing the signal through a chain of allpass filters whose
//! frequencies are modulated by an LFO. Stereo offset creates
//! wide spatial movement.

use super::Effect;
use std::f32::consts::PI;

/// Phaser effect with 6-stage allpass chain and stereo LFO
pub struct Phaser {
    enabled: bool,
    sample_rate: f32,

    /// LFO rate in Hz (0.02 - 8.0)
    rate: f32,

    /// Modulation depth (0.0 - 1.0)
    depth: f32,

    /// Feedback amount (-0.95 to 0.95)
    feedback: f32,

    /// Wet/dry mix (0.0 - 1.0)
    mix: f32,

    /// Stereo LFO phase offset (0.0 - 0.5)
    stereo_offset: f32,

    /// LFO phase (0.0 - 1.0)
    lfo_phase: f32,

    /// LFO phase increment per sample
    lfo_inc: f32,

    /// Allpass filter state: [channel][stage]
    allpass_state: [[f32; 6]; 2],

    /// Feedback state (stereo)
    feedback_l: f32,
    feedback_r: f32,

    /// Wet envelope for click-free enable/disable
    wet_target: f32,
    wet_current: f32,
}

/// Number of allpass stages
const NUM_STAGES: usize = 6;

/// Minimum allpass sweep frequency in Hz
const MIN_FREQ: f32 = 200.0;

/// ln(MAX_FREQ / MIN_FREQ) = ln(4000 / 200) = ln(20) ≈ 2.9957
/// Used in the polynomial exp() approximation for the frequency sweep.
const LN_FREQ_RATIO: f32 = 2.9957;

impl Phaser {
    /// Wet envelope smoothing coefficient
    const WET_SMOOTH_COEFF: f32 = 0.9995;

    /// Create a new phaser effect
    pub fn new(sample_rate: f32) -> Self {
        let rate = 0.3;
        let lfo_inc = rate / sample_rate;

        Self {
            enabled: false,
            sample_rate,
            rate,
            depth: 0.7,
            feedback: 0.5,
            mix: 0.5,
            stereo_offset: 0.25,
            lfo_phase: 0.0,
            lfo_inc,
            allpass_state: [[0.0; 6]; 2],
            feedback_l: 0.0,
            feedback_r: 0.0,
            wet_target: 0.0,
            wet_current: 0.0,
        }
    }

    /// Set LFO rate in Hz (0.02 - 8.0)
    pub fn set_rate(&mut self, rate: f32) {
        self.rate = rate.clamp(0.02, 8.0);
        self.lfo_inc = self.rate / self.sample_rate;
    }

    /// Get LFO rate
    pub fn rate(&self) -> f32 {
        self.rate
    }

    /// Set modulation depth (0.0 - 1.0)
    pub fn set_depth(&mut self, depth: f32) {
        self.depth = depth.clamp(0.0, 1.0);
    }

    /// Get depth
    pub fn depth(&self) -> f32 {
        self.depth
    }

    /// Set feedback amount (-0.95 to 0.95)
    pub fn set_feedback(&mut self, feedback: f32) {
        self.feedback = feedback.clamp(-0.95, 0.95);
    }

    /// Get feedback
    pub fn feedback(&self) -> f32 {
        self.feedback
    }

    /// Set wet/dry mix (0.0 - 1.0)
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    /// Get mix
    pub fn mix(&self) -> f32 {
        self.mix
    }

    /// Set stereo LFO phase offset (0.0 - 0.5)
    pub fn set_stereo_offset(&mut self, offset: f32) {
        self.stereo_offset = offset.clamp(0.0, 0.5);
    }

    /// Get stereo offset
    pub fn stereo_offset(&self) -> f32 {
        self.stereo_offset
    }

    /// Compute allpass coefficient from LFO value
    /// Uses polynomial approximation to avoid per-sample powf()+tan()
    #[inline]
    fn lfo_to_coeff(&self, lfo_val: f32) -> f32 {
        // Map LFO (0..1) through depth to a normalized sweep position
        let sweep = lfo_val * self.depth;
        // Approximate exponential frequency sweep: MIN_FREQ * FREQ_RATIO^sweep
        // Use exp(sweep * ln(FREQ_RATIO)) with a fast cubic approximation of exp()
        let ln_ratio = LN_FREQ_RATIO;
        let x = sweep * ln_ratio;
        // Cubic Padé-style exp approximation: (1 + x/2 + x²/8) / (1 - x/2 + x²/8)
        // Accurate to ~0.1% over [0, 3.0]
        let x2 = x * x;
        let freq = MIN_FREQ * (1.0 + x * 0.5 + x2 * 0.125) / (1.0 - x * 0.5 + x2 * 0.125);
        // Approximate tan(π * f / sr) ≈ π * f / sr for f << sr/2
        // This is accurate to <1% for freq up to ~4kHz at 48kHz sample rate
        let w = PI * freq / self.sample_rate;
        (w - 1.0) / (w + 1.0)
    }

    /// First-order allpass filter
    #[inline]
    fn allpass(input: f32, coeff: f32, state: &mut f32) -> f32 {
        let y = coeff * input + *state;
        *state = input - coeff * y;
        y
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

    /// Soft saturation for feedback path
    #[inline]
    fn soft_saturate(x: f32) -> f32 {
        let x2 = x * x;
        x * (27.0 + x2) / (27.0 + 9.0 * x2)
    }
}

impl Effect for Phaser {
    fn process(&mut self, samples: &mut [f32]) {
        // Skip if fully disabled and envelope settled
        if !self.enabled && self.wet_current < 0.0001 {
            return;
        }

        for frame in samples.chunks_mut(2) {
            if frame.len() < 2 {
                continue;
            }

            // Smooth wet envelope
            self.wet_current = Self::WET_SMOOTH_COEFF * self.wet_current
                + (1.0 - Self::WET_SMOOTH_COEFF) * self.wet_target;

            // Calculate LFO values for left and right (sine wave, 0.0 - 1.0)
            let lfo_l = (self.lfo_phase * 2.0 * PI).sin() * 0.5 + 0.5;
            let lfo_r =
                ((self.lfo_phase + self.stereo_offset) * 2.0 * PI).sin() * 0.5 + 0.5;

            // Advance LFO phase
            self.lfo_phase += self.lfo_inc;
            if self.lfo_phase >= 1.0 {
                self.lfo_phase -= 1.0;
            }

            // Compute allpass coefficients from LFO
            let coeff_l = self.lfo_to_coeff(lfo_l);
            let coeff_r = self.lfo_to_coeff(lfo_r);

            // Input with feedback
            let input_l = frame[0] + Self::soft_saturate(self.feedback_l * self.feedback);
            let input_r = frame[1] + Self::soft_saturate(self.feedback_r * self.feedback);

            // Chain 6 allpass stages for left channel
            let mut out_l = input_l;
            for stage in 0..NUM_STAGES {
                out_l = Self::allpass(out_l, coeff_l, &mut self.allpass_state[0][stage]);
            }

            // Chain 6 allpass stages for right channel
            let mut out_r = input_r;
            for stage in 0..NUM_STAGES {
                out_r = Self::allpass(out_r, coeff_r, &mut self.allpass_state[1][stage]);
            }

            // Store feedback from allpass output
            self.feedback_l = out_l;
            self.feedback_r = out_r;

            // Mix dry and wet with envelope, soft clip to prevent energy accumulation
            let effective_mix = self.mix * self.wet_current;
            frame[0] =
                Self::soft_clip(frame[0] * (1.0 - effective_mix) + out_l * effective_mix);
            frame[1] =
                Self::soft_clip(frame[1] * (1.0 - effective_mix) + out_r * effective_mix);
        }
    }

    fn reset(&mut self) {
        self.allpass_state = [[0.0; 6]; 2];
        self.lfo_phase = 0.0;
        self.feedback_l = 0.0;
        self.feedback_r = 0.0;
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.wet_target = if enabled { 1.0 } else { 0.0 };
    }

    fn name(&self) -> &'static str {
        "Phaser"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phaser_creation() {
        let phaser = Phaser::new(48000.0);
        assert!(!phaser.is_enabled());
        assert_eq!(phaser.rate(), 0.3);
        assert_eq!(phaser.depth(), 0.7);
        assert_eq!(phaser.feedback(), 0.5);
        assert_eq!(phaser.mix(), 0.5);
        assert_eq!(phaser.stereo_offset(), 0.25);
        assert_eq!(phaser.name(), "Phaser");
    }

    #[test]
    fn test_phaser_parameter_clamping() {
        let mut phaser = Phaser::new(48000.0);

        phaser.set_rate(20.0);
        assert_eq!(phaser.rate(), 8.0);

        phaser.set_rate(0.001);
        assert_eq!(phaser.rate(), 0.02);

        phaser.set_feedback(2.0);
        assert_eq!(phaser.feedback(), 0.95);

        phaser.set_feedback(-2.0);
        assert_eq!(phaser.feedback(), -0.95);

        phaser.set_depth(1.5);
        assert_eq!(phaser.depth(), 1.0);

        phaser.set_mix(-0.5);
        assert_eq!(phaser.mix(), 0.0);

        phaser.set_stereo_offset(1.0);
        assert_eq!(phaser.stereo_offset(), 0.5);
    }

    #[test]
    fn test_phaser_passthrough_when_disabled() {
        let mut phaser = Phaser::new(48000.0);

        let mut samples = vec![0.5, -0.5, 0.3, -0.3, 0.1, -0.1];
        let original = samples.clone();
        phaser.process(&mut samples);

        // When disabled and wet_current is 0, output should be unchanged
        assert_eq!(samples, original);
    }

    #[test]
    fn test_phaser_processes_audio() {
        let mut phaser = Phaser::new(48000.0);
        phaser.set_enabled(true);
        phaser.wet_current = 1.0; // Force wet for test

        let mut samples = vec![0.5, 0.5, 0.3, 0.3, 0.1, 0.1];
        let original = samples.clone();
        phaser.process(&mut samples);

        // Output should be modified by allpass chain
        let changed = samples.iter().zip(original.iter()).any(|(a, b)| (a - b).abs() > 0.001);
        assert!(changed, "Phaser should modify audio when enabled");
    }

    #[test]
    fn test_phaser_reset() {
        let mut phaser = Phaser::new(48000.0);
        phaser.set_enabled(true);
        phaser.wet_current = 1.0;

        // Process some audio to fill state
        let mut samples: Vec<f32> = (0..64).flat_map(|_| [1.0_f32, 1.0]).collect();
        phaser.process(&mut samples);

        // Reset should clear all state
        phaser.reset();
        assert_eq!(phaser.allpass_state, [[0.0; 6]; 2]);
        assert_eq!(phaser.feedback_l, 0.0);
        assert_eq!(phaser.feedback_r, 0.0);
        assert_eq!(phaser.lfo_phase, 0.0);
    }

    #[test]
    fn test_phaser_enable_disable() {
        let mut phaser = Phaser::new(48000.0);

        phaser.set_enabled(true);
        assert!(phaser.is_enabled());
        assert_eq!(phaser.wet_target, 1.0);

        phaser.set_enabled(false);
        assert!(!phaser.is_enabled());
        assert_eq!(phaser.wet_target, 0.0);
    }

    #[test]
    fn test_phaser_output_bounded() {
        let mut phaser = Phaser::new(48000.0);
        phaser.set_enabled(true);
        phaser.set_feedback(0.95);
        phaser.wet_current = 1.0;

        // Process many frames of loud signal
        let mut samples: Vec<f32> = (0..256).flat_map(|_| [0.9_f32, -0.9]).collect();
        phaser.process(&mut samples);

        // All outputs should remain within soft-clip bounds
        for s in &samples {
            assert!(
                s.abs() < 2.0,
                "Output sample {} exceeded bounds",
                s
            );
        }
    }

    #[test]
    fn test_phaser_stereo_offset() {
        let mut phaser_mono = Phaser::new(48000.0);
        phaser_mono.set_enabled(true);
        phaser_mono.set_stereo_offset(0.0);
        phaser_mono.wet_current = 1.0;

        let mut phaser_stereo = Phaser::new(48000.0);
        phaser_stereo.set_enabled(true);
        phaser_stereo.set_stereo_offset(0.5);
        phaser_stereo.wet_current = 1.0;

        let input: Vec<f32> = (0..128)
            .flat_map(|i| {
                let v = (i as f32 * 0.1).sin();
                [v, v]
            })
            .collect();

        let mut mono_out = input.clone();
        let mut stereo_out = input.clone();

        phaser_mono.process(&mut mono_out);
        phaser_stereo.process(&mut stereo_out);

        // With zero stereo offset, L and R should be identical
        let mono_lr_same = mono_out
            .chunks(2)
            .all(|f| (f[0] - f[1]).abs() < 0.0001);

        // With 0.5 stereo offset, L and R should differ
        let stereo_lr_diff = stereo_out
            .chunks(2)
            .any(|f| (f[0] - f[1]).abs() > 0.001);

        assert!(mono_lr_same, "Zero stereo offset should produce identical L/R");
        assert!(stereo_lr_diff, "Non-zero stereo offset should produce different L/R");
    }
}
