//! Stereo enhancer with M/S processing
//!
//! Features:
//! - Mid/Side encoding for width control
//! - Bass mono below configurable frequency (prevents phase issues on playback systems)
//! - Frequency-dependent width boost for high frequencies
//!
//! Parameters:
//! - Bass mono frequency: 80-200 Hz (default: 150 Hz)
//! - Width: 0.5-1.5 (default: 1.05)
//! - HF width boost: 0.0-0.3 (default: 0.05)

use std::f32::consts::PI;

use crate::effects::Effect;

/// Linkwitz-Riley 2nd order filter for bass mono crossover
#[derive(Clone)]
struct LowpassState {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Default for LowpassState {
    fn default() -> Self {
        Self {
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }
}

/// Stereo enhancer with M/S processing
pub struct StereoEnhancer {
    enabled: bool,
    sample_rate: f32,

    // Parameters
    bass_mono_freq: f32, // Bass mono crossover frequency (Hz)
    width: f32,          // Stereo width multiplier (0.5-1.5)
    hf_width_boost: f32, // Additional width for high frequencies (0.0-0.3)

    // Crossover filter coefficients (LPF for bass mono)
    lp_a0: f32,
    lp_a1: f32,
    lp_a2: f32,
    lp_b1: f32,
    lp_b2: f32,

    // Filter states for bass mono
    lp_state_l: LowpassState,
    lp_state_r: LowpassState,
    hp_state_l: LowpassState,
    hp_state_r: LowpassState,

    // Simple high-shelf detection for frequency-dependent width
    hf_env_l: f32,
    hf_env_r: f32,
    hf_coeff: f32,
}

impl StereoEnhancer {
    /// Create a new stereo enhancer
    pub fn new(sample_rate: f32) -> Self {
        let mut enhancer = Self {
            enabled: true,
            sample_rate,
            bass_mono_freq: 150.0,
            width: 1.05,
            hf_width_boost: 0.05,
            lp_a0: 1.0,
            lp_a1: 0.0,
            lp_a2: 0.0,
            lp_b1: 0.0,
            lp_b2: 0.0,
            lp_state_l: LowpassState::default(),
            lp_state_r: LowpassState::default(),
            hp_state_l: LowpassState::default(),
            hp_state_r: LowpassState::default(),
            hf_env_l: 0.0,
            hf_env_r: 0.0,
            hf_coeff: (-1.0 / (sample_rate * 0.01)).exp(), // ~10ms envelope
        };
        enhancer.calculate_crossover();
        enhancer
    }

    /// Set bass mono crossover frequency (80-200 Hz)
    pub fn set_bass_mono_freq(&mut self, freq: f32) {
        self.bass_mono_freq = freq.clamp(80.0, 200.0);
        self.calculate_crossover();
    }

    /// Get bass mono frequency
    pub fn bass_mono_freq(&self) -> f32 {
        self.bass_mono_freq
    }

    /// Set stereo width (0.5-1.5)
    pub fn set_width(&mut self, width: f32) {
        self.width = width.clamp(0.5, 1.5);
    }

    /// Get current width
    pub fn width(&self) -> f32 {
        self.width
    }

    /// Set HF width boost (0.0-0.3)
    pub fn set_hf_width_boost(&mut self, boost: f32) {
        self.hf_width_boost = boost.clamp(0.0, 0.3);
    }

    /// Get HF width boost
    pub fn hf_width_boost(&self) -> f32 {
        self.hf_width_boost
    }

    /// Calculate Butterworth lowpass coefficients for crossover
    fn calculate_crossover(&mut self) {
        let freq = self.bass_mono_freq;
        let q = 0.707; // Butterworth Q

        let omega = 2.0 * PI * freq / self.sample_rate;
        let sin_omega = omega.sin();
        let cos_omega = omega.cos();
        let alpha = sin_omega / (2.0 * q);

        let a0 = 1.0 + alpha;
        self.lp_a0 = ((1.0 - cos_omega) / 2.0) / a0;
        self.lp_a1 = (1.0 - cos_omega) / a0;
        self.lp_a2 = ((1.0 - cos_omega) / 2.0) / a0;
        self.lp_b1 = (-2.0 * cos_omega) / a0;
        self.lp_b2 = (1.0 - alpha) / a0;
    }

