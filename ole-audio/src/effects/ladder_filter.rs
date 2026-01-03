//! Moog-style 4-pole ladder filter with analog warmth
//!
//! Based on the Huovilainen model for accurate Moog emulation.
//! Features:
//! - 4-pole cascade (-24dB/octave rolloff)
//! - Nonlinear saturation for analog character
//! - Self-oscillation at high resonance
//! - Parameter smoothing to prevent zipper noise

use super::Effect;
use std::f32::consts::PI;

/// Moog-style ladder filter
pub struct LadderFilter {
    enabled: bool,
    sample_rate: f32,

    // Parameters
    cutoff: f32,      // Hz (20-20000)
    resonance: f32,   // 0.0-1.0 (self-oscillation at ~0.95)
    drive: f32,       // Input saturation 0.0-1.0

    // 4-pole filter state (stereo)
    stage_l: [f32; 4],
    stage_r: [f32; 4],
    delay_l: [f32; 4],
    delay_r: [f32; 4],

    // Feedback delay for resonance
    feedback_l: f32,
    feedback_r: f32,

    // Parameter smoothing (prevents zipper noise)
    cutoff_smooth: f32,
    resonance_smooth: f32,
    smoothing_coeff: f32,

    // Thermal noise simulation (subtle random variations)
    thermal_l: f32,
    thermal_r: f32,
}

impl LadderFilter {
    /// Create a new ladder filter
    pub fn new(sample_rate: f32) -> Self {
        Self {
            enabled: false,
            sample_rate,
            cutoff: 1000.0,
            resonance: 0.0,
            drive: 0.0,
            stage_l: [0.0; 4],
            stage_r: [0.0; 4],
            delay_l: [0.0; 4],
            delay_r: [0.0; 4],
            feedback_l: 0.0,
            feedback_r: 0.0,
            cutoff_smooth: 1000.0,
            resonance_smooth: 0.0,
            // Smoothing coefficient for ~5ms transition at 48kHz
            smoothing_coeff: 1.0 - (-1.0 / (0.005 * sample_rate)).exp(),
            thermal_l: 0.0,
            thermal_r: 0.0,
        }
    }

    /// Set cutoff frequency (20-20000 Hz)
    pub fn set_cutoff(&mut self, cutoff: f32) {
        self.cutoff = cutoff.clamp(20.0, 20000.0);
    }

    /// Get cutoff frequency
    pub fn cutoff(&self) -> f32 {
        self.cutoff
    }

    /// Set resonance (0.0-1.0)
    /// Values above ~0.95 will cause self-oscillation
    pub fn set_resonance(&mut self, resonance: f32) {
        self.resonance = resonance.clamp(0.0, 1.0);
    }

    /// Get resonance
    pub fn resonance(&self) -> f32 {
        self.resonance
    }

    /// Set drive (input saturation, 0.0-1.0)
    pub fn set_drive(&mut self, drive: f32) {
        self.drive = drive.clamp(0.0, 1.0);
    }

    /// Get drive
    pub fn drive(&self) -> f32 {
        self.drive
    }

    /// Fast tanh approximation (no libm dependency)
    /// Uses rational function: x * (27 + x²) / (27 + 9x²)
    #[inline]
    fn fast_tanh(x: f32) -> f32 {
        let x2 = x * x;
        x * (27.0 + x2) / (27.0 + 9.0 * x2)
    }

    /// Soft saturation for analog warmth
    #[inline]
    fn saturate(x: f32, amount: f32) -> f32 {
        if amount < 0.001 {
            return x;
        }
        let driven = x * (1.0 + amount * 3.0);
        Self::fast_tanh(driven)
    }

    /// Smooth parameter towards target
    #[inline]
    fn smooth(&self, current: f32, target: f32) -> f32 {
        current + (target - current) * self.smoothing_coeff
    }

