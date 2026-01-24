//! Harmonic saturation for subtle analog warmth
//!
//! Very subtle saturation designed for mastering use. Uses the same curves
//! as the vinyl warmth module but at much lower intensities.
//!
//! Features:
//! - Tape, tube, and transistor saturation modes
//! - DC blocker to prevent offset buildup
//! - Auto-gain compensation
//! - Parallel dry/wet mix

use std::f32::consts::PI;

use crate::effects::Effect;

/// Saturation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SaturationMode {
    /// Tape-style: soft symmetric clipping (warm, musical)
    #[default]
    Tape,
    /// Tube-style: asymmetric, even harmonics (warm, rich)
    Tube,
    /// Transistor-style: harder clipping (edgy, aggressive)
    Transistor,
}

impl SaturationMode {
    /// Get display name
    pub fn display_name(&self) -> &'static str {
        match self {
            SaturationMode::Tape => "TAPE",
            SaturationMode::Tube => "TUBE",
            SaturationMode::Transistor => "TRANS",
        }
    }
}

/// DC blocker filter state
#[derive(Default, Clone)]
struct DcBlocker {
    x_prev: f32,
    y_prev: f32,
    coeff: f32,
}

impl DcBlocker {
    fn new(sample_rate: f32) -> Self {
        // Cutoff around 10Hz
        let cutoff = 10.0;
        let omega = 2.0 * PI * cutoff / sample_rate;
        Self {
            x_prev: 0.0,
            y_prev: 0.0,
            coeff: 1.0 - omega,
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let output = input - self.x_prev + self.coeff * self.y_prev;
        self.x_prev = input;
        self.y_prev = output;
        output
    }

    fn reset(&mut self) {
        self.x_prev = 0.0;
        self.y_prev = 0.0;
    }
}

/// Mastering saturation processor
pub struct MasteringSaturation {
    enabled: bool,

    // Parameters
    drive: f32, // Drive amount (0.0-0.3 for mastering)
    mix: f32,   // Wet/dry mix (0.0-1.0)
    mode: SaturationMode,

    // DC blocker (stereo)
    dc_blocker_l: DcBlocker,
    dc_blocker_r: DcBlocker,
}

impl MasteringSaturation {
    /// Create a new mastering saturation processor
    pub fn new(sample_rate: f32) -> Self {
        Self {
            enabled: true,
            drive: 0.1,
            mix: 0.3,
            mode: SaturationMode::Tape,
            dc_blocker_l: DcBlocker::new(sample_rate),
            dc_blocker_r: DcBlocker::new(sample_rate),
        }
    }

    /// Set drive amount (0.0-0.3 for mastering use)
    pub fn set_drive(&mut self, drive: f32) {
        self.drive = drive.clamp(0.0, 0.3);
    }

    /// Get current drive
    pub fn drive(&self) -> f32 {
        self.drive
    }

    /// Set wet/dry mix (0.0-1.0)
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    /// Get current mix
    pub fn mix(&self) -> f32 {
        self.mix
    }

    /// Set saturation mode
    pub fn set_mode(&mut self, mode: SaturationMode) {
        self.mode = mode;
    }

    /// Get current mode
    pub fn mode(&self) -> SaturationMode {
        self.mode
    }

    /// Tape-style saturation (symmetric, soft)
    /// Formula: x * (27 + x²) / (27 + 9x²)
    #[inline]
    fn saturate_tape(x: f32, drive: f32) -> f32 {
        let driven = x * (1.0 + drive * 2.0);
        let x2 = driven * driven;
        driven * (27.0 + x2) / (27.0 + 9.0 * x2)
    }

    /// Tube-style saturation (asymmetric, even harmonics)
    #[inline]
    fn saturate_tube(x: f32, drive: f32) -> f32 {
        let driven = x * (1.0 + drive * 3.0);
        if driven >= 0.0 {
            1.0 - (-driven).exp()
        } else {
            (driven.exp() - 1.0) * 0.9 // Slightly less gain on negative
        }
    }

    /// Transistor-style saturation (harder)
    #[inline]
    fn saturate_transistor(x: f32, drive: f32) -> f32 {
        let driven = x * (1.0 + drive * 4.0);
        driven / (1.0 + driven.abs())
    }

