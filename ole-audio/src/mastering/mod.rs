//! Mastering chain for professional output processing
//!
//! A subtle, transparent mastering chain optimized for electronic music.
//! Philosophy: "extremely mild but state of the art" - enhances without coloring.
//!
//! Signal flow:
//! ```text
//! Input → EQ → Compressor → Saturation → Stereo → Output
//!                                          ↓
//!                                    Loudness Meter (analysis only)
//! ```

mod compressor;
mod eq;
mod meter;
mod saturation;
mod stereo;

pub use compressor::MasteringCompressor;
pub use eq::MasteringEQ;
pub use meter::{LoudnessMeter, LufsValues};
pub use saturation::{MasteringSaturation, SaturationMode};
pub use stereo::StereoEnhancer;

use crate::effects::Effect;

/// Genre presets for the mastering chain
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MasteringPreset {
    /// Bypass - no processing
    Off,
    /// Minimal processing - transparent
    #[default]
    Clean,
    /// Techno - subtle warmth, tight low end
    Techno,
    /// House - warm, wide, punchy
    House,
    /// Drum & Bass - crisp highs, controlled lows
    DnB,
}

impl MasteringPreset {
    /// Get display name for the preset
    pub fn display_name(&self) -> &'static str {
        match self {
            MasteringPreset::Off => "OFF",
            MasteringPreset::Clean => "CLEAN",
            MasteringPreset::Techno => "TECHNO",
            MasteringPreset::House => "HOUSE",
            MasteringPreset::DnB => "D&B",
        }
    }

    /// Cycle to the next preset
    pub fn next(self) -> Self {
        match self {
            MasteringPreset::Off => MasteringPreset::Clean,
            MasteringPreset::Clean => MasteringPreset::Techno,
            MasteringPreset::Techno => MasteringPreset::House,
            MasteringPreset::House => MasteringPreset::DnB,
            MasteringPreset::DnB => MasteringPreset::Off,
        }
    }
}

/// Mastering chain wrapper
///
/// Combines all mastering components in the correct signal flow order.
/// Each component can be individually bypassed or the entire chain can be disabled.
pub struct MasteringChain {
    enabled: bool,
    preset: MasteringPreset,

    // Processing components (in signal flow order)
    eq: MasteringEQ,
    compressor: MasteringCompressor,
    saturation: MasteringSaturation,
    stereo: StereoEnhancer,

    // Analysis (does not affect signal)
    meter: LoudnessMeter,
}

impl MasteringChain {
    /// Create a new mastering chain with default settings
    pub fn new(sample_rate: f32) -> Self {
        let mut chain = Self {
            enabled: true,
            preset: MasteringPreset::default(),
            eq: MasteringEQ::new(sample_rate),
            compressor: MasteringCompressor::new(sample_rate),
            saturation: MasteringSaturation::new(sample_rate),
            stereo: StereoEnhancer::new(sample_rate),
            meter: LoudnessMeter::new(sample_rate as u32),
        };
        chain.apply_preset(MasteringPreset::default());
        chain
    }

    /// Get current preset
    pub fn preset(&self) -> MasteringPreset {
        self.preset
    }

    /// Set preset and apply its settings
    pub fn set_preset(&mut self, preset: MasteringPreset) {
        self.preset = preset;
        self.apply_preset(preset);
    }

    /// Cycle to the next preset
    pub fn cycle_preset(&mut self) {
        self.set_preset(self.preset.next());
    }

