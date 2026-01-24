//! 3-band mastering EQ
//!
//! Subtle frequency shaping using biquad shelving and peaking filters.
//! Uses RBJ Audio EQ Cookbook formulas for high-quality filtering.
//!
//! Bands:
//! - Low shelf: 80-120Hz, ±3dB (default: +1dB)
//! - Mid bell: 2-4kHz, ±3dB (default: 0dB)
//! - High shelf: 10-14kHz, ±3dB (default: +0.5dB)

use std::f32::consts::PI;

use crate::effects::Effect;

/// Biquad filter coefficients
#[derive(Clone, Copy, Default)]
struct BiquadCoeffs {
    a0: f32,
    a1: f32,
    a2: f32,
    b1: f32,
    b2: f32,
}

/// Biquad filter state for a single channel
#[derive(Default, Clone)]
struct BiquadState {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl BiquadState {
    #[inline]
    fn process(&mut self, input: f32, coeffs: &BiquadCoeffs) -> f32 {
        let output = coeffs.a0 * input + coeffs.a1 * self.x1 + coeffs.a2 * self.x2
            - coeffs.b1 * self.y1
            - coeffs.b2 * self.y2;

        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = output;

        output
    }

    fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

/// 3-band mastering EQ
pub struct MasteringEQ {
    enabled: bool,
    sample_rate: f32,

    // Parameters
    low_freq: f32,  // Low shelf frequency (Hz)
    low_gain: f32,  // Low shelf gain (dB)
    mid_freq: f32,  // Mid bell frequency (Hz)
    mid_gain: f32,  // Mid bell gain (dB)
    mid_q: f32,     // Mid bell Q
    high_freq: f32, // High shelf frequency (Hz)
    high_gain: f32, // High shelf gain (dB)

    // Smoothed parameters (for click-free adjustment)
    low_gain_smooth: f32,
    mid_gain_smooth: f32,
    high_gain_smooth: f32,
    smooth_coeff: f32,

    // Coefficients
    low_coeffs: BiquadCoeffs,
    mid_coeffs: BiquadCoeffs,
    high_coeffs: BiquadCoeffs,

    // State (stereo)
    low_state_l: BiquadState,
    low_state_r: BiquadState,
    mid_state_l: BiquadState,
    mid_state_r: BiquadState,
    high_state_l: BiquadState,
    high_state_r: BiquadState,

    // Flag to recalculate coefficients
    needs_update: bool,
}

impl MasteringEQ {
    /// Create a new mastering EQ
    pub fn new(sample_rate: f32) -> Self {
        // Smoothing coefficient for ~5ms at sample rate
        let smooth_coeff = 1.0 - (-1.0 / (sample_rate * 0.005)).exp();

        let mut eq = Self {
            enabled: true,
            sample_rate,
            low_freq: 100.0,
            low_gain: 1.0, // +1dB default
            mid_freq: 3000.0,
            mid_gain: 0.0,
            mid_q: 0.7,
            high_freq: 12000.0,
            high_gain: 0.5, // +0.5dB default
            low_gain_smooth: 1.0,
            mid_gain_smooth: 0.0,
            high_gain_smooth: 0.5,
            smooth_coeff,
            low_coeffs: BiquadCoeffs::default(),
            mid_coeffs: BiquadCoeffs::default(),
            high_coeffs: BiquadCoeffs::default(),
            low_state_l: BiquadState::default(),
            low_state_r: BiquadState::default(),
            mid_state_l: BiquadState::default(),
            mid_state_r: BiquadState::default(),
            high_state_l: BiquadState::default(),
            high_state_r: BiquadState::default(),
            needs_update: true,
        };
        eq.update_coefficients();
        eq
    }

    /// Set low shelf gain in dB (±3dB range)
    pub fn set_low_gain(&mut self, gain_db: f32) {
        self.low_gain = gain_db.clamp(-3.0, 3.0);
        self.needs_update = true;
    }

    /// Set low shelf frequency (80-120Hz)
    pub fn set_low_freq(&mut self, freq: f32) {
        self.low_freq = freq.clamp(80.0, 120.0);
        self.needs_update = true;
    }

    /// Set mid bell gain in dB (±3dB range)
    pub fn set_mid_gain(&mut self, gain_db: f32) {
        self.mid_gain = gain_db.clamp(-3.0, 3.0);
        self.needs_update = true;
    }

    /// Set mid bell frequency (2-4kHz)
    pub fn set_mid_freq(&mut self, freq: f32) {
        self.mid_freq = freq.clamp(2000.0, 4000.0);
        self.needs_update = true;
    }

