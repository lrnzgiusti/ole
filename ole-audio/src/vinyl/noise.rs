//! Vinyl noise generator
//!
//! Generates characteristic vinyl playback noise:
//! - Surface noise (continuous hiss)
//! - Crackle (random clicks)
//! - Pops (louder transients)

/// Vinyl noise generator
pub struct VinylNoise {
    enabled: bool,
    sample_rate: f32,

    // Noise levels (0.0-1.0)
    surface_level: f32,   // Continuous hiss
    crackle_level: f32,   // Random clicks
    pop_level: f32,       // Occasional louder pops

    // PRNG state (deterministic, no allocation)
    random_state: u64,

    // Surface noise filter state (pink-ish noise)
    noise_filter_b0: f32,
    noise_filter_b1: f32,
    noise_filter_b2: f32,

    // Pop timing
    samples_until_next_pop: u32,
    current_pop_amplitude: f32,
    pop_decay: f32,

    // Crackle probability (per sample)
    crackle_probability: f32,
}

impl VinylNoise {
    /// Create new vinyl noise generator
    pub fn new(sample_rate: f32) -> Self {
        Self {
            enabled: false,
            sample_rate,
            surface_level: 0.008,   // Very subtle
            crackle_level: 0.015,
            pop_level: 0.04,
            random_state: 0xDEADBEEF_CAFEBABE,
            noise_filter_b0: 0.0,
            noise_filter_b1: 0.0,
            noise_filter_b2: 0.0,
            samples_until_next_pop: 0,
            current_pop_amplitude: 0.0,
            pop_decay: 0.0,
            crackle_probability: 0.0001, // 0.01% per sample
        }
    }

