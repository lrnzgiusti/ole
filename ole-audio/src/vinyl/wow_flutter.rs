//! Wow and Flutter - pitch modulation effects
//!
//! - Wow: Low frequency (0.5-2 Hz) pitch wobble from platter eccentricity
//! - Flutter: Higher frequency (5-15 Hz) from motor cogging
//!
//! Combined these create the characteristic "warmth" of vinyl playback.

use std::f32::consts::{LN_2, PI};

/// Wow and Flutter processor
pub struct WowFlutter {
    enabled: bool,
    sample_rate: f32,

    // Wow (slow wobble from platter eccentricity)
    wow_rate: f32,  // Hz (0.5-2.0)
    wow_depth: f32, // Semitones (0-0.5)
    wow_phase: f32,

    // Flutter (faster wobble from motor/belt)
    flutter_rate: f32,  // Hz (5-15)
    flutter_depth: f32, // Semitones (0-0.15)
    flutter_phase: f32,

    // Secondary flutter for complexity
    flutter2_rate: f32,
    flutter2_depth: f32,
    flutter2_phase: f32,

    // Random component (subtle noise in pitch)
    random_state: u32,
    random_depth: f32,
}

impl WowFlutter {
    /// Create new wow/flutter processor
    pub fn new(sample_rate: f32) -> Self {
        Self {
            enabled: false,
            sample_rate,
            // Wow: slow wobble ~0.8 Hz, 5 cents depth
            wow_rate: 0.8,
            wow_depth: 0.05,
            wow_phase: 0.0,
            // Flutter: faster wobble ~8 Hz, 2 cents depth
            flutter_rate: 8.0,
            flutter_depth: 0.02,
            flutter_phase: 0.0,
            // Secondary flutter at different rate for more organic feel
            flutter2_rate: 11.3,
            flutter2_depth: 0.01,
            flutter2_phase: 0.3, // Start offset
            // Random component
            random_state: 0xDEADBEEF,
            random_depth: 0.005, // 0.5 cents random
        }
    }

    /// Enable/disable wow and flutter
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Check if enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set wow rate (0.5-2.0 Hz)
    pub fn set_wow_rate(&mut self, rate: f32) {
        self.wow_rate = rate.clamp(0.3, 3.0);
    }

    /// Set wow depth in semitones (0-0.5)
    pub fn set_wow_depth(&mut self, depth: f32) {
        self.wow_depth = depth.clamp(0.0, 0.5);
    }

    /// Set flutter rate (5-15 Hz)
    pub fn set_flutter_rate(&mut self, rate: f32) {
        self.flutter_rate = rate.clamp(3.0, 20.0);
        // Adjust secondary flutter to be slightly different
        self.flutter2_rate = self.flutter_rate * 1.4;
    }

    /// Set flutter depth in semitones (0-0.15)
    pub fn set_flutter_depth(&mut self, depth: f32) {
        self.flutter_depth = depth.clamp(0.0, 0.2);
        self.flutter2_depth = depth * 0.5;
    }

    /// Set overall intensity (0.0-1.0)
    /// Scales both wow and flutter proportionally
    pub fn set_intensity(&mut self, intensity: f32) {
        let i = intensity.clamp(0.0, 1.0);
        // Scale depths based on intensity
        self.wow_depth = 0.05 * i;
        self.flutter_depth = 0.02 * i;
        self.flutter2_depth = 0.01 * i;
        self.random_depth = 0.005 * i;
    }

    /// Reset phase accumulators
    pub fn reset(&mut self) {
        self.wow_phase = 0.0;
        self.flutter_phase = 0.0;
        self.flutter2_phase = 0.3;
    }

    /// Simple xorshift PRNG (no allocation, deterministic)
    #[inline]
    fn next_random(&mut self) -> f32 {
        self.random_state ^= self.random_state << 13;
        self.random_state ^= self.random_state >> 17;
        self.random_state ^= self.random_state << 5;
        // Convert to [-1, 1] range
        (self.random_state as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Get pitch multiplier for this sample
    ///
    /// Returns a value around 1.0. Multiply this with your playback
    /// position increment to apply the wow/flutter effect.
    #[inline]
    pub fn get_pitch_multiplier(&mut self) -> f32 {
        if !self.enabled {
            return 1.0;
        }

        // Update phases
        let wow_inc = self.wow_rate / self.sample_rate;
        let flutter_inc = self.flutter_rate / self.sample_rate;
        let flutter2_inc = self.flutter2_rate / self.sample_rate;

        self.wow_phase += wow_inc;
        if self.wow_phase >= 1.0 {
            self.wow_phase -= 1.0;
        }

        self.flutter_phase += flutter_inc;
        if self.flutter_phase >= 1.0 {
            self.flutter_phase -= 1.0;
        }

        self.flutter2_phase += flutter2_inc;
        if self.flutter2_phase >= 1.0 {
            self.flutter2_phase -= 1.0;
        }

        // Calculate detuning in semitones
        let wow = (self.wow_phase * 2.0 * PI).sin() * self.wow_depth;
        let flutter = (self.flutter_phase * 2.0 * PI).sin() * self.flutter_depth;
        let flutter2 = (self.flutter2_phase * 2.0 * PI).sin() * self.flutter2_depth;
        let random = self.next_random() * self.random_depth;

        let total_semitones = wow + flutter + flutter2 + random;

        // Convert semitones to pitch multiplier: 2^(semitones/12)
        // Use fast approximation for small values
        fast_pow2(total_semitones / 12.0)
    }

    /// Process a buffer, filling with pitch multipliers
    pub fn process_buffer(&mut self, multipliers: &mut [f32]) {
        if !self.enabled {
            multipliers.fill(1.0);
            return;
        }

        for m in multipliers.iter_mut() {
            *m = self.get_pitch_multiplier();
        }
    }
}

/// Fast 2^x approximation for small x (|x| < 0.1)
/// Uses Taylor series expansion
#[inline]
fn fast_pow2(x: f32) -> f32 {
    // For very small x, use linear approximation
    if x.abs() < 0.001 {
        return 1.0 + x * LN_2;
    }

    // For slightly larger x, use quadratic approximation
    let x_ln2 = x * LN_2;
    1.0 + x_ln2 + 0.5 * x_ln2 * x_ln2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wow_flutter_creation() {
        let wf = WowFlutter::new(48000.0);
        assert!(!wf.is_enabled());
    }

    #[test]
    fn test_disabled_returns_unity() {
        let mut wf = WowFlutter::new(48000.0);
        wf.set_enabled(false);

        for _ in 0..1000 {
            assert_eq!(wf.get_pitch_multiplier(), 1.0);
        }
    }

    #[test]
    fn test_enabled_varies_pitch() {
        let mut wf = WowFlutter::new(48000.0);
        wf.set_enabled(true);

        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;

        for _ in 0..48000 {
            let m = wf.get_pitch_multiplier();
            min = min.min(m);
            max = max.max(m);
        }

        // Should vary around 1.0
        assert!(min < 1.0);
        assert!(max > 1.0);
        // But not too much
        assert!(min > 0.99);
        assert!(max < 1.01);
    }

    #[test]
    fn test_fast_pow2() {
        // Compare with real pow for small values
        for i in -10..=10 {
            let x = i as f32 * 0.01;
            let fast = fast_pow2(x);
            let real = 2.0f32.powf(x);
            assert!(
                (fast - real).abs() < 0.001,
                "fast_pow2({}) = {}, expected {}",
                x,
                fast,
                real
            );
        }
    }
}