    /// Set mid bell Q (0.5-2.0)
    pub fn set_mid_q(&mut self, q: f32) {
        self.mid_q = q.clamp(0.5, 2.0);
        self.needs_update = true;
    }

    /// Set high shelf gain in dB (±3dB range)
    pub fn set_high_gain(&mut self, gain_db: f32) {
        self.high_gain = gain_db.clamp(-3.0, 3.0);
        self.needs_update = true;
    }

    /// Set high shelf frequency (10-14kHz)
    pub fn set_high_freq(&mut self, freq: f32) {
        self.high_freq = freq.clamp(10000.0, 14000.0);
        self.needs_update = true;
    }

    /// Get current low gain
    pub fn low_gain(&self) -> f32 {
        self.low_gain
    }

    /// Get current mid gain
    pub fn mid_gain(&self) -> f32 {
        self.mid_gain
    }

    /// Get current high gain
    pub fn high_gain(&self) -> f32 {
        self.high_gain
    }

    /// Update filter coefficients from parameters
    fn update_coefficients(&mut self) {
        self.low_coeffs = self.calc_low_shelf_coeffs(self.low_freq, self.low_gain_smooth);
        self.mid_coeffs = self.calc_peaking_coeffs(self.mid_freq, self.mid_gain_smooth, self.mid_q);
        self.high_coeffs = self.calc_high_shelf_coeffs(self.high_freq, self.high_gain_smooth);
        self.needs_update = false;
    }

    /// Calculate low shelf filter coefficients (RBJ cookbook)
    fn calc_low_shelf_coeffs(&self, freq: f32, gain_db: f32) -> BiquadCoeffs {
        if gain_db.abs() < 0.01 {
            // Bypass - unity gain
            return BiquadCoeffs {
                a0: 1.0,
                a1: 0.0,
                a2: 0.0,
                b1: 0.0,
                b2: 0.0,
            };
        }

        let a = 10.0f32.powf(gain_db / 40.0); // sqrt(10^(dB/20))
        let omega = 2.0 * PI * freq / self.sample_rate;
        let sin_omega = omega.sin();
        let cos_omega = omega.cos();
        let alpha = sin_omega / 2.0 * (2.0f32).sqrt(); // Slope = 1
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let a0 = (a + 1.0) + (a - 1.0) * cos_omega + two_sqrt_a_alpha;

        BiquadCoeffs {
            a0: (a * ((a + 1.0) - (a - 1.0) * cos_omega + two_sqrt_a_alpha)) / a0,
            a1: (2.0 * a * ((a - 1.0) - (a + 1.0) * cos_omega)) / a0,
            a2: (a * ((a + 1.0) - (a - 1.0) * cos_omega - two_sqrt_a_alpha)) / a0,
            b1: (-2.0 * ((a - 1.0) + (a + 1.0) * cos_omega)) / a0,
            b2: ((a + 1.0) + (a - 1.0) * cos_omega - two_sqrt_a_alpha) / a0,
        }
    }

    /// Calculate high shelf filter coefficients (RBJ cookbook)
    fn calc_high_shelf_coeffs(&self, freq: f32, gain_db: f32) -> BiquadCoeffs {
        if gain_db.abs() < 0.01 {
            // Bypass - unity gain
            return BiquadCoeffs {
                a0: 1.0,
                a1: 0.0,
                a2: 0.0,
                b1: 0.0,
                b2: 0.0,
            };
        }

        let a = 10.0f32.powf(gain_db / 40.0);
        let omega = 2.0 * PI * freq / self.sample_rate;
        let sin_omega = omega.sin();
        let cos_omega = omega.cos();
        let alpha = sin_omega / 2.0 * (2.0f32).sqrt();
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let a0 = (a + 1.0) - (a - 1.0) * cos_omega + two_sqrt_a_alpha;

        BiquadCoeffs {
            a0: (a * ((a + 1.0) + (a - 1.0) * cos_omega + two_sqrt_a_alpha)) / a0,
            a1: (-2.0 * a * ((a - 1.0) + (a + 1.0) * cos_omega)) / a0,
            a2: (a * ((a + 1.0) + (a - 1.0) * cos_omega - two_sqrt_a_alpha)) / a0,
            b1: (2.0 * ((a - 1.0) - (a + 1.0) * cos_omega)) / a0,
            b2: ((a + 1.0) - (a - 1.0) * cos_omega - two_sqrt_a_alpha) / a0,
        }
    }

