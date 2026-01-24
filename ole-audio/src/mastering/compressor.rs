//! Mastering compressor
//!
//! A gentle "glue" compressor designed for transparent bus compression.
//! Features:
//! - Soft knee for smooth onset
//! - Program-dependent attack/release (adapts to transients)
//! - Sidechain HPF (60Hz) to prevent kick-driven pumping
//! - Optional look-ahead for transparent peak catching

use std::f32::consts::PI;

use crate::effects::Effect;

/// Biquad state for sidechain HPF
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

/// Mastering compressor
pub struct MasteringCompressor {
    enabled: bool,
    sample_rate: f32,

    // Parameters
    threshold: f32, // Threshold in dB (-20 to 0)
    ratio: f32,     // Compression ratio (1.1 to 2.5)
    knee: f32,      // Knee width in dB (0-12)
    attack: f32,    // Attack time in ms (5-50)
    release: f32,   // Release time in ms (50-300)

    // Computed coefficients
    attack_coeff: f32,
    release_coeff: f32,

    // Sidechain HPF (60Hz) - prevents kick-driven pumping
    sc_hpf_a0: f32,
    sc_hpf_a1: f32,
    sc_hpf_a2: f32,
    sc_hpf_b1: f32,
    sc_hpf_b2: f32,
    sc_hpf_state_l: BiquadState,
    sc_hpf_state_r: BiquadState,

    // Envelope follower state
    envelope: f32,

    // Gain smoothing
    gain_smooth: f32,
    gain_smooth_coeff: f32,

    // Look-ahead buffer (optional, small for mastering)
    lookahead_enabled: bool,
    lookahead_samples: usize,
    lookahead_buffer_l: Vec<f32>,
    lookahead_buffer_r: Vec<f32>,
    lookahead_write_pos: usize,

    // Metering
    current_gr_db: f32,

    // Makeup gain (auto-calculated based on compression settings)
    makeup_gain: f32,
}

impl MasteringCompressor {
    /// Create a new mastering compressor
    pub fn new(sample_rate: f32) -> Self {
        let mut comp = Self {
            enabled: true,
            sample_rate,
            threshold: -12.0,
            ratio: 1.5,
            knee: 6.0,
            attack: 20.0,
            release: 150.0,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            sc_hpf_a0: 1.0,
            sc_hpf_a1: 0.0,
            sc_hpf_a2: 0.0,
            sc_hpf_b1: 0.0,
            sc_hpf_b2: 0.0,
            sc_hpf_state_l: BiquadState::default(),
            sc_hpf_state_r: BiquadState::default(),
            envelope: 0.0,
            gain_smooth: 1.0,
            gain_smooth_coeff: 0.9995,
            lookahead_enabled: true,
            lookahead_samples: (sample_rate * 0.001) as usize, // 1ms lookahead
            lookahead_buffer_l: vec![0.0; (sample_rate * 0.003) as usize],
            lookahead_buffer_r: vec![0.0; (sample_rate * 0.003) as usize],
            lookahead_write_pos: 0,
            current_gr_db: 0.0,
            makeup_gain: 1.0,
        };
        comp.update_coefficients();
        comp.calculate_sidechain_hpf();
        comp
    }

    /// Set threshold in dB (-20 to 0)
    pub fn set_threshold(&mut self, db: f32) {
        self.threshold = db.clamp(-20.0, 0.0);
        self.calculate_makeup_gain();
    }

    /// Set compression ratio (1.1 to 2.5)
    pub fn set_ratio(&mut self, ratio: f32) {
        self.ratio = ratio.clamp(1.1, 2.5);
        self.calculate_makeup_gain();
    }

    /// Set knee width in dB (0-12)
    pub fn set_knee(&mut self, knee_db: f32) {
        self.knee = knee_db.clamp(0.0, 12.0);
    }

    /// Set attack time in ms (5-50)
    pub fn set_attack_ms(&mut self, ms: f32) {
        self.attack = ms.clamp(5.0, 50.0);
        self.update_coefficients();
    }

    /// Set release time in ms (50-300)
    pub fn set_release_ms(&mut self, ms: f32) {
        self.release = ms.clamp(50.0, 300.0);
        self.update_coefficients();
    }

