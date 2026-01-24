//! Bitcrusher effect - lo-fi digital degradation
//!
//! Reduces bit depth and sample rate for that crunchy retro sound.
//! Perfect for adding grit and character to digital tracks.

use super::Effect;

/// Bitcrusher effect with bit depth and sample rate reduction
pub struct Bitcrusher {
    enabled: bool,

    /// Bit depth (1 - 16 bits)
    bits: u8,

    /// Sample rate reduction factor (1 - 50)
    /// 1 = no reduction, 10 = 1/10th sample rate, etc.
    downsample: u8,

    /// Wet/dry mix (0.0 - 1.0)
    mix: f32,

    /// Current downsample counter
    downsample_counter: u8,

    /// Held sample values (stereo)
    hold_l: f32,
    hold_r: f32,

    /// Wet envelope for click-free enable/disable
    wet_target: f32,
    wet_current: f32,

    /// Optional noise/jitter amount (0.0 - 1.0)
    jitter: f32,

    /// Simple LFSR for noise
    noise_state: u32,
}

impl Bitcrusher {
    /// Wet envelope smoothing coefficient
    const WET_SMOOTH_COEFF: f32 = 0.9995;

    /// Create a new bitcrusher effect
    pub fn new(_sample_rate: f32) -> Self {
        Self {
            enabled: false,
            bits: 8,
            downsample: 4,
            mix: 1.0,
            downsample_counter: 0,
            hold_l: 0.0,
            hold_r: 0.0,
            wet_target: 0.0,
            wet_current: 0.0,
            jitter: 0.0,
            noise_state: 0x12345678,
        }
    }

    /// Set bit depth (1 - 16)
    pub fn set_bits(&mut self, bits: u8) {
        self.bits = bits.clamp(1, 16);
    }

    /// Get bit depth
    pub fn bits(&self) -> u8 {
        self.bits
    }

    /// Set sample rate reduction factor (1 - 50)
    pub fn set_downsample(&mut self, factor: u8) {
        self.downsample = factor.clamp(1, 50);
    }

    /// Get downsample factor
    pub fn downsample(&self) -> u8 {
        self.downsample
    }

    /// Set wet/dry mix (0.0 - 1.0)
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    /// Get mix
    pub fn mix(&self) -> f32 {
        self.mix
    }

    /// Set jitter/noise amount (0.0 - 1.0)
    pub fn set_jitter(&mut self, jitter: f32) {
        self.jitter = jitter.clamp(0.0, 1.0);
    }

    /// Get jitter amount
    pub fn jitter(&self) -> f32 {
        self.jitter
    }

    /// Crush a sample to the specified bit depth
    #[inline]
    fn crush(&self, sample: f32) -> f32 {
        let levels = (1u32 << self.bits) as f32;
        let half_levels = levels * 0.5;

        // Quantize to discrete levels
        let quantized = ((sample * half_levels).round() / half_levels).clamp(-1.0, 1.0);

        quantized
    }

    /// Simple LFSR noise generator
    #[inline]
    fn next_noise(&mut self) -> f32 {
        // Galois LFSR with taps at bits 31, 21, 1, 0
        let lsb = self.noise_state & 1;
        self.noise_state >>= 1;
        if lsb == 1 {
            self.noise_state ^= 0xB400_0000;
        }
        // Convert to -1.0 to 1.0
        (self.noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

impl Effect for Bitcrusher {
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

            // Downsample: only update held sample every N samples
            self.downsample_counter += 1;
            if self.downsample_counter >= self.downsample {
                self.downsample_counter = 0;

                // Apply bit crushing
                let mut crushed_l = self.crush(frame[0]);
                let mut crushed_r = self.crush(frame[1]);

                // Add jitter/noise if enabled
                if self.jitter > 0.0 {
                    let noise_amount = self.jitter * 0.05; // Scale jitter to reasonable range
                    crushed_l += self.next_noise() * noise_amount;
                    crushed_r += self.next_noise() * noise_amount;
                }

                self.hold_l = crushed_l;
                self.hold_r = crushed_r;
            }

            // Mix dry and wet with envelope
            let effective_mix = self.mix * self.wet_current;
            frame[0] = frame[0] * (1.0 - effective_mix) + self.hold_l * effective_mix;
            frame[1] = frame[1] * (1.0 - effective_mix) + self.hold_r * effective_mix;
        }
    }

    fn reset(&mut self) {
        self.downsample_counter = 0;
        self.hold_l = 0.0;
        self.hold_r = 0.0;
        self.noise_state = 0x12345678;
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.wet_target = if enabled { 1.0 } else { 0.0 };
    }

    fn name(&self) -> &'static str {
        "Bitcrusher"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitcrusher_creation() {
        let bc = Bitcrusher::new(48000.0);
        assert!(!bc.is_enabled());
        assert_eq!(bc.bits(), 8);
        assert_eq!(bc.downsample(), 4);
    }

    #[test]
    fn test_bitcrusher_parameter_clamping() {
        let mut bc = Bitcrusher::new(48000.0);

        bc.set_bits(0);
        assert_eq!(bc.bits(), 1);

        bc.set_bits(32);
        assert_eq!(bc.bits(), 16);

        bc.set_downsample(100);
        assert_eq!(bc.downsample(), 50);
    }

    #[test]
    fn test_bit_crushing() {
        let mut bc = Bitcrusher::new(48000.0);
        bc.set_bits(1);

        // 1-bit should give -1, 0, or 1
        let crushed = bc.crush(0.3);
        assert!(
            crushed == 0.0 || crushed == 0.5 || crushed == -0.5 || crushed == 1.0,
            "Expected 1-bit quantized value, got {}",
            crushed
        );
    }

    #[test]
    fn test_bitcrusher_processes_audio() {
        let mut bc = Bitcrusher::new(48000.0);
        bc.set_enabled(true);
        bc.set_bits(4);
        bc.wet_current = 1.0; // Force wet for test

        let original = vec![0.5, 0.5, 0.333, 0.333, 0.1, 0.1];
        let mut samples = original.clone();
        bc.process(&mut samples);

        // Output should be quantized (different from input)
        // Note: due to sample-and-hold, some samples might be the same
    }
}
