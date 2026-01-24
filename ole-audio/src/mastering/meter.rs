//! ITU-R BS.1770 Loudness meter
//!
//! Implements LUFS (Loudness Units Full Scale) metering according to
//! ITU-R BS.1770-4 / EBU R128 specifications.
//!
//! Provides:
//! - Momentary loudness (400ms window)
//! - Short-term loudness (3s window)
//! - True peak measurement
//!
//! Note: Integrated loudness (program loudness) is not implemented as it
//! requires gating and is less useful for real-time DJ applications.

use std::f32::consts::PI;

/// LUFS measurement values
#[derive(Debug, Clone, Copy, Default)]
pub struct LufsValues {
    /// Momentary loudness (400ms window) in LUFS
    pub momentary: f32,
    /// Short-term loudness (3s window) in LUFS
    pub short_term: f32,
    /// True peak in dBFS
    pub true_peak: f32,
}

/// Ring buffer for storing samples
struct RingBuffer {
    buffer: Vec<f32>,
    write_pos: usize,
    len: usize,
}

impl RingBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            buffer: vec![0.0; capacity],
            write_pos: 0,
            len: 0,
        }
    }

    fn push(&mut self, value: f32) {
        self.buffer[self.write_pos] = value;
        self.write_pos = (self.write_pos + 1) % self.buffer.len();
        if self.len < self.buffer.len() {
            self.len += 1;
        }
    }

    fn sum(&self) -> f32 {
        self.buffer[..self.len].iter().sum()
    }

    fn len(&self) -> usize {
        self.len
    }

    fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
        self.len = 0;
    }
}

/// Biquad filter state
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

/// ITU-R BS.1770 loudness meter
pub struct LoudnessMeter {
    sample_rate: u32,

    // K-weighting filter coefficients (pre-filter: high shelf)
    pre_a0: f32,
    pre_a1: f32,
    pre_a2: f32,
    pre_b1: f32,
    pre_b2: f32,

    // K-weighting filter coefficients (RLB: high-pass)
    rlb_a0: f32,
    rlb_a1: f32,
    rlb_a2: f32,
    rlb_b1: f32,
    rlb_b2: f32,

    // Filter states (stereo)
    pre_state_l: BiquadState,
    pre_state_r: BiquadState,
    rlb_state_l: BiquadState,
    rlb_state_r: BiquadState,

    // Mean square accumulators for block processing
    block_ms_sum: f32,
    block_sample_count: usize,
    block_size: usize, // 100ms blocks

    // Ring buffers for windowed loudness
    momentary_buffer: RingBuffer,  // 400ms = 4 blocks
    short_term_buffer: RingBuffer, // 3s = 30 blocks

    // Results
    momentary_lufs: f32,
    short_term_lufs: f32,

    // True peak detection
    true_peak: f32,
    peak_hold_samples: usize,
    peak_hold_counter: usize,
    prev_l: f32,
    prev_r: f32,
}

impl LoudnessMeter {
    /// Create a new loudness meter
    pub fn new(sample_rate: u32) -> Self {
        let block_size = (sample_rate as f32 * 0.1) as usize; // 100ms blocks

        let mut meter = Self {
            sample_rate,
            pre_a0: 1.0,
            pre_a1: 0.0,
            pre_a2: 0.0,
            pre_b1: 0.0,
            pre_b2: 0.0,
            rlb_a0: 1.0,
            rlb_a1: 0.0,
            rlb_a2: 0.0,
            rlb_b1: 0.0,
            rlb_b2: 0.0,
            pre_state_l: BiquadState::default(),
            pre_state_r: BiquadState::default(),
            rlb_state_l: BiquadState::default(),
            rlb_state_r: BiquadState::default(),
            block_ms_sum: 0.0,
            block_sample_count: 0,
            block_size,
            momentary_buffer: RingBuffer::new(4), // 400ms = 4 x 100ms blocks
            short_term_buffer: RingBuffer::new(30), // 3s = 30 x 100ms blocks
            momentary_lufs: -70.0,
            short_term_lufs: -70.0,
            true_peak: -70.0,
            peak_hold_samples: (sample_rate as f32 * 1.0) as usize, // 1s hold
            peak_hold_counter: 0,
            prev_l: 0.0,
            prev_r: 0.0,
        };

        meter.calculate_k_weighting_filters();
        meter
    }