    /// Enable/disable look-ahead
    pub fn set_lookahead(&mut self, enabled: bool) {
        self.lookahead_enabled = enabled;
        if !enabled {
            self.lookahead_buffer_l.fill(0.0);
            self.lookahead_buffer_r.fill(0.0);
        }
    }

    /// Get current gain reduction in dB (for metering)
    pub fn gain_reduction_db(&self) -> f32 {
        self.current_gr_db
    }

    /// Update time constants
    fn update_coefficients(&mut self) {
        let attack_secs = self.attack / 1000.0;
        let release_secs = self.release / 1000.0;

        self.attack_coeff = (-1.0 / (self.sample_rate * attack_secs)).exp();
        self.release_coeff = (-1.0 / (self.sample_rate * release_secs)).exp();
    }

    /// Calculate sidechain HPF coefficients (60Hz, 2nd order Butterworth)
    fn calculate_sidechain_hpf(&mut self) {
        let freq = 60.0;
        let q = 0.707; // Butterworth Q

        let omega = 2.0 * PI * freq / self.sample_rate;
        let sin_omega = omega.sin();
        let cos_omega = omega.cos();
        let alpha = sin_omega / (2.0 * q);

        let a0 = 1.0 + alpha;
        self.sc_hpf_a0 = ((1.0 + cos_omega) / 2.0) / a0;
        self.sc_hpf_a1 = (-(1.0 + cos_omega)) / a0;
        self.sc_hpf_a2 = ((1.0 + cos_omega) / 2.0) / a0;
        self.sc_hpf_b1 = (-2.0 * cos_omega) / a0;
        self.sc_hpf_b2 = (1.0 - alpha) / a0;
    }

    /// Calculate automatic makeup gain based on compression settings
    fn calculate_makeup_gain(&mut self) {
        // Estimate average gain reduction and compensate
        // This is a simplified calculation assuming typical program material
        let avg_compression_db = (self.threshold.abs() * (1.0 - 1.0 / self.ratio)) / 4.0;
        self.makeup_gain = Self::db_to_linear(avg_compression_db.min(6.0));
    }

    /// Convert dB to linear
    #[inline]
    fn db_to_linear(db: f32) -> f32 {
        10.0f32.powf(db / 20.0)
    }

    /// Convert linear to dB
    #[inline]
    fn linear_to_db(linear: f32) -> f32 {
        if linear > 1e-10 {
            20.0 * linear.log10()
        } else {
            -200.0
        }
    }

    /// Compute gain reduction with soft knee
    #[inline]
    fn compute_gain_reduction(&self, input_db: f32) -> f32 {
        let threshold = self.threshold;
        let ratio = self.ratio;
        let knee = self.knee;

        if input_db < threshold - knee / 2.0 {
            // Below knee - no compression
            0.0
        } else if input_db > threshold + knee / 2.0 {
            // Above knee - full compression
            threshold + (input_db - threshold) / ratio - input_db
        } else {
            // In knee - soft transition
            let knee_start = threshold - knee / 2.0;
            let x = input_db - knee_start;
            // Quadratic knee curve
            let compression = (1.0 / ratio - 1.0) * (x * x) / (2.0 * knee);
            compression
        }
    }