    /// Apply preset settings to all components
    fn apply_preset(&mut self, preset: MasteringPreset) {
        match preset {
            MasteringPreset::Off => {
                self.eq.set_enabled(false);
                self.compressor.set_enabled(false);
                self.saturation.set_enabled(false);
                self.stereo.set_enabled(false);
            }
            MasteringPreset::Clean => {
                // Minimal processing - very transparent
                self.eq.set_enabled(true);
                self.eq.set_low_gain(0.0);
                self.eq.set_mid_gain(0.0);
                self.eq.set_high_gain(0.0);

                self.compressor.set_enabled(true);
                self.compressor.set_ratio(1.1);
                self.compressor.set_threshold(-18.0);

                self.saturation.set_enabled(false);

                self.stereo.set_enabled(true);
                self.stereo.set_width(1.0);
            }
            MasteringPreset::Techno => {
                // Tight low end, subtle warmth, crisp highs
                self.eq.set_enabled(true);
                self.eq.set_low_gain(1.0); // +1dB sub weight
                self.eq.set_mid_gain(0.0);
                self.eq.set_high_gain(0.5); // +0.5dB air

                self.compressor.set_enabled(true);
                self.compressor.set_ratio(1.5);
                self.compressor.set_threshold(-12.0);
                self.compressor.set_attack_ms(20.0);
                self.compressor.set_release_ms(150.0);

                self.saturation.set_enabled(true);
                self.saturation.set_drive(0.1);
                self.saturation.set_mix(0.3);
                self.saturation.set_mode(SaturationMode::Tape);

                self.stereo.set_enabled(true);
                self.stereo.set_width(1.05);
                self.stereo.set_bass_mono_freq(150.0);
            }
            MasteringPreset::House => {
                // Warm, wide, punchy
                self.eq.set_enabled(true);
                self.eq.set_low_gain(1.5); // +1.5dB warmth
                self.eq.set_mid_gain(0.0);
                self.eq.set_high_gain(1.0); // +1dB presence

                self.compressor.set_enabled(true);
                self.compressor.set_ratio(1.25);
                self.compressor.set_threshold(-14.0);
                self.compressor.set_attack_ms(25.0);
                self.compressor.set_release_ms(200.0);

                self.saturation.set_enabled(true);
                self.saturation.set_drive(0.15);
                self.saturation.set_mix(0.35);
                self.saturation.set_mode(SaturationMode::Tape);

                self.stereo.set_enabled(true);
                self.stereo.set_width(1.10);
                self.stereo.set_bass_mono_freq(120.0);
            }
            MasteringPreset::DnB => {
                // Crisp highs, controlled lows, punchy
                self.eq.set_enabled(true);
                self.eq.set_low_gain(0.5); // Subtle low boost
                self.eq.set_mid_gain(-0.5); // Slight mid scoop
                self.eq.set_high_gain(1.5); // +1.5dB clarity

                self.compressor.set_enabled(true);
                self.compressor.set_ratio(1.75);
                self.compressor.set_threshold(-10.0);
                self.compressor.set_attack_ms(15.0);
                self.compressor.set_release_ms(100.0);

                self.saturation.set_enabled(true);
                self.saturation.set_drive(0.08);
                self.saturation.set_mix(0.25);
                self.saturation.set_mode(SaturationMode::Tape);

                self.stereo.set_enabled(true);
                self.stereo.set_width(1.0); // Keep tight for DnB
                self.stereo.set_bass_mono_freq(180.0);
            }
        }
    }

    /// Get the current LUFS values from the meter
    pub fn lufs(&self) -> LufsValues {
        self.meter.get_lufs()
    }

    /// Get current compressor gain reduction in dB
    pub fn gain_reduction_db(&self) -> f32 {
        self.compressor.gain_reduction_db()
    }

    /// Access the EQ for direct parameter control
    pub fn eq_mut(&mut self) -> &mut MasteringEQ {
        &mut self.eq
    }

    /// Access the compressor for direct parameter control
    pub fn compressor_mut(&mut self) -> &mut MasteringCompressor {
        &mut self.compressor
    }

    /// Access the saturation for direct parameter control
    pub fn saturation_mut(&mut self) -> &mut MasteringSaturation {
        &mut self.saturation
    }

    /// Access the stereo enhancer for direct parameter control
    pub fn stereo_mut(&mut self) -> &mut StereoEnhancer {
        &mut self.stereo
    }
}

impl Effect for MasteringChain {
    fn process(&mut self, samples: &mut [f32]) {
        if !self.enabled || self.preset == MasteringPreset::Off {
            // Still run the meter for analysis even when bypassed
            self.meter.process(samples);
            return;
        }

        // Signal flow: EQ → Compressor → Saturation → Stereo
        self.eq.process(samples);
        self.compressor.process(samples);
        self.saturation.process(samples);
        self.stereo.process(samples);

        // Analysis (doesn't modify signal)
        self.meter.process(samples);
    }

    fn reset(&mut self) {
        self.eq.reset();
        self.compressor.reset();
        self.saturation.reset();
        self.stereo.reset();
        self.meter.reset();
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
        "MasteringChain"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mastering_chain_creation() {
        let chain = MasteringChain::new(48000.0);
        assert!(chain.is_enabled());
        assert_eq!(chain.preset(), MasteringPreset::Clean);
    }

    #[test]
    fn test_preset_cycling() {
        let mut chain = MasteringChain::new(48000.0);
        assert_eq!(chain.preset(), MasteringPreset::Clean);

        chain.cycle_preset();
        assert_eq!(chain.preset(), MasteringPreset::Techno);

        chain.cycle_preset();
        assert_eq!(chain.preset(), MasteringPreset::House);

        chain.cycle_preset();
        assert_eq!(chain.preset(), MasteringPreset::DnB);

        chain.cycle_preset();
        assert_eq!(chain.preset(), MasteringPreset::Off);

        chain.cycle_preset();
        assert_eq!(chain.preset(), MasteringPreset::Clean);
    }

    #[test]
    fn test_disabled_passthrough() {
        let mut chain = MasteringChain::new(48000.0);
        chain.set_enabled(false);

        let mut samples = vec![0.5, 0.5, 0.3, 0.3];
        let original = samples.clone();
        chain.process(&mut samples);

        // When disabled, samples should pass through unchanged
        // (meter still runs but doesn't modify)
        assert_eq!(samples, original);
    }
}