    /// L/R to M/S encode
    #[inline]
    fn encode_ms(left: f32, right: f32) -> (f32, f32) {
        let mid = (left + right) * 0.5;
        let side = (left - right) * 0.5;
        (mid, side)
    }

    /// M/S to L/R decode
    #[inline]
    fn decode_ms(mid: f32, side: f32) -> (f32, f32) {
        let left = mid + side;
        let right = mid - side;
        (left, right)
    }

    /// Process a stereo sample pair
    #[inline]
    fn process_sample(&mut self, left: f32, right: f32) -> (f32, f32) {
        // Split into low and high frequency bands using inline lowpass
        // Process left channel lowpass
        let low_l = {
            let state = &mut self.lp_state_l;
            let output = self.lp_a0 * left + self.lp_a1 * state.x1 + self.lp_a2 * state.x2
                - self.lp_b1 * state.y1
                - self.lp_b2 * state.y2;
            state.x2 = state.x1;
            state.x1 = left;
            state.y2 = state.y1;
            state.y1 = output;
            output
        };

        // Process right channel lowpass
        let low_r = {
            let state = &mut self.lp_state_r;
            let output = self.lp_a0 * right + self.lp_a1 * state.x1 + self.lp_a2 * state.x2
                - self.lp_b1 * state.y1
                - self.lp_b2 * state.y2;
            state.x2 = state.x1;
            state.x1 = right;
            state.y2 = state.y1;
            state.y1 = output;
            output
        };

        let high_l = left - low_l;
        let high_r = right - low_r;

        // Bass mono: sum lows to mono and distribute equally
        let low_mono = (low_l + low_r) * 0.5;

        // M/S encode the high frequencies for width control
        let (mid, side) = Self::encode_ms(high_l, high_r);

        // Calculate frequency-dependent width
        // Simple envelope following of high frequency content
        let hf_level = (high_l.abs() + high_r.abs()) * 0.5;
        self.hf_env_l = self.hf_coeff * self.hf_env_l + (1.0 - self.hf_coeff) * hf_level;

        // Width increases slightly with HF content
        let hf_boost = self.hf_env_l.min(1.0) * self.hf_width_boost;
        let effective_width = self.width + hf_boost;

        // Apply width to side channel
        let side_processed = side * effective_width;

        // M/S decode back to L/R
        let (proc_l, proc_r) = Self::decode_ms(mid, side_processed);

        // Combine bass mono with processed highs
        (low_mono + proc_l, low_mono + proc_r)
    }
}

impl Effect for StereoEnhancer {
    fn process(&mut self, samples: &mut [f32]) {
        if !self.enabled {
            return;
        }

        for frame in samples.chunks_exact_mut(2) {
            let (out_l, out_r) = self.process_sample(frame[0], frame[1]);
            frame[0] = out_l;
            frame[1] = out_r;
        }
    }