    /// Process a single sample through the 4-pole ladder
    #[inline]
    fn process_sample(&mut self, input: f32, is_right: bool) -> f32 {
        // Select channel state
        let (stage, delay, feedback, thermal) = if is_right {
            (&mut self.stage_r, &mut self.delay_r, &mut self.feedback_r, &mut self.thermal_r)
        } else {
            (&mut self.stage_l, &mut self.delay_l, &mut self.feedback_l, &mut self.thermal_l)
        };

        // Calculate normalized frequency (0-1, where 1 = Nyquist)
        let fc = (self.cutoff_smooth / self.sample_rate).min(0.49);

        // Huovilainen's frequency warping for better high-frequency accuracy
        let fc_warped = fc * 1.16;
        let g = fc_warped * PI;
        let g_comp = g / (1.0 + g);

        // Resonance compensation (prevent volume drop at high resonance)
        let res_scale = 1.0 + self.resonance_smooth * 0.5;

        // Apply drive/saturation to input
        let input_driven = Self::saturate(input * (1.0 + self.drive * 2.0), self.drive);

        // Add subtle thermal noise for analog character
        *thermal = *thermal * 0.99 + (input_driven * 0.001);
        let thermal_noise = *thermal * 0.0001;

        // Resonance feedback (scaled for self-oscillation behavior)
        let resonance_feedback = self.resonance_smooth * 4.0 * res_scale;
        let feedback_signal = Self::fast_tanh(*feedback * resonance_feedback);

        // Input with resonance feedback subtracted
        let u = input_driven - feedback_signal + thermal_noise;

        // 4-pole cascade with nonlinear saturation between stages
        // Each stage is a one-pole lowpass filter
        stage[0] = g_comp * (Self::fast_tanh(u) - Self::fast_tanh(delay[0])) + delay[0];
        delay[0] = stage[0];

        stage[1] = g_comp * (Self::fast_tanh(stage[0]) - Self::fast_tanh(delay[1])) + delay[1];
        delay[1] = stage[1];

        stage[2] = g_comp * (Self::fast_tanh(stage[1]) - Self::fast_tanh(delay[2])) + delay[2];
        delay[2] = stage[2];

        stage[3] = g_comp * (Self::fast_tanh(stage[2]) - Self::fast_tanh(delay[3])) + delay[3];
        delay[3] = stage[3];

        // Update feedback for next sample (one-sample delay)
        *feedback = stage[3];

        // Output with gain compensation
        stage[3] / res_scale
    }

    /// Smooth parameters at the start of buffer processing
    fn update_smooth_params(&mut self) {
        self.cutoff_smooth = self.smooth(self.cutoff_smooth, self.cutoff);
        self.resonance_smooth = self.smooth(self.resonance_smooth, self.resonance);
    }
}

impl Effect for LadderFilter {
    fn process(&mut self, samples: &mut [f32]) {
        if !self.enabled {
            return;
        }

        // Update smoothed parameters
        self.update_smooth_params();

        for frame in samples.chunks_mut(2) {
            if frame.len() == 2 {
                frame[0] = self.process_sample(frame[0], false);
                frame[1] = self.process_sample(frame[1], true);
            }
        }
    }

    fn reset(&mut self) {
        self.stage_l = [0.0; 4];
        self.stage_r = [0.0; 4];
        self.delay_l = [0.0; 4];
        self.delay_r = [0.0; 4];
        self.feedback_l = 0.0;
        self.feedback_r = 0.0;
        self.thermal_l = 0.0;
        self.thermal_r = 0.0;
        self.cutoff_smooth = self.cutoff;
        self.resonance_smooth = self.resonance;
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.reset();
        }
    }

    fn name(&self) -> &'static str {
        "Ladder Filter"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ladder_filter_creation() {
        let filter = LadderFilter::new(48000.0);
        assert!(!filter.is_enabled());
        assert_eq!(filter.cutoff(), 1000.0);
        assert_eq!(filter.resonance(), 0.0);
    }

    #[test]
    fn test_ladder_filter_parameter_clamping() {
        let mut filter = LadderFilter::new(48000.0);

        filter.set_cutoff(10.0);
        assert_eq!(filter.cutoff(), 20.0);

        filter.set_cutoff(30000.0);
        assert_eq!(filter.cutoff(), 20000.0);

        filter.set_resonance(-0.5);
        assert_eq!(filter.resonance(), 0.0);

        filter.set_resonance(1.5);
        assert_eq!(filter.resonance(), 1.0);
    }

    #[test]
    fn test_ladder_filter_processes_audio() {
        let mut filter = LadderFilter::new(48000.0);
        filter.set_enabled(true);
        filter.set_cutoff(1000.0);
        filter.set_resonance(0.5);

        // Test with a simple sine-like input
        let mut samples = vec![0.5, 0.5, 0.3, 0.3, 0.1, 0.1, -0.1, -0.1];
        filter.process(&mut samples);

        // Output should be modified (not equal to input)
        assert_ne!(samples[0], 0.5);
    }

    #[test]
    fn test_fast_tanh() {
        // Fast tanh should approximate real tanh reasonably well
        assert!((LadderFilter::fast_tanh(0.0) - 0.0).abs() < 0.01);
        assert!((LadderFilter::fast_tanh(1.0) - 0.7615941).abs() < 0.05);
        assert!((LadderFilter::fast_tanh(-1.0) - (-0.7615941)).abs() < 0.05);
    }
}
