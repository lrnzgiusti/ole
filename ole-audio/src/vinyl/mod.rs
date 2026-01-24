//! Vinyl emulation module
//!
//! Provides authentic vinyl turntable simulation including:
//! - Turntable motor physics (startup, brake, momentum)
//! - Wow and flutter (pitch modulation)
//! - Analog warmth (RIAA EQ, saturation)
//! - Vinyl noise (surface noise, crackle, pops)

mod motor;
mod noise;
mod warmth;
mod wow_flutter;

pub use motor::TurntableMotor;
pub use noise::VinylNoise;
pub use warmth::{AnalogWarmth, SaturationType};
pub use wow_flutter::WowFlutter;

/// Complete vinyl emulation system
///
/// Combines all vinyl effects into a cohesive simulation.
/// Enable/disable individual components as needed.
pub struct VinylEmulator {
    /// Master enable for all vinyl effects
    pub enabled: bool,

    /// Turntable motor physics
    pub motor: TurntableMotor,

    /// Wow and flutter pitch modulation
    pub wow_flutter: WowFlutter,

    /// Analog warmth (RIAA EQ, saturation, compression)
    pub warmth: AnalogWarmth,

    /// Vinyl surface noise
    pub noise: VinylNoise,

    /// Current preset
    current_preset: VinylPreset,

    /// Wet envelope for click-free enable/disable
    wet_target: f32,
    wet_current: f32,
}

impl VinylEmulator {
    /// Wet envelope smoothing coefficient (~10ms at 48kHz)
    const WET_SMOOTH_COEFF: f32 = 0.9995;

    /// Create a new vinyl emulator
    pub fn new(sample_rate: f32) -> Self {
        Self {
            enabled: false,
            motor: TurntableMotor::new(sample_rate),
            wow_flutter: WowFlutter::new(sample_rate),
            warmth: AnalogWarmth::new(sample_rate),
            noise: VinylNoise::new(sample_rate),
            current_preset: VinylPreset::default(),
            wet_target: 0.0,
            wet_current: 0.0,
        }
    }

    /// Enable/disable all vinyl effects (with smooth crossfade)
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.wet_target = if enabled { 1.0 } else { 0.0 };
        // Enable sub-components when master is enabled
        if enabled {
            self.wow_flutter.set_enabled(true);
            self.warmth.set_enabled(true);
            self.noise.set_enabled(true);
        }
        // Note: don't disable sub-components immediately - let them fade out
    }

    /// Check if enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set overall vinyl intensity (0.0-1.0)
    ///
    /// This scales all sub-components proportionally:
    /// - 0.0: No vinyl character
    /// - 0.5: Subtle warmth
    /// - 1.0: Full vintage vinyl sound
    pub fn set_intensity(&mut self, intensity: f32) {
        let i = intensity.clamp(0.0, 1.0);

        // Scale wow/flutter
        self.wow_flutter.set_intensity(i);

        // Scale warmth
        self.warmth.set_drive(i * 0.3);
        self.warmth.set_riaa_amount(i * 0.5);

        // Scale noise
        self.noise.set_intensity(i * 0.5);
    }

    /// Get current preset (if any)
    pub fn preset(&self) -> VinylPreset {
        self.current_preset
    }

    /// Apply preset
    pub fn set_preset(&mut self, preset: VinylPreset) {
        self.current_preset = preset;
        match preset {
            VinylPreset::Clean => {
                // Minimal vinyl character
                self.wow_flutter.set_intensity(0.2);
                self.warmth.set_drive(0.1);
                self.warmth.set_riaa_amount(0.2);
                self.noise.set_intensity(0.1);
            }
            VinylPreset::Warm => {
                // Warm but clean
                self.wow_flutter.set_intensity(0.4);
                self.warmth.set_drive(0.3);
                self.warmth.set_riaa_amount(0.4);
                self.noise.set_intensity(0.2);
            }
            VinylPreset::Vintage => {
                // Classic vinyl sound
                self.wow_flutter.set_intensity(0.6);
                self.warmth.set_drive(0.4);
                self.warmth.set_riaa_amount(0.6);
                self.noise.set_intensity(0.4);
            }
            VinylPreset::Worn => {
                // Old, worn record
                self.wow_flutter.set_intensity(0.8);
                self.warmth.set_drive(0.5);
                self.warmth.set_riaa_amount(0.7);
                self.noise.set_intensity(0.7);
            }
            VinylPreset::Extreme => {
                // Maximum vinyl character
                self.wow_flutter.set_intensity(1.0);
                self.warmth.set_drive(0.7);
                self.warmth.set_riaa_amount(0.8);
                self.noise.set_intensity(1.0);
            }
        }
    }

    /// Set wow/flutter amount (0.0-1.0)
    pub fn set_wow_amount(&mut self, amount: f32) {
        self.wow_flutter.set_intensity(amount.clamp(0.0, 1.0));
    }

    /// Set noise amount (0.0-1.0)
    pub fn set_noise_amount(&mut self, amount: f32) {
        self.noise.set_intensity(amount.clamp(0.0, 1.0));
    }

    /// Set warmth amount (0.0-1.0)
    pub fn set_warmth_amount(&mut self, amount: f32) {
        let a = amount.clamp(0.0, 1.0);
        self.warmth.set_drive(a * 0.5);
        self.warmth.set_riaa_amount(a);
    }

    /// Reset all state
    pub fn reset(&mut self) {
        self.motor.reset();
        self.wow_flutter.reset();
        self.warmth.reset();
        self.noise.reset();
    }

    /// Get combined speed multiplier (motor + wow/flutter)
    ///
    /// Use this value to multiply the playback position increment
    /// in the deck's process loop.
    #[inline]
    pub fn get_speed_multiplier(&mut self) -> f32 {
        if !self.enabled {
            return 1.0;
        }

        let motor_speed = self.motor.get_speed();
        let wow_flutter = self.wow_flutter.get_pitch_multiplier();

        motor_speed * wow_flutter
    }

    /// Process audio samples with warmth and noise
    ///
    /// Call this after deck playback to add vinyl character.
    /// The speed modulation is handled separately via get_speed_multiplier().
    pub fn process_audio(&mut self, samples: &mut [f32]) {
        // Skip processing only if fully disabled and envelope has settled
        if !self.enabled && self.wet_current < 0.0001 {
            // Disable sub-components once fully faded
            self.wow_flutter.set_enabled(false);
            self.warmth.set_enabled(false);
            self.noise.set_enabled(false);
            return;
        }

        // Process with wet envelope crossfade for click-free toggling
        for frame in samples.chunks_mut(2) {
            if frame.len() == 2 {
                // Smooth wet envelope toward target
                self.wet_current = Self::WET_SMOOTH_COEFF * self.wet_current
                    + (1.0 - Self::WET_SMOOTH_COEFF) * self.wet_target;

                // Save dry signal
                let dry_l = frame[0];
                let dry_r = frame[1];

                // Apply warmth (RIAA EQ, saturation, compression)
                self.warmth.process(frame);

                // Add vinyl noise
                self.noise.process(frame);

                // Crossfade between dry and wet based on envelope
                frame[0] = dry_l * (1.0 - self.wet_current) + frame[0] * self.wet_current;
                frame[1] = dry_r * (1.0 - self.wet_current) + frame[1] * self.wet_current;
            }
        }
    }

    /// Process audio samples (alias for process_audio)
    ///
    /// Implements the same interface as Effect trait for consistency.
    #[inline]
    pub fn process(&mut self, samples: &mut [f32]) {
        self.process_audio(samples);
    }

    /// Start playback (motor spin-up)
    pub fn play(&mut self) {
        self.motor.start();
    }

    /// Stop playback (motor brake)
    pub fn stop(&mut self) {
        self.motor.stop();
    }
}

