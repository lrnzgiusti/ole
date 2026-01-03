//! Analog warmth simulation
//!
//! - RIAA EQ curve compensation
//! - Subtle saturation (tube/tape character)
//! - Gentle compression for "glue"

use std::f32::consts::PI;

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
    fn process(&mut self, input: f32, a0: f32, a1: f32, a2: f32, b1: f32, b2: f32) -> f32 {
        let output = a0 * input + a1 * self.x1 + a2 * self.x2 - b1 * self.y1 - b2 * self.y2;
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

/// RIAA EQ filter coefficients
struct RiaaCoeffs {
    // Low shelf (bass boost)
    low_a0: f32,
    low_a1: f32,
    low_a2: f32,
    low_b1: f32,
    low_b2: f32,
    // High shelf (treble rolloff)
    high_a0: f32,
    high_a1: f32,
    high_a2: f32,
    high_b1: f32,
    high_b2: f32,
}

/// Analog warmth processor
pub struct AnalogWarmth {
    enabled: bool,
    sample_rate: f32,

    // RIAA EQ (stereo)
    riaa_coeffs: RiaaCoeffs,
    riaa_low_l: BiquadState,
    riaa_low_r: BiquadState,
    riaa_high_l: BiquadState,
    riaa_high_r: BiquadState,

    // Saturation
    drive: f32,           // 0.0-1.0 (amount of saturation)
    saturation_type: SaturationType,

    // Compression
    compression_threshold: f32,
    compression_ratio: f32,
    compression_envelope_l: f32,
    compression_envelope_r: f32,
    compression_attack: f32,
    compression_release: f32,

    // Output level
    output_gain: f32,

    // RIAA intensity (0.0 = bypass, 1.0 = full)
    riaa_amount: f32,
}

/// Type of saturation curve
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SaturationType {
    /// Tube-like (asymmetric, even harmonics)
    #[default]
    Tube,
    /// Tape-like (soft symmetric clipping)
    Tape,
    /// Transistor-like (harder clipping)
    Transistor,
}

impl AnalogWarmth {
    /// Create new analog warmth processor
    pub fn new(sample_rate: f32) -> Self {
        let mut warmth = Self {
            enabled: false,
            sample_rate,
            riaa_coeffs: RiaaCoeffs {
                low_a0: 1.0, low_a1: 0.0, low_a2: 0.0, low_b1: 0.0, low_b2: 0.0,
                high_a0: 1.0, high_a1: 0.0, high_a2: 0.0, high_b1: 0.0, high_b2: 0.0,
            },
            riaa_low_l: BiquadState::default(),
            riaa_low_r: BiquadState::default(),
            riaa_high_l: BiquadState::default(),
            riaa_high_r: BiquadState::default(),
            drive: 0.2,
            saturation_type: SaturationType::Tube,
            compression_threshold: 0.7,
            compression_ratio: 3.0,
            compression_envelope_l: 0.0,
            compression_envelope_r: 0.0,
            compression_attack: 0.0,
            compression_release: 0.0,
            output_gain: 0.9,
            riaa_amount: 0.5,
        };
        warmth.calculate_riaa_coefficients();
        warmth.calculate_compression_coefficients();
        warmth
    }

    /// Enable/disable warmth processing
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.reset();
        }
    }

    /// Check if enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set drive (saturation amount, 0.0-1.0)
    pub fn set_drive(&mut self, drive: f32) {
        self.drive = drive.clamp(0.0, 1.0);
    }

    /// Get drive
    pub fn drive(&self) -> f32 {
        self.drive
    }

    /// Set saturation type
    pub fn set_saturation_type(&mut self, sat_type: SaturationType) {
        self.saturation_type = sat_type;
    }

    /// Set RIAA EQ amount (0.0-1.0)
    pub fn set_riaa_amount(&mut self, amount: f32) {
        self.riaa_amount = amount.clamp(0.0, 1.0);
    }

    /// Set output gain
    pub fn set_output_gain(&mut self, gain: f32) {
        self.output_gain = gain.clamp(0.0, 2.0);
    }

    /// Reset all state
    pub fn reset(&mut self) {
        self.riaa_low_l.reset();
        self.riaa_low_r.reset();
        self.riaa_high_l.reset();
        self.riaa_high_r.reset();
        self.compression_envelope_l = 0.0;
        self.compression_envelope_r = 0.0;
    }

    /// Calculate RIAA EQ coefficients
    ///
    /// RIAA playback curve time constants:
    /// - 3180 μs (50.05 Hz) - bass turnover
    /// - 318 μs (500.5 Hz) - mid transition
    /// - 75 μs (2122 Hz) - treble rolloff start
    fn calculate_riaa_coefficients(&mut self) {
        let sr = self.sample_rate;

        // Low shelf: boost below ~500 Hz
        // Simplified: low shelf at 300 Hz with +3dB gain
        let f_low: f32 = 300.0;
        let gain_low: f32 = 1.5; // ~3.5 dB boost
        let omega_low = 2.0 * PI * f_low / sr;
        let sin_low = omega_low.sin();
        let cos_low = omega_low.cos();
        let alpha_low = sin_low / 2.0 * (2.0f32).sqrt();
        let a_low = gain_low.sqrt();

        let b0 = a_low * ((a_low + 1.0) - (a_low - 1.0) * cos_low + 2.0 * a_low.sqrt() * alpha_low);
        let b1 = 2.0 * a_low * ((a_low - 1.0) - (a_low + 1.0) * cos_low);
        let b2 = a_low * ((a_low + 1.0) - (a_low - 1.0) * cos_low - 2.0 * a_low.sqrt() * alpha_low);
        let a0 = (a_low + 1.0) + (a_low - 1.0) * cos_low + 2.0 * a_low.sqrt() * alpha_low;
        let a1 = -2.0 * ((a_low - 1.0) + (a_low + 1.0) * cos_low);
        let a2 = (a_low + 1.0) + (a_low - 1.0) * cos_low - 2.0 * a_low.sqrt() * alpha_low;

        self.riaa_coeffs.low_a0 = b0 / a0;
        self.riaa_coeffs.low_a1 = b1 / a0;
        self.riaa_coeffs.low_a2 = b2 / a0;
        self.riaa_coeffs.low_b1 = a1 / a0;
        self.riaa_coeffs.low_b2 = a2 / a0;

        // High shelf: rolloff above ~2 kHz
        // Simplified: high shelf at 2500 Hz with -2dB cut
        let f_high: f32 = 2500.0;
        let gain_high: f32 = 0.8; // ~-2 dB
        let omega_high = 2.0 * PI * f_high / sr;
        let sin_high = omega_high.sin();
        let cos_high = omega_high.cos();
        let alpha_high = sin_high / 2.0 * (2.0f32).sqrt();
        let a_high = gain_high.sqrt();

        let b0 = a_high * ((a_high + 1.0) + (a_high - 1.0) * cos_high + 2.0 * a_high.sqrt() * alpha_high);
        let b1 = -2.0 * a_high * ((a_high - 1.0) + (a_high + 1.0) * cos_high);
        let b2 = a_high * ((a_high + 1.0) + (a_high - 1.0) * cos_high - 2.0 * a_high.sqrt() * alpha_high);
        let a0 = (a_high + 1.0) - (a_high - 1.0) * cos_high + 2.0 * a_high.sqrt() * alpha_high;
        let a1 = 2.0 * ((a_high - 1.0) - (a_high + 1.0) * cos_high);
        let a2 = (a_high + 1.0) - (a_high - 1.0) * cos_high - 2.0 * a_high.sqrt() * alpha_high;

        self.riaa_coeffs.high_a0 = b0 / a0;
        self.riaa_coeffs.high_a1 = b1 / a0;
        self.riaa_coeffs.high_a2 = b2 / a0;
        self.riaa_coeffs.high_b1 = a1 / a0;
        self.riaa_coeffs.high_b2 = a2 / a0;
    }

    /// Calculate compression envelope coefficients
    fn calculate_compression_coefficients(&mut self) {
        // Attack: ~5ms
        self.compression_attack = 1.0 - (-1.0 / (0.005 * self.sample_rate)).exp();
        // Release: ~100ms
        self.compression_release = 1.0 - (-1.0 / (0.100 * self.sample_rate)).exp();
    }

    /// Tube-style saturation (asymmetric, even harmonics)
    #[inline]
    fn saturate_tube(x: f32, drive: f32) -> f32 {
        let driven = x * (1.0 + drive * 3.0);
        // Asymmetric soft clipping
        if driven >= 0.0 {
            1.0 - (-driven).exp()
        } else {
            (driven.exp() - 1.0) * 0.9 // Slightly less gain on negative
        }
    }

    /// Tape-style saturation (symmetric, soft)
    #[inline]
    fn saturate_tape(x: f32, drive: f32) -> f32 {
        let driven = x * (1.0 + drive * 2.0);
        // Soft symmetric clipping using tanh approximation
        let x2 = driven * driven;
        driven * (27.0 + x2) / (27.0 + 9.0 * x2)
    }

    /// Transistor-style saturation (harder)
    #[inline]
    fn saturate_transistor(x: f32, drive: f32) -> f32 {
        let driven = x * (1.0 + drive * 4.0);
        // Harder clipping
        driven / (1.0 + driven.abs())
    }

    /// Apply saturation based on current type
    #[inline]
    fn saturate(&self, x: f32) -> f32 {
        if self.drive < 0.001 {
            return x;
        }

        match self.saturation_type {
            SaturationType::Tube => Self::saturate_tube(x, self.drive),
            SaturationType::Tape => Self::saturate_tape(x, self.drive),
            SaturationType::Transistor => Self::saturate_transistor(x, self.drive),
        }
    }

    /// Apply gentle compression
    #[inline]
    fn compress(&mut self, sample: f32, is_right: bool) -> f32 {
        let envelope = if is_right {
            &mut self.compression_envelope_r
        } else {
            &mut self.compression_envelope_l
        };

        let input_level = sample.abs();

        // Envelope follower
        if input_level > *envelope {
            *envelope += (input_level - *envelope) * self.compression_attack;
        } else {
            *envelope += (input_level - *envelope) * self.compression_release;
        }

        // Soft knee compression
        if *envelope > self.compression_threshold {
            let over = *envelope - self.compression_threshold;
            let gain_reduction = 1.0 - (over / self.compression_ratio);
            sample * gain_reduction.max(0.5)
        } else {
            sample
        }
    }

    /// Process audio samples in place (stereo interleaved)
    pub fn process(&mut self, samples: &mut [f32]) {
        if !self.enabled {
            return;
        }

        for frame in samples.chunks_mut(2) {
            if frame.len() != 2 {
                continue;
            }

            let mut left = frame[0];
            let mut right = frame[1];

            // Apply RIAA EQ (if enabled)
            if self.riaa_amount > 0.001 {
                let eq_left = {
                    let low = self.riaa_low_l.process(
                        left,
                        self.riaa_coeffs.low_a0,
                        self.riaa_coeffs.low_a1,
                        self.riaa_coeffs.low_a2,
                        self.riaa_coeffs.low_b1,
                        self.riaa_coeffs.low_b2,
                    );
                    self.riaa_high_l.process(
                        low,
                        self.riaa_coeffs.high_a0,
                        self.riaa_coeffs.high_a1,
                        self.riaa_coeffs.high_a2,
                        self.riaa_coeffs.high_b1,
                        self.riaa_coeffs.high_b2,
                    )
                };
                let eq_right = {
                    let low = self.riaa_low_r.process(
                        right,
                        self.riaa_coeffs.low_a0,
                        self.riaa_coeffs.low_a1,
                        self.riaa_coeffs.low_a2,
                        self.riaa_coeffs.low_b1,
                        self.riaa_coeffs.low_b2,
                    );
                    self.riaa_high_r.process(
                        low,
                        self.riaa_coeffs.high_a0,
                        self.riaa_coeffs.high_a1,
                        self.riaa_coeffs.high_a2,
                        self.riaa_coeffs.high_b1,
                        self.riaa_coeffs.high_b2,
                    )
                };

                // Blend with original based on riaa_amount
                left = left * (1.0 - self.riaa_amount) + eq_left * self.riaa_amount;
                right = right * (1.0 - self.riaa_amount) + eq_right * self.riaa_amount;
            }

            // Apply saturation
            left = self.saturate(left);
            right = self.saturate(right);

            // Apply gentle compression
            left = self.compress(left, false);
            right = self.compress(right, true);

            // Apply output gain
            frame[0] = left * self.output_gain;
            frame[1] = right * self.output_gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_warmth_creation() {
        let warmth = AnalogWarmth::new(48000.0);
        assert!(!warmth.is_enabled());
    }

    #[test]
    fn test_disabled_passthrough() {
        let mut warmth = AnalogWarmth::new(48000.0);
        warmth.set_enabled(false);

        let mut samples = vec![0.5, 0.5, 0.3, 0.3];
        let original = samples.clone();
        warmth.process(&mut samples);

        assert_eq!(samples, original);
    }

    #[test]
    fn test_saturation_types() {
        // Tube saturation
        let tube = AnalogWarmth::saturate_tube(0.5, 0.5);
        assert!(tube > 0.0 && tube < 1.0);

        // Tape saturation
        let tape = AnalogWarmth::saturate_tape(0.5, 0.5);
        assert!(tape > 0.0 && tape < 1.0);

        // Transistor saturation
        let transistor = AnalogWarmth::saturate_transistor(0.5, 0.5);
        assert!(transistor > 0.0 && transistor < 1.0);
    }

    #[test]
    fn test_drive_affects_output() {
        let mut warmth = AnalogWarmth::new(48000.0);
        warmth.set_enabled(true);
        warmth.set_riaa_amount(0.0); // Disable RIAA for this test

        // Low drive
        warmth.set_drive(0.1);
        let mut samples_low = vec![0.5, 0.5];
        warmth.process(&mut samples_low);

        // Reset and high drive
        warmth.reset();
        warmth.set_drive(0.9);
        let mut samples_high = vec![0.5, 0.5];
        warmth.process(&mut samples_high);

        // High drive should affect output more
        // (Note: actual relationship depends on saturation curve)
        assert!(samples_low[0] != samples_high[0]);
    }
}