    /// Calculate K-weighting filter coefficients according to ITU-R BS.1770
    fn calculate_k_weighting_filters(&mut self) {
        let fs = self.sample_rate as f32;

        // Stage 1: Pre-filter (high shelf boosting high frequencies)
        // These coefficients are from ITU-R BS.1770-4 for 48kHz
        // For other sample rates, we use bilinear transform approximation

        if (fs - 48000.0).abs() < 1.0 {
            // Use exact coefficients for 48kHz
            self.pre_a0 = 1.53512485958697;
            self.pre_a1 = -2.69169618940638;
            self.pre_a2 = 1.19839281085285;
            self.pre_b1 = -1.69065929318241;
            self.pre_b2 = 0.73248077421585;
        } else {
            // Approximation for other sample rates using biquad high shelf
            let f0 = 1681.974450955533;
            let g = 3.999843853973347; // dB
            let q = 0.7071752369554196;

            let a = 10.0f32.powf(g / 40.0);
            let omega = 2.0 * PI * f0 / fs;
            let sin_omega = omega.sin();
            let cos_omega = omega.cos();
            let alpha = sin_omega / (2.0 * q);

            let a0 = (a + 1.0) - (a - 1.0) * cos_omega + 2.0 * a.sqrt() * alpha;
            self.pre_a0 = (a * ((a + 1.0) + (a - 1.0) * cos_omega + 2.0 * a.sqrt() * alpha)) / a0;
            self.pre_a1 = (-2.0 * a * ((a - 1.0) + (a + 1.0) * cos_omega)) / a0;
            self.pre_a2 = (a * ((a + 1.0) + (a - 1.0) * cos_omega - 2.0 * a.sqrt() * alpha)) / a0;
            self.pre_b1 = (2.0 * ((a - 1.0) - (a + 1.0) * cos_omega)) / a0;
            self.pre_b2 = ((a + 1.0) - (a - 1.0) * cos_omega - 2.0 * a.sqrt() * alpha) / a0;
        }

        // Stage 2: RLB (Revised Low-frequency B-weighting) high-pass filter
        if (fs - 48000.0).abs() < 1.0 {
            // Use exact coefficients for 48kHz
            self.rlb_a0 = 1.0;
            self.rlb_a1 = -2.0;
            self.rlb_a2 = 1.0;
            self.rlb_b1 = -1.99004745483398;
            self.rlb_b2 = 0.99007225036621;
        } else {
            // High-pass at ~38Hz for other sample rates
            let f0 = 38.13547087602444;
            let q = 0.5003270373238773;

            let omega = 2.0 * PI * f0 / fs;
            let sin_omega = omega.sin();
            let cos_omega = omega.cos();
            let alpha = sin_omega / (2.0 * q);

            let a0 = 1.0 + alpha;
            self.rlb_a0 = ((1.0 + cos_omega) / 2.0) / a0;
            self.rlb_a1 = (-(1.0 + cos_omega)) / a0;
            self.rlb_a2 = ((1.0 + cos_omega) / 2.0) / a0;
            self.rlb_b1 = (-2.0 * cos_omega) / a0;
            self.rlb_b2 = (1.0 - alpha) / a0;
        }
    }

    /// Get current LUFS values
    pub fn get_lufs(&self) -> LufsValues {
        LufsValues {
            momentary: self.momentary_lufs,
            short_term: self.short_term_lufs,
            true_peak: self.true_peak,
        }
    }

    /// Convert mean square to LUFS
    #[inline]
    fn ms_to_lufs(ms: f32) -> f32 {
        if ms > 1e-10 {
            -0.691 + 10.0 * ms.log10()
        } else {
            -70.0
        }
    }

    /// Convert linear to dB
    #[inline]
    fn linear_to_db(linear: f32) -> f32 {
        if linear > 1e-10 {
            20.0 * linear.log10()
        } else {
            -70.0
        }
    }

    /// Detect true peak using 4x oversampling approximation
    #[inline]
    fn detect_true_peak(&mut self, left: f32, right: f32) {
        // Simple inter-sample peak estimation
        let peak_l = left.abs();
        let peak_r = right.abs();

        // Estimate inter-sample peaks
        let inter_l = self.estimate_intersample(self.prev_l, left);
        let inter_r = self.estimate_intersample(self.prev_r, right);

        let current_peak = peak_l.max(peak_r).max(inter_l).max(inter_r);
        let current_peak_db = Self::linear_to_db(current_peak);

        if current_peak_db > self.true_peak {
            self.true_peak = current_peak_db;
            self.peak_hold_counter = self.peak_hold_samples;
        } else if self.peak_hold_counter > 0 {
            self.peak_hold_counter -= 1;
        } else {
            // Slow decay
            self.true_peak = self.true_peak * 0.9999 + current_peak_db * 0.0001;
        }

        self.prev_l = left;
        self.prev_r = right;
    }

    /// Estimate inter-sample peak
    #[inline]
    fn estimate_intersample(&self, prev: f32, curr: f32) -> f32 {
        // Simple parabolic interpolation for inter-sample peak estimation
        let avg = (prev + curr) * 0.5;
        // If there's a sign change, peak is approximately the max
        if prev * curr < 0.0 {
            prev.abs().max(curr.abs())
        } else {
            // Estimate overshoot
            avg.abs() * 1.05 // Small margin for safety
        }
    }

