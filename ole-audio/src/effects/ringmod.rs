//! Ring Modulator effect - amplitude modulation for metallic/robotic textures
//!
//! Classic DJ effect that multiplies the input signal by an oscillator,
//! producing sum and difference frequencies for bell-like, robotic, or
//! dissonant tones. Supports sine, square, and triangle waveforms with
//! a stereo offset for spatial width.

use super::Effect;
use std::f32::consts::PI;

/// Oscillator waveform for ring modulation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RingModWaveform {
    #[default]
    Sine,
    Square,
    Triangle,
}

/// Ring Modulator effect with selectable waveform and stereo offset
pub struct RingModulator {
    enabled: bool,
    sample_rate: f32,

    /// Modulation frequency in Hz (1.0 - 5000.0)
    frequency: f32,

    /// Wet/dry mix (0.0 - 1.0)
    mix: f32,

    /// Oscillator waveform
    waveform: RingModWaveform,

    /// Stereo frequency offset in Hz (0.0 - 100.0)
    stereo_offset: f32,

    /// Phase counter for left channel (0.0 - 1.0)
    phase_l: f32,

    /// Phase counter for right channel (0.0 - 1.0)
    phase_r: f32,

    /// Wet envelope for click-free enable/disable
    wet_target: f32,
    wet_current: f32,
}

impl RingModulator {
    /// Wet envelope smoothing coefficient
    const WET_SMOOTH_COEFF: f32 = 0.9995;

    /// Create a new ring modulator effect
    pub fn new(sample_rate: f32) -> Self {
        Self {
            enabled: false,
            sample_rate,
            frequency: 440.0,
            mix: 0.5,
            waveform: RingModWaveform::default(),
            stereo_offset: 5.0,
            phase_l: 0.0,
            phase_r: 0.0,
            wet_target: 0.0,
            wet_current: 0.0,
        }
    }

    /// Set modulation frequency in Hz (1.0 - 5000.0)
    pub fn set_frequency(&mut self, frequency: f32) {
        self.frequency = frequency.clamp(1.0, 5000.0);
    }

    /// Get modulation frequency
    pub fn frequency(&self) -> f32 {
        self.frequency
    }

    /// Set wet/dry mix (0.0 - 1.0)
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    /// Get mix
    pub fn mix(&self) -> f32 {
        self.mix
    }

    /// Set oscillator waveform
    pub fn set_waveform(&mut self, waveform: RingModWaveform) {
        self.waveform = waveform;
    }

    /// Get oscillator waveform
    pub fn waveform(&self) -> RingModWaveform {
        self.waveform
    }

    /// Set stereo frequency offset in Hz (0.0 - 100.0)
    pub fn set_stereo_offset(&mut self, offset: f32) {
        self.stereo_offset = offset.clamp(0.0, 100.0);
    }

    /// Get stereo offset
    pub fn stereo_offset(&self) -> f32 {
        self.stereo_offset
    }

    /// Calculate oscillator value for a given phase
    #[inline]
    fn oscillator(&self, phase: f32) -> f32 {
        match self.waveform {
            RingModWaveform::Sine => (2.0 * PI * phase).sin(),
            RingModWaveform::Square => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            RingModWaveform::Triangle => (phase * 4.0 - 2.0).abs() - 1.0,
        }
    }
}

impl Effect for RingModulator {
    fn process(&mut self, samples: &mut [f32]) {
        // Skip if fully disabled and envelope settled
        if !self.enabled && self.wet_current < 0.0001 {
            return;
        }

        let freq_l = self.frequency;
        let freq_r = self.frequency + self.stereo_offset;
        let phase_inc_l = freq_l / self.sample_rate;
        let phase_inc_r = freq_r / self.sample_rate;

        for frame in samples.chunks_mut(2) {
            if frame.len() < 2 {
                continue;
            }

            // Smooth wet envelope
            self.wet_current = Self::WET_SMOOTH_COEFF * self.wet_current
                + (1.0 - Self::WET_SMOOTH_COEFF) * self.wet_target;

            let effective_mix = self.mix * self.wet_current;

            // Calculate oscillator values for each channel
            let osc_l = self.oscillator(self.phase_l);
            let osc_r = self.oscillator(self.phase_r);

            // Ring modulation: dry * (1 - mix) + dry * osc * mix
            let dry_l = frame[0];
            let dry_r = frame[1];
            frame[0] = dry_l * (1.0 - effective_mix) + dry_l * osc_l * effective_mix;
            frame[1] = dry_r * (1.0 - effective_mix) + dry_r * osc_r * effective_mix;

            // Advance phases
            self.phase_l += phase_inc_l;
            if self.phase_l >= 1.0 {
                self.phase_l -= 1.0;
            }
            self.phase_r += phase_inc_r;
            if self.phase_r >= 1.0 {
                self.phase_r -= 1.0;
            }
        }
    }