    fn reset(&mut self) {
        self.lp_state_l = LowpassState::default();
        self.lp_state_r = LowpassState::default();
        self.hp_state_l = LowpassState::default();
        self.hp_state_r = LowpassState::default();
        self.hf_env_l = 0.0;
        self.hf_env_r = 0.0;
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
        "StereoEnhancer"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stereo_enhancer_creation() {
        let enhancer = StereoEnhancer::new(48000.0);
        assert!(enhancer.is_enabled());
        assert!((enhancer.width() - 1.05).abs() < 0.01);
        assert!((enhancer.bass_mono_freq() - 150.0).abs() < 0.01);
    }

    #[test]
    fn test_disabled_passthrough() {
        let mut enhancer = StereoEnhancer::new(48000.0);
        enhancer.set_enabled(false);

        let mut samples = vec![0.5, 0.5, 0.3, 0.3];
        let original = samples.clone();
        enhancer.process(&mut samples);

        assert_eq!(samples, original);
    }

    #[test]
    fn test_ms_encode_decode() {
        // Test that encode/decode is reversible
        let left = 0.7;
        let right = 0.3;

        let (mid, side) = StereoEnhancer::encode_ms(left, right);
        let (decoded_l, decoded_r) = StereoEnhancer::decode_ms(mid, side);

        assert!((decoded_l - left).abs() < 0.0001);
        assert!((decoded_r - right).abs() < 0.0001);
    }

    #[test]
    fn test_mono_signal_unchanged_at_unity_width() {
        let mut enhancer = StereoEnhancer::new(48000.0);
        enhancer.set_width(1.0);
        enhancer.set_hf_width_boost(0.0);

        // Mono signal (same L and R) should remain mono
        let mut samples: Vec<f32> = (0..200)
            .flat_map(|i| {
                let val = (i as f32 * 0.01).sin() * 0.5;
                vec![val, val] // Mono signal
            })
            .collect();

        enhancer.process(&mut samples);

        // After processing, L and R should still be similar (except for bass mono processing)
        for chunk in samples.chunks(2) {
            // Allow some difference due to filter phase
            assert!(
                (chunk[0] - chunk[1]).abs() < 0.1,
                "L={} R={} differ too much",
                chunk[0],
                chunk[1]
            );
        }
    }

    #[test]
    fn test_width_affects_stereo_content() {
        let mut enhancer_narrow = StereoEnhancer::new(48000.0);
        enhancer_narrow.set_width(0.5);
        enhancer_narrow.set_hf_width_boost(0.0);
        enhancer_narrow.set_bass_mono_freq(80.0); // Low to minimize bass mono effect

        let mut enhancer_wide = StereoEnhancer::new(48000.0);
        enhancer_wide.set_width(1.5);
        enhancer_wide.set_hf_width_boost(0.0);
        enhancer_wide.set_bass_mono_freq(80.0);

        // Stereo signal with L/R difference (high frequency)
        let create_samples = || -> Vec<f32> {
            (0..200)
                .flat_map(|i| {
                    let base = (i as f32 * 0.1).sin() * 0.5;
                    vec![base * 0.8, base * 0.2] // Different L and R
                })
                .collect()
        };

        let mut samples_narrow = create_samples();
        let mut samples_wide = create_samples();

        enhancer_narrow.process(&mut samples_narrow);
        enhancer_wide.process(&mut samples_wide);

        // Wide should have more L/R difference than narrow
        let diff_narrow: f32 = samples_narrow.chunks(2).map(|c| (c[0] - c[1]).abs()).sum();
        let diff_wide: f32 = samples_wide.chunks(2).map(|c| (c[0] - c[1]).abs()).sum();

        // The wide processing should preserve more stereo difference
        assert!(
            diff_wide > diff_narrow * 1.2,
            "Wide diff {} should be > narrow diff {} * 1.2",
            diff_wide,
            diff_narrow
        );
    }

    #[test]
    fn test_parameter_clamping() {
        let mut enhancer = StereoEnhancer::new(48000.0);

        enhancer.set_width(3.0);
        assert!((enhancer.width() - 1.5).abs() < 0.01);

        enhancer.set_width(0.0);
        assert!((enhancer.width() - 0.5).abs() < 0.01);

        enhancer.set_bass_mono_freq(20.0);
        assert!((enhancer.bass_mono_freq() - 80.0).abs() < 0.01);

        enhancer.set_bass_mono_freq(500.0);
        assert!((enhancer.bass_mono_freq() - 200.0).abs() < 0.01);
    }
}