    /// Process audio samples (analysis only - does not modify input)
    pub fn process(&mut self, samples: &[f32]) {
        for frame in samples.chunks_exact(2) {
            let left = frame[0];
            let right = frame[1];

            // True peak detection
            self.detect_true_peak(left, right);

            // Apply K-weighting filters
            let weighted_l = {
                let pre = self.pre_state_l.process(
                    left,
                    self.pre_a0,
                    self.pre_a1,
                    self.pre_a2,
                    self.pre_b1,
                    self.pre_b2,
                );
                self.rlb_state_l.process(
                    pre,
                    self.rlb_a0,
                    self.rlb_a1,
                    self.rlb_a2,
                    self.rlb_b1,
                    self.rlb_b2,
                )
            };

            let weighted_r = {
                let pre = self.pre_state_r.process(
                    right,
                    self.pre_a0,
                    self.pre_a1,
                    self.pre_a2,
                    self.pre_b1,
                    self.pre_b2,
                );
                self.rlb_state_r.process(
                    pre,
                    self.rlb_a0,
                    self.rlb_a1,
                    self.rlb_a2,
                    self.rlb_b1,
                    self.rlb_b2,
                )
            };

            // Accumulate mean square (stereo sum with equal weights for L/R)
            self.block_ms_sum += weighted_l * weighted_l + weighted_r * weighted_r;
            self.block_sample_count += 1;

            // When we have a complete 100ms block, update the ring buffers
            if self.block_sample_count >= self.block_size {
                let block_ms = self.block_ms_sum / (self.block_sample_count as f32 * 2.0);

                self.momentary_buffer.push(block_ms);
                self.short_term_buffer.push(block_ms);

                // Calculate momentary (400ms)
                if self.momentary_buffer.len() > 0 {
                    let sum = self.momentary_buffer.sum();
                    let avg_ms = sum / self.momentary_buffer.len() as f32;
                    self.momentary_lufs = Self::ms_to_lufs(avg_ms);
                }

                // Calculate short-term (3s)
                if self.short_term_buffer.len() > 0 {
                    let sum = self.short_term_buffer.sum();
                    let avg_ms = sum / self.short_term_buffer.len() as f32;
                    self.short_term_lufs = Self::ms_to_lufs(avg_ms);
                }

                // Reset block accumulator
                self.block_ms_sum = 0.0;
                self.block_sample_count = 0;
            }
        }
    }

    /// Reset the meter state
    pub fn reset(&mut self) {
        self.pre_state_l.reset();
        self.pre_state_r.reset();
        self.rlb_state_l.reset();
        self.rlb_state_r.reset();
        self.block_ms_sum = 0.0;
        self.block_sample_count = 0;
        self.momentary_buffer.clear();
        self.short_term_buffer.clear();
        self.momentary_lufs = -70.0;
        self.short_term_lufs = -70.0;
        self.true_peak = -70.0;
        self.peak_hold_counter = 0;
        self.prev_l = 0.0;
        self.prev_r = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meter_creation() {
        let meter = LoudnessMeter::new(48000);
        let lufs = meter.get_lufs();
        assert!(lufs.momentary < -60.0);
        assert!(lufs.short_term < -60.0);
    }

    #[test]
    fn test_silence_low_lufs() {
        let mut meter = LoudnessMeter::new(48000);

        // Process silence
        let samples = vec![0.0; 48000]; // 1 second
        meter.process(&samples);

        let lufs = meter.get_lufs();
        assert!(
            lufs.momentary < -60.0,
            "Momentary should be very low for silence, got {}",
            lufs.momentary
        );
    }

    #[test]
    fn test_loud_signal_higher_lufs() {
        let mut meter = LoudnessMeter::new(48000);

        // Process a loud sine wave
        let samples: Vec<f32> = (0..48000)
            .flat_map(|i| {
                let val = (i as f32 * 2.0 * PI * 1000.0 / 48000.0).sin() * 0.5;
                vec![val, val]
            })
            .collect();

        meter.process(&samples);

        let lufs = meter.get_lufs();
        // A 0.5 amplitude signal should be around -10 to -15 LUFS
        assert!(
            lufs.momentary > -20.0,
            "Momentary should be reasonable for 0.5 amplitude, got {}",
            lufs.momentary
        );
        assert!(
            lufs.momentary < 0.0,
            "Momentary should be negative LUFS, got {}",
            lufs.momentary
        );
    }

    #[test]
    fn test_true_peak_detection() {
        let mut meter = LoudnessMeter::new(48000);

        // Signal with peak at 0.9
        let samples: Vec<f32> = (0..4800)
            .flat_map(|i| {
                let val = (i as f32 * 2.0 * PI * 1000.0 / 48000.0).sin() * 0.9;
                vec![val, val]
            })
            .collect();

        meter.process(&samples);

        let lufs = meter.get_lufs();
        // True peak should be around -0.9 dBFS (0.9 linear ≈ -0.92 dB)
        assert!(
            lufs.true_peak > -2.0,
            "True peak should detect 0.9 amplitude, got {} dBFS",
            lufs.true_peak
        );
    }

    #[test]
    fn test_reset() {
        let mut meter = LoudnessMeter::new(48000);

        // Process some audio
        let samples: Vec<f32> = vec![0.5, 0.5, 0.3, 0.3, 0.1, 0.1];
        meter.process(&samples);

        // Reset
        meter.reset();

        let lufs = meter.get_lufs();
        assert!(lufs.momentary < -60.0);
        assert!(lufs.short_term < -60.0);
    }
}