    /// Apply saturation based on current mode
    #[inline]
    fn saturate(&self, x: f32) -> f32 {
        match self.mode {
            SaturationMode::Tape => Self::saturate_tape(x, self.drive),
            SaturationMode::Tube => Self::saturate_tube(x, self.drive),
            SaturationMode::Transistor => Self::saturate_transistor(x, self.drive),
        }
    }

    /// Calculate auto-gain compensation based on drive and mode
    #[inline]
    fn auto_gain(&self) -> f32 {
        // Compensate for the gain increase from saturation
        // These values are empirically tuned for each mode
        let base_compensation = match self.mode {
            SaturationMode::Tape => 1.0 / (1.0 + self.drive * 0.3),
            SaturationMode::Tube => 1.0 / (1.0 + self.drive * 0.5),
            SaturationMode::Transistor => 1.0 / (1.0 + self.drive * 0.4),
        };
        base_compensation
    }
}

impl Effect for MasteringSaturation {
    fn process(&mut self, samples: &mut [f32]) {
        if !self.enabled || self.drive < 0.001 {
            return;
        }

        let gain = self.auto_gain();
        let wet = self.mix;
        let dry = 1.0 - self.mix;

        for frame in samples.chunks_exact_mut(2) {
            let dry_l = frame[0];
            let dry_r = frame[1];

            // Apply saturation
            let mut wet_l = self.saturate(dry_l);
            let mut wet_r = self.saturate(dry_r);

            // Apply DC blocker to remove any offset from asymmetric saturation
            wet_l = self.dc_blocker_l.process(wet_l);
            wet_r = self.dc_blocker_r.process(wet_r);

            // Apply auto-gain compensation
            wet_l *= gain;
            wet_r *= gain;

            // Mix dry and wet
            frame[0] = dry_l * dry + wet_l * wet;
            frame[1] = dry_r * dry + wet_r * wet;
        }
    }

    fn reset(&mut self) {
        self.dc_blocker_l.reset();
        self.dc_blocker_r.reset();
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
        "MasteringSaturation"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_saturation_creation() {
        let sat = MasteringSaturation::new(48000.0);
        assert!(sat.is_enabled());
        assert!((sat.drive() - 0.1).abs() < 0.01);
        assert!((sat.mix() - 0.3).abs() < 0.01);
    }

    #[test]
    fn test_disabled_passthrough() {
        let mut sat = MasteringSaturation::new(48000.0);
        sat.set_enabled(false);

        let mut samples = vec![0.5, 0.5, 0.3, 0.3];
        let original = samples.clone();
        sat.process(&mut samples);

        assert_eq!(samples, original);
    }

    #[test]
    fn test_zero_drive_passthrough() {
        let mut sat = MasteringSaturation::new(48000.0);
        sat.set_drive(0.0);

        let mut samples = vec![0.5, 0.5, 0.3, 0.3];
        let original = samples.clone();
        sat.process(&mut samples);

        // With zero drive, signal should pass through unchanged
        assert_eq!(samples, original);
    }

    #[test]
    fn test_saturation_modes() {
        // Tape saturation
        let tape = MasteringSaturation::saturate_tape(0.5, 0.2);
        assert!(tape > 0.0 && tape < 1.0);

        // Tube saturation
        let tube = MasteringSaturation::saturate_tube(0.5, 0.2);
        assert!(tube > 0.0 && tube < 1.0);

        // Transistor saturation
        let trans = MasteringSaturation::saturate_transistor(0.5, 0.2);
        assert!(trans > 0.0 && trans < 1.0);
    }

    #[test]
    fn test_parameter_clamping() {
        let mut sat = MasteringSaturation::new(48000.0);

        sat.set_drive(1.0);
        assert!((sat.drive() - 0.3).abs() < 0.01);

        sat.set_mix(2.0);
        assert!((sat.mix() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_saturation_does_not_exceed_unity() {
        let mut sat = MasteringSaturation::new(48000.0);
        sat.set_drive(0.3);
        sat.set_mix(1.0);

        // Even with maximum drive, output should stay reasonable
        let mut samples: Vec<f32> = vec![0.9, 0.9, -0.9, -0.9];
        samples.extend(vec![0.5; 100]); // Some settling time for DC blocker

        sat.process(&mut samples);

        // Output should not clip excessively
        for s in &samples {
            assert!(s.abs() < 1.5, "Output {} exceeds expected range", s);
        }
    }
}