    /// Enable/disable noise
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if enabled {
            self.reset_pop_timer();
        }
    }

    /// Check if enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set surface noise level (0.0-1.0)
    pub fn set_surface_level(&mut self, level: f32) {
        self.surface_level = level.clamp(0.0, 1.0) * 0.02;
    }

    /// Set crackle level (0.0-1.0)
    pub fn set_crackle_level(&mut self, level: f32) {
        self.crackle_level = level.clamp(0.0, 1.0) * 0.05;
        self.crackle_probability = level.clamp(0.0, 1.0) * 0.0005;
    }

    /// Set pop level (0.0-1.0)
    pub fn set_pop_level(&mut self, level: f32) {
        self.pop_level = level.clamp(0.0, 1.0) * 0.1;
    }

    /// Set overall intensity (0.0-1.0)
    pub fn set_intensity(&mut self, intensity: f32) {
        let i = intensity.clamp(0.0, 1.0);
        self.surface_level = i * 0.01;
        self.crackle_level = i * 0.02;
        self.pop_level = i * 0.05;
        self.crackle_probability = i * 0.0003;
    }

    /// Reset filter state
    pub fn reset(&mut self) {
        self.noise_filter_b0 = 0.0;
        self.noise_filter_b1 = 0.0;
        self.noise_filter_b2 = 0.0;
        self.current_pop_amplitude = 0.0;
        self.reset_pop_timer();
    }

    /// Schedule next pop
    fn reset_pop_timer(&mut self) {
        // Random interval 0.5-5 seconds
        let interval = 0.5 + self.next_random() * 4.5;
        self.samples_until_next_pop = (interval * self.sample_rate) as u32;
        // Calculate decay rate (pop lasts ~10-30ms)
        let pop_duration = 0.01 + self.next_random() * 0.02;
        self.pop_decay = 1.0 - (1.0 / (pop_duration * self.sample_rate));
    }

    /// xorshift64 PRNG (no allocation, fast)
    #[inline]
    fn next_random(&mut self) -> f32 {
        self.random_state ^= self.random_state << 13;
        self.random_state ^= self.random_state >> 7;
        self.random_state ^= self.random_state << 17;
        (self.random_state as f32) / (u64::MAX as f32)
    }

    /// Get white noise sample in [-1, 1]
    #[inline]
    fn white_noise(&mut self) -> f32 {
        self.next_random() * 2.0 - 1.0
    }

    /// Get pink-ish noise (filtered white noise)
    /// Uses a simple 3-stage IIR filter approximation
    #[inline]
    fn pink_noise(&mut self) -> f32 {
        let white = self.white_noise();

        // Paul Kellet's economy pink noise filter
        self.noise_filter_b0 = 0.99886 * self.noise_filter_b0 + white * 0.0555179;
        self.noise_filter_b1 = 0.99332 * self.noise_filter_b1 + white * 0.0750759;
        self.noise_filter_b2 = 0.96900 * self.noise_filter_b2 + white * 0.1538520;

        let pink = self.noise_filter_b0 + self.noise_filter_b1 + self.noise_filter_b2 + white * 0.5362;

        // Normalize (pink noise has higher amplitude)
        pink * 0.11
    }

    /// Get a single mono noise sample
    #[inline]
    pub fn get_sample(&mut self) -> f32 {
        if !self.enabled {
            return 0.0;
        }

        let mut output = 0.0;

        // Surface noise (pink-ish)
        if self.surface_level > 0.0001 {
            output += self.pink_noise() * self.surface_level;
        }

        // Crackle (random impulses)
        if self.next_random() < self.crackle_probability {
            // Random click amplitude and polarity
            let click = (self.next_random() - 0.5) * 2.0 * self.crackle_level;
            output += click;
        }

        // Pop (scheduled larger impulses with decay)
        if self.samples_until_next_pop == 0 {
            // Trigger new pop
            self.current_pop_amplitude = self.pop_level * (0.5 + self.next_random() * 0.5);
            self.reset_pop_timer();
        } else {
            self.samples_until_next_pop -= 1;
        }

        if self.current_pop_amplitude > 0.0001 {
            // Pop waveform (decaying)
            output += self.current_pop_amplitude * (self.next_random() - 0.5);
            self.current_pop_amplitude *= self.pop_decay;
        }

        output
    }

    /// Process a buffer of stereo samples, adding noise
    pub fn process(&mut self, samples: &mut [f32]) {
        if !self.enabled {
            return;
        }

        for frame in samples.chunks_mut(2) {
            let noise = self.get_sample();

            // Add same noise to both channels with slight variation
            if frame.len() >= 1 {
                frame[0] += noise;
            }
            if frame.len() >= 2 {
                // Slight stereo variation for more natural feel
                let variation = 1.0 + (self.next_random() - 0.5) * 0.1;
                frame[1] += noise * variation;
            }
        }
    }

    /// Get stereo noise samples (returns left, right)
    #[inline]
    pub fn get_stereo_sample(&mut self) -> (f32, f32) {
        let mono = self.get_sample();
        let variation = 1.0 + (self.next_random() - 0.5) * 0.1;
        (mono, mono * variation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noise_creation() {
        let noise = VinylNoise::new(48000.0);
        assert!(!noise.is_enabled());
    }

    #[test]
    fn test_disabled_silent() {
        let mut noise = VinylNoise::new(48000.0);
        noise.set_enabled(false);

        for _ in 0..1000 {
            assert_eq!(noise.get_sample(), 0.0);
        }
    }

    #[test]
    fn test_enabled_produces_noise() {
        let mut noise = VinylNoise::new(48000.0);
        noise.set_enabled(true);
        noise.set_intensity(1.0);

        let mut sum = 0.0;
        for _ in 0..10000 {
            sum += noise.get_sample().abs();
        }

        // Should produce some noise
        assert!(sum > 0.0);
    }

    #[test]
    fn test_pink_noise_distribution() {
        let mut noise = VinylNoise::new(48000.0);
        noise.set_enabled(true);

        let mut samples = Vec::new();
        for _ in 0..10000 {
            samples.push(noise.pink_noise());
        }

        // Check that noise is roughly centered around zero
        let mean: f32 = samples.iter().sum::<f32>() / samples.len() as f32;
        assert!(mean.abs() < 0.1);

        // Check that we have variation
        let variance: f32 = samples.iter().map(|s| (s - mean).powi(2)).sum::<f32>() / samples.len() as f32;
        assert!(variance > 0.001);
    }

    #[test]
    fn test_process_adds_noise() {
        let mut noise = VinylNoise::new(48000.0);
        noise.set_enabled(true);
        noise.set_intensity(1.0);

        let mut samples = vec![0.0f32; 100];
        noise.process(&mut samples);

        // Some samples should be non-zero
        let non_zero = samples.iter().filter(|&&s| s != 0.0).count();
        assert!(non_zero > 0);
    }
}