    /// Process a single stereo sample pair
    #[inline]
    fn process_sample(&mut self, left: f32, right: f32) -> (f32, f32) {
        // Apply sidechain HPF to prevent kick-driven pumping
        let sc_l = self.sc_hpf_state_l.process(
            left,
            self.sc_hpf_a0,
            self.sc_hpf_a1,
            self.sc_hpf_a2,
            self.sc_hpf_b1,
            self.sc_hpf_b2,
        );
        let sc_r = self.sc_hpf_state_r.process(
            right,
            self.sc_hpf_a0,
            self.sc_hpf_a1,
            self.sc_hpf_a2,
            self.sc_hpf_b1,
            self.sc_hpf_b2,
        );

        // Peak detection on sidechain (linked stereo)
        let peak = sc_l.abs().max(sc_r.abs());
        let peak_db = Self::linear_to_db(peak);

        // Compute target gain reduction
        let gr_db = self.compute_gain_reduction(peak_db);
        let target_gain = Self::db_to_linear(gr_db);

        // Envelope follower with program-dependent timing
        // Adapts release based on how much compression is happening
        let is_attack = target_gain < self.envelope;
        let coeff = if is_attack {
            self.attack_coeff
        } else {
            // Program-dependent release: slower release when compressing more
            let gr_factor = 1.0 + (-gr_db / 10.0).min(1.0);
            self.release_coeff.powf(1.0 / gr_factor)
        };

        self.envelope = coeff * self.envelope + (1.0 - coeff) * target_gain;

        // Apply gain smoothing to avoid zipper noise
        self.gain_smooth = self.gain_smooth_coeff * self.gain_smooth
            + (1.0 - self.gain_smooth_coeff) * self.envelope;

        // Update metering
        self.current_gr_db = Self::linear_to_db(self.gain_smooth);

        // Get output samples (from lookahead buffer if enabled)
        let (out_l, out_r) = if self.lookahead_enabled {
            let read_pos = self.lookahead_write_pos;

            // Write current samples
            self.lookahead_buffer_l[self.lookahead_write_pos] = left;
            self.lookahead_buffer_r[self.lookahead_write_pos] = right;

            // Advance write position
            self.lookahead_write_pos = (self.lookahead_write_pos + 1) % self.lookahead_samples;

            // Read delayed samples
            let read_idx = (read_pos + 1) % self.lookahead_samples;
            (
                self.lookahead_buffer_l[read_idx],
                self.lookahead_buffer_r[read_idx],
            )
        } else {
            (left, right)
        };

        // Apply gain and makeup
        let gain = self.gain_smooth * self.makeup_gain;
        (out_l * gain, out_r * gain)
    }
}

impl Effect for MasteringCompressor {
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
        self.envelope = 0.0;
        self.gain_smooth = 1.0;
        self.current_gr_db = 0.0;
        self.sc_hpf_state_l.reset();
        self.sc_hpf_state_r.reset();
        self.lookahead_buffer_l.fill(0.0);
        self.lookahead_buffer_r.fill(0.0);
        self.lookahead_write_pos = 0;
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
        "MasteringCompressor"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compressor_creation() {
        let comp = MasteringCompressor::new(48000.0);
        assert!(comp.is_enabled());
        assert!((comp.threshold - (-12.0)).abs() < 0.01);
        assert!((comp.ratio - 1.5).abs() < 0.01);
    }

    #[test]
    fn test_disabled_passthrough() {
        let mut comp = MasteringCompressor::new(48000.0);
        comp.set_enabled(false);

        let mut samples = vec![0.5, 0.5, 0.3, 0.3];
        let original = samples.clone();
        comp.process(&mut samples);

        assert_eq!(samples, original);
    }

    #[test]
    fn test_soft_signal_minimal_compression() {
        let mut comp = MasteringCompressor::new(48000.0);
        comp.set_lookahead(false); // Disable for simpler test

        // Quiet signal should have minimal compression
        let mut samples: Vec<f32> = vec![0.1; 1000];
        comp.process(&mut samples);

        // With makeup gain, output should be similar or slightly higher
        // Signal at -20dB should be below -12dB threshold
        assert!(samples.iter().all(|&s| s.abs() < 0.5));
    }

    #[test]
    fn test_loud_signal_compression() {
        let mut comp = MasteringCompressor::new(48000.0);
        comp.set_lookahead(false);
        comp.set_threshold(-6.0);
        comp.set_ratio(2.0);

        // Loud signal should be compressed
        let mut samples: Vec<f32> = vec![0.9, 0.9, -0.9, -0.9];
        samples.extend(vec![0.9; 1000]); // Let envelope settle

        comp.process(&mut samples);

        // Should show some gain reduction
        let gr = comp.gain_reduction_db();
        assert!(gr < 0.0, "Expected gain reduction, got {} dB", gr);
    }

    #[test]
    fn test_parameter_clamping() {
        let mut comp = MasteringCompressor::new(48000.0);

        comp.set_threshold(-50.0);
        assert!((comp.threshold - (-20.0)).abs() < 0.01);

        comp.set_ratio(10.0);
        assert!((comp.ratio - 2.5).abs() < 0.01);

        comp.set_attack_ms(1.0);
        assert!((comp.attack - 5.0).abs() < 0.01);
    }
}