    fn reset(&mut self) {
        self.phase_l = 0.0;
        self.phase_r = 0.0;
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.wet_target = if enabled { 1.0 } else { 0.0 };
    }

    fn name(&self) -> &'static str {
        "Ring Mod"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ringmod_creation() {
        let rm = RingModulator::new(48000.0);
        assert!(!rm.is_enabled());
        assert_eq!(rm.frequency(), 440.0);
        assert_eq!(rm.mix(), 0.5);
        assert_eq!(rm.waveform(), RingModWaveform::Sine);
        assert_eq!(rm.stereo_offset(), 5.0);
        assert_eq!(rm.name(), "Ring Mod");
    }

    #[test]
    fn test_ringmod_parameter_clamping() {
        let mut rm = RingModulator::new(48000.0);

        rm.set_frequency(0.0);
        assert_eq!(rm.frequency(), 1.0);

        rm.set_frequency(10000.0);
        assert_eq!(rm.frequency(), 5000.0);

        rm.set_mix(-1.0);
        assert_eq!(rm.mix(), 0.0);

        rm.set_mix(2.0);
        assert_eq!(rm.mix(), 1.0);

        rm.set_stereo_offset(-5.0);
        assert_eq!(rm.stereo_offset(), 0.0);

        rm.set_stereo_offset(200.0);
        assert_eq!(rm.stereo_offset(), 100.0);
    }

    #[test]
    fn test_ringmod_bypass_when_disabled() {
        let mut rm = RingModulator::new(48000.0);
        // wet_current is 0.0 by default, enabled is false
        let mut samples = vec![0.5, 0.5, 0.3, 0.3];
        let original = samples.clone();
        rm.process(&mut samples);
        assert_eq!(samples, original);
    }

    #[test]
    fn test_ringmod_processes_audio_sine() {
        let mut rm = RingModulator::new(48000.0);
        rm.set_enabled(true);
        rm.wet_current = 1.0; // Force wet for test
        rm.set_mix(1.0);

        let mut samples = vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        rm.process(&mut samples);

        // With sine waveform at phase 0, sin(0) = 0, so first sample should
        // be dry*(1-mix) + dry*0*mix = 0 (since mix=1.0)
        // Actually after smoothing, wet_current might not be exactly 1.0
        // but we forced it, so effective_mix = 1.0 * ~1.0
        // sin(2*PI*0) = 0, so output ~ 0
        assert!(samples[0].abs() < 0.01);
        assert!(samples[1].abs() < 0.01);

        // Subsequent samples should differ as phase advances
        // Phase after 1 frame: 440/48000 ~ 0.00917
        // sin(2*PI*0.00917) ~ 0.0576, so output ~ 0.0576
        assert!(samples[2].abs() > 0.01);
    }

    #[test]
    fn test_ringmod_processes_audio_square() {
        let mut rm = RingModulator::new(48000.0);
        rm.set_enabled(true);
        rm.wet_current = 1.0;
        rm.set_mix(1.0);
        rm.set_waveform(RingModWaveform::Square);

        let mut samples = vec![1.0, 1.0];
        rm.process(&mut samples);

        // Phase starts at 0.0, which is < 0.5, so square = 1.0
        // output = 1.0 * (1-1) + 1.0 * 1.0 * 1.0 = 1.0
        assert!((samples[0] - 1.0).abs() < 0.01);
        assert!((samples[1] - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_ringmod_processes_audio_triangle() {
        let mut rm = RingModulator::new(48000.0);
        rm.set_enabled(true);
        rm.wet_current = 1.0;
        rm.set_mix(1.0);
        rm.set_waveform(RingModWaveform::Triangle);

        let mut samples = vec![1.0, 1.0];
        rm.process(&mut samples);

        // Phase 0: (0*4 - 2).abs() - 1 = 2 - 1 = 1.0
        // output = 1.0 * (1-1) + 1.0 * 1.0 * 1.0 = 1.0
        assert!((samples[0] - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_ringmod_reset() {
        let mut rm = RingModulator::new(48000.0);
        rm.set_enabled(true);
        rm.wet_current = 1.0;

        let mut samples = vec![1.0, 1.0, 1.0, 1.0];
        rm.process(&mut samples);
        assert!(rm.phase_l > 0.0);

        rm.reset();
        assert_eq!(rm.phase_l, 0.0);
        assert_eq!(rm.phase_r, 0.0);
    }

    #[test]
    fn test_ringmod_wet_envelope() {
        let mut rm = RingModulator::new(48000.0);
        rm.set_enabled(true);
        // wet_current starts at 0, wet_target is now 1.0
        // After processing, wet_current should increase toward 1.0
        let mut samples = vec![1.0, 1.0];
        rm.process(&mut samples);
        assert!(rm.wet_current > 0.0);
        assert!(rm.wet_current < 1.0);
    }

    #[test]
    fn test_ringmod_waveform_default() {
        assert_eq!(RingModWaveform::default(), RingModWaveform::Sine);
    }
}