/// Vinyl emulation presets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VinylPreset {
    /// Minimal character, almost clean
    Clean,
    /// Warm but not noisy
    #[default]
    Warm,
    /// Classic vintage vinyl
    Vintage,
    /// Old, worn record with more noise
    Worn,
    /// Maximum vinyl character
    Extreme,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vinyl_emulator_creation() {
        let vinyl = VinylEmulator::new(48000.0);
        assert!(!vinyl.is_enabled());
    }

    #[test]
    fn test_enable_enables_subcomponents() {
        let mut vinyl = VinylEmulator::new(48000.0);

        vinyl.set_enabled(true);
        assert!(vinyl.enabled);
        assert!(vinyl.wow_flutter.is_enabled());
        assert!(vinyl.warmth.is_enabled());
        assert!(vinyl.noise.is_enabled());

        // Disable - but sub-components stay enabled until fade-out completes
        vinyl.set_enabled(false);
        assert!(!vinyl.enabled);
        // Sub-components remain enabled for click-free fade-out
        assert!(vinyl.wow_flutter.is_enabled());
        assert!(vinyl.warmth.is_enabled());
        assert!(vinyl.noise.is_enabled());

        // Process enough samples to complete fade-out, then sub-components disable
        let mut samples = vec![0.0f32; 4096];
        for _ in 0..100 {
            vinyl.process_audio(&mut samples);
        }
        // After fade-out completes, sub-components should be disabled
        assert!(!vinyl.wow_flutter.is_enabled());
        assert!(!vinyl.warmth.is_enabled());
        assert!(!vinyl.noise.is_enabled());
    }

    #[test]
    fn test_disabled_speed_unity() {
        let mut vinyl = VinylEmulator::new(48000.0);
        vinyl.set_enabled(false);

        for _ in 0..1000 {
            assert_eq!(vinyl.get_speed_multiplier(), 1.0);
        }
    }

    #[test]
    fn test_enabled_speed_varies() {
        let mut vinyl = VinylEmulator::new(48000.0);
        vinyl.set_enabled(true);
        vinyl.set_intensity(1.0);

        let mut speeds = Vec::new();
        for _ in 0..4800 {
            speeds.push(vinyl.get_speed_multiplier());
        }

        let min = speeds.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = speeds.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        // Should vary slightly
        assert!(max > min);
    }

    #[test]
    fn test_presets() {
        let mut vinyl = VinylEmulator::new(48000.0);
        vinyl.set_enabled(true);

        // Test all presets compile and run
        vinyl.set_preset(VinylPreset::Clean);
        vinyl.set_preset(VinylPreset::Warm);
        vinyl.set_preset(VinylPreset::Vintage);
        vinyl.set_preset(VinylPreset::Worn);
        vinyl.set_preset(VinylPreset::Extreme);
    }

    #[test]
    fn test_process_audio() {
        let mut vinyl = VinylEmulator::new(48000.0);
        vinyl.set_enabled(true);
        vinyl.set_intensity(0.5);

        let mut samples = vec![0.5f32, 0.5, 0.3, 0.3, 0.1, 0.1];
        vinyl.process_audio(&mut samples);

        // Samples should be modified
        // (Note: warmth changes levels, noise adds signal)
    }
}