    /// Calculate peaking (bell) filter coefficients (RBJ cookbook)
    fn calc_peaking_coeffs(&self, freq: f32, gain_db: f32, q: f32) -> BiquadCoeffs {
        if gain_db.abs() < 0.01 {
            // Bypass - unity gain
            return BiquadCoeffs {
                a0: 1.0,
                a1: 0.0,
                a2: 0.0,
                b1: 0.0,
                b2: 0.0,
            };
        }

        let a = 10.0f32.powf(gain_db / 40.0);
        let omega = 2.0 * PI * freq / self.sample_rate;
        let sin_omega = omega.sin();
        let cos_omega = omega.cos();
        let alpha = sin_omega / (2.0 * q);

        let a0 = 1.0 + alpha / a;

        BiquadCoeffs {
            a0: (1.0 + alpha * a) / a0,
            a1: (-2.0 * cos_omega) / a0,
            a2: (1.0 - alpha * a) / a0,
            b1: (-2.0 * cos_omega) / a0,
            b2: (1.0 - alpha / a) / a0,
        }
    }

    /// Smooth parameter transitions and update coefficients if needed
    fn smooth_and_update(&mut self) {
        let mut changed = false;

        // Exponential smoothing for each gain parameter
        if (self.low_gain_smooth - self.low_gain).abs() > 0.001 {
            self.low_gain_smooth += (self.low_gain - self.low_gain_smooth) * self.smooth_coeff;
            changed = true;
        } else {
            self.low_gain_smooth = self.low_gain;
        }

        if (self.mid_gain_smooth - self.mid_gain).abs() > 0.001 {
            self.mid_gain_smooth += (self.mid_gain - self.mid_gain_smooth) * self.smooth_coeff;
            changed = true;
        } else {
            self.mid_gain_smooth = self.mid_gain;
        }

        if (self.high_gain_smooth - self.high_gain).abs() > 0.001 {
            self.high_gain_smooth += (self.high_gain - self.high_gain_smooth) * self.smooth_coeff;
            changed = true;
        } else {
            self.high_gain_smooth = self.high_gain;
        }

        if changed || self.needs_update {
            self.update_coefficients();
        }
    }
}

impl Effect for MasteringEQ {
    fn process(&mut self, samples: &mut [f32]) {
        if !self.enabled {
            return;
        }

        // Update smoothed parameters and coefficients
        self.smooth_and_update();

        // Process stereo pairs
        for frame in samples.chunks_exact_mut(2) {
            let mut left = frame[0];
            let mut right = frame[1];

            // Low shelf
            left = self.low_state_l.process(left, &self.low_coeffs);
            right = self.low_state_r.process(right, &self.low_coeffs);

            // Mid bell
            left = self.mid_state_l.process(left, &self.mid_coeffs);
            right = self.mid_state_r.process(right, &self.mid_coeffs);

            // High shelf
            left = self.high_state_l.process(left, &self.high_coeffs);
            right = self.high_state_r.process(right, &self.high_coeffs);

            frame[0] = left;
            frame[1] = right;
        }
    }

    fn reset(&mut self) {
        self.low_state_l.reset();
        self.low_state_r.reset();
        self.mid_state_l.reset();
        self.mid_state_r.reset();
        self.high_state_l.reset();
        self.high_state_r.reset();
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
        "MasteringEQ"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eq_creation() {
        let eq = MasteringEQ::new(48000.0);
        assert!(eq.is_enabled());
        assert!((eq.low_gain() - 1.0).abs() < 0.01);
        assert!(eq.mid_gain().abs() < 0.01);
        assert!((eq.high_gain() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_disabled_passthrough() {
        let mut eq = MasteringEQ::new(48000.0);
        eq.set_enabled(false);

        let mut samples = vec![0.5, 0.5, 0.3, 0.3];
        let original = samples.clone();
        eq.process(&mut samples);

        assert_eq!(samples, original);
    }

    #[test]
    fn test_gain_clamping() {
        let mut eq = MasteringEQ::new(48000.0);

        eq.set_low_gain(10.0);
        assert!((eq.low_gain() - 3.0).abs() < 0.01);

        eq.set_low_gain(-10.0);
        assert!((eq.low_gain() - (-3.0)).abs() < 0.01);
    }

    #[test]
    fn test_flat_eq_processes_audio() {
        // Test that EQ with flat settings processes without error and produces reasonable output
        let mut eq = MasteringEQ::new(48000.0);
        eq.set_low_gain(0.0);
        eq.set_mid_gain(0.0);
        eq.set_high_gain(0.0);

        // Process a test signal
        let mut samples = vec![0.5, 0.5, 0.3, 0.3, 0.1, 0.1, -0.2, -0.2];
        eq.process(&mut samples);

        // Output should be within reasonable range (no NaN, no clipping beyond ±1.5)
        for s in &samples {
            assert!(!s.is_nan(), "EQ produced NaN");
            assert!(s.abs() < 1.5, "EQ output {} exceeds reasonable range", s);
        }
    }
}
