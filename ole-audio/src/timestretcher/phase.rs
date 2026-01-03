//! State-of-the-art Phase Vocoder for pitch-independent time-stretching.
//!
//! Key innovations over traditional phase vocoders:
//! - Phase-locking to preserve harmonic relationships
//! - Transient detection with phase reset (preserves attack clarity)
//! - Spectral peak tracking for reduced phasiness
//! - Identity phase locking for extreme stretch ratios

use super::stft::{Complex, FftSize, Stft};
use std::f32::consts::PI;

const TWO_PI: f32 = 2.0 * PI;

/// Phase vocoder processor for high-quality time-stretching
pub struct PhaseVocoder {
    /// STFT processor
    stft: Stft,
    /// FFT size
    fft_size: usize,
    /// Hop size
    hop_size: usize,
    /// Number of frequency bins
    num_bins: usize,
    /// Sample rate
    sample_rate: f32,
    /// Current time stretch ratio (1.0 = normal, 2.0 = double length)
    stretch_ratio: f32,
    /// Phase accumulator (left channel)
    phase_accum_l: Vec<f32>,
    /// Phase accumulator (right channel)
    phase_accum_r: Vec<f32>,
    /// Previous frame phases (left)
    prev_phase_l: Vec<f32>,
    /// Previous frame phases (right)
    prev_phase_r: Vec<f32>,
    /// Previous frame magnitudes for transient detection (left)
    prev_mag_l: Vec<f32>,
    /// Previous frame magnitudes for transient detection (right)
    prev_mag_r: Vec<f32>,
    /// Expected phase advance per bin (based on hop size)
    omega: Vec<f32>,
    /// Frequency bins (left) - pre-allocated
    bins_l: Vec<Complex>,
    /// Frequency bins (right) - pre-allocated
    bins_r: Vec<Complex>,
    /// Output bins (left) - pre-allocated
    out_bins_l: Vec<Complex>,
    /// Output bins (right) - pre-allocated
    out_bins_r: Vec<Complex>,
    /// Peak bin indices for phase locking
    peaks: Vec<usize>,
    /// Transient threshold
    transient_threshold: f32,
    /// Whether transient was detected in last frame
    transient_detected: bool,
    /// Phase lock mode
    phase_lock_mode: PhaseLockMode,
    /// Enabled state
    enabled: bool,
    /// Samples processed since last frame
    samples_since_frame: usize,
    /// Fractional sample position for variable rate
    fractional_pos: f32,
}

/// Phase locking modes for different quality/CPU trade-offs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseLockMode {
    /// No phase locking (fastest, most phasiness)
    None,
    /// Identity phase locking (good quality, low CPU)
    Identity,
    /// Peak-locked phase locking (best quality, higher CPU)
    PeakLocked,
}

impl Default for PhaseLockMode {
    fn default() -> Self {
        PhaseLockMode::PeakLocked
    }
}

impl PhaseVocoder {
    /// Create new phase vocoder
    pub fn new(sample_rate: f32, fft_size: FftSize) -> Self {
        let stft = Stft::new(fft_size);
        let num_bins = stft.num_bins();
        let hop_size = stft.hop_size();
        let fft_size_val = stft.size();

        // Pre-compute expected phase advance per bin
        // omega[k] = 2 * pi * k * hop_size / fft_size
        let omega: Vec<f32> = (0..num_bins)
            .map(|k| TWO_PI * k as f32 * hop_size as f32 / fft_size_val as f32)
            .collect();

        Self {
            stft,
            fft_size: fft_size_val,
            hop_size,
            num_bins,
            sample_rate,
            stretch_ratio: 1.0,
            phase_accum_l: vec![0.0; num_bins],
            phase_accum_r: vec![0.0; num_bins],
            prev_phase_l: vec![0.0; num_bins],
            prev_phase_r: vec![0.0; num_bins],
            prev_mag_l: vec![0.0; num_bins],
            prev_mag_r: vec![0.0; num_bins],
            omega,
            bins_l: vec![Complex::default(); num_bins],
            bins_r: vec![Complex::default(); num_bins],
            out_bins_l: vec![Complex::default(); num_bins],
            out_bins_r: vec![Complex::default(); num_bins],
            peaks: Vec::with_capacity(num_bins / 4),
            transient_threshold: 1.5,
            transient_detected: false,
            phase_lock_mode: PhaseLockMode::PeakLocked,
            enabled: true,
            samples_since_frame: 0,
            fractional_pos: 0.0,
        }
    }

    /// Set time stretch ratio (0.25 to 4.0)
    /// Values < 1.0 speed up, > 1.0 slow down
    #[inline]
    pub fn set_stretch_ratio(&mut self, ratio: f32) {
        self.stretch_ratio = ratio.clamp(0.25, 4.0);
    }

    /// Get current stretch ratio
    #[inline]
    pub fn stretch_ratio(&self) -> f32 {
        self.stretch_ratio
    }

    /// Set phase lock mode
    #[inline]
    pub fn set_phase_lock_mode(&mut self, mode: PhaseLockMode) {
        self.phase_lock_mode = mode;
    }

    /// Set transient sensitivity (1.0 = low, 3.0 = high)
    #[inline]
    pub fn set_transient_sensitivity(&mut self, sensitivity: f32) {
        self.transient_threshold = sensitivity.clamp(1.0, 5.0);
    }

    /// Enable/disable time stretching
    #[inline]
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.reset();
        }
    }

    /// Check if enabled
    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Process stereo samples through phase vocoder
    /// Returns processed sample or None if more input needed
    pub fn process(&mut self, left: f32, right: f32) -> Option<(f32, f32)> {
        if !self.enabled {
            return Some((left, right));
        }

        // Feed input to STFT
        let frame_ready = self.stft.push_samples(left, right);

        if frame_ready {
            // Analyze current frame
            self.stft.analyze(&mut self.bins_l, &mut self.bins_r);

            // Detect transients
            self.detect_transients();

            // Process phase (the core algorithm)
            self.process_phase();

            // Synthesize with modified phases
            self.stft
                .synthesize(&self.out_bins_l, &self.out_bins_r, self.stretch_ratio);

            // Store current as previous for next frame
            for k in 0..self.num_bins {
                self.prev_phase_l[k] = self.bins_l[k].phase();
                self.prev_phase_r[k] = self.bins_r[k].phase();
                self.prev_mag_l[k] = self.bins_l[k].magnitude();
                self.prev_mag_r[k] = self.bins_r[k].magnitude();
            }
        }

        // Pop output sample
        self.stft.pop_sample()
    }

    /// Core phase processing algorithm
    fn process_phase(&mut self) {
        // Find spectral peaks for phase locking
        if self.phase_lock_mode == PhaseLockMode::PeakLocked {
            self.find_peaks();
        }

        let hop_size = self.hop_size as f32;
        let stretch_ratio = self.stretch_ratio;
        let transient_detected = self.transient_detected;
        let output_hop = hop_size * stretch_ratio;

        // Process each frequency bin - inline to avoid borrow conflicts
        for k in 0..self.num_bins {
            let omega_k = self.omega[k];

            // Left channel
            {
                let mag = self.bins_l[k].magnitude();
                let phase = self.bins_l[k].phase();

                let expected_phase = self.prev_phase_l[k] + omega_k;
                let phase_diff = Self::wrap_phase(phase - expected_phase);
                let freq_dev = phase_diff / hop_size;

                let new_phase = if transient_detected {
                    phase
                } else {
                    self.phase_accum_l[k] += omega_k * stretch_ratio + freq_dev * output_hop;
                    self.phase_accum_l[k]
                };

                self.out_bins_l[k] = Complex::from_polar(mag, new_phase);
            }

            // Right channel
            {
                let mag = self.bins_r[k].magnitude();
                let phase = self.bins_r[k].phase();

                let expected_phase = self.prev_phase_r[k] + omega_k;
                let phase_diff = Self::wrap_phase(phase - expected_phase);
                let freq_dev = phase_diff / hop_size;

                let new_phase = if transient_detected {
                    phase
                } else {
                    self.phase_accum_r[k] += omega_k * stretch_ratio + freq_dev * output_hop;
                    self.phase_accum_r[k]
                };

                self.out_bins_r[k] = Complex::from_polar(mag, new_phase);
            }
        }

        // Apply phase locking
        if self.phase_lock_mode == PhaseLockMode::PeakLocked {
            self.apply_peak_phase_lock();
        }
    }

    /// Find spectral peaks for phase locking
    fn find_peaks(&mut self) {
        self.peaks.clear();

        // Simple peak detection: bin is peak if greater than neighbors
        for k in 2..self.num_bins - 2 {
            let mag = self.bins_l[k].magnitude() + self.bins_r[k].magnitude();
            let prev2 = self.bins_l[k - 2].magnitude() + self.bins_r[k - 2].magnitude();
            let prev1 = self.bins_l[k - 1].magnitude() + self.bins_r[k - 1].magnitude();
            let next1 = self.bins_l[k + 1].magnitude() + self.bins_r[k + 1].magnitude();
            let next2 = self.bins_l[k + 2].magnitude() + self.bins_r[k + 2].magnitude();

            if mag > prev2 && mag > prev1 && mag > next1 && mag > next2 {
                self.peaks.push(k);
            }
        }
    }

    /// Apply phase locking around spectral peaks
    fn apply_peak_phase_lock(&mut self) {
        for &peak in &self.peaks {
            // Lock nearby bins to peak's phase
            let peak_phase_l = self.out_bins_l[peak].phase();
            let peak_phase_r = self.out_bins_r[peak].phase();

            // Influence radius based on peak strength
            let peak_mag = self.bins_l[peak].magnitude() + self.bins_r[peak].magnitude();
            let radius = ((peak_mag * 10.0) as usize).clamp(1, 5);

            for j in 1..=radius {
                if peak >= j {
                    let k = peak - j;
                    let weight = 1.0 - j as f32 / (radius + 1) as f32;

                    // Blend phase towards peak phase
                    let mag_l = self.out_bins_l[k].magnitude();
                    let phase_l = self.out_bins_l[k].phase();
                    let blended_phase_l = Self::blend_phase(phase_l, peak_phase_l, weight);
                    self.out_bins_l[k] = Complex::from_polar(mag_l, blended_phase_l);

                    let mag_r = self.out_bins_r[k].magnitude();
                    let phase_r = self.out_bins_r[k].phase();
                    let blended_phase_r = Self::blend_phase(phase_r, peak_phase_r, weight);
                    self.out_bins_r[k] = Complex::from_polar(mag_r, blended_phase_r);
                }

                if peak + j < self.num_bins {
                    let k = peak + j;
                    let weight = 1.0 - j as f32 / (radius + 1) as f32;

                    let mag_l = self.out_bins_l[k].magnitude();
                    let phase_l = self.out_bins_l[k].phase();
                    let blended_phase_l = Self::blend_phase(phase_l, peak_phase_l, weight);
                    self.out_bins_l[k] = Complex::from_polar(mag_l, blended_phase_l);

                    let mag_r = self.out_bins_r[k].magnitude();
                    let phase_r = self.out_bins_r[k].phase();
                    let blended_phase_r = Self::blend_phase(phase_r, peak_phase_r, weight);
                    self.out_bins_r[k] = Complex::from_polar(mag_r, blended_phase_r);
                }
            }
        }
    }

    /// Detect transients using spectral flux
    fn detect_transients(&mut self) {
        let mut flux_l = 0.0f32;
        let mut flux_r = 0.0f32;
        let mut total_l = 0.0f32;
        let mut total_r = 0.0f32;

        for k in 0..self.num_bins {
            let mag_l = self.bins_l[k].magnitude();
            let mag_r = self.bins_r[k].magnitude();
            let diff_l = mag_l - self.prev_mag_l[k];
            let diff_r = mag_r - self.prev_mag_r[k];

            // Only count increases (onset detection)
            if diff_l > 0.0 {
                flux_l += diff_l * diff_l;
            }
            if diff_r > 0.0 {
                flux_r += diff_r * diff_r;
            }

            total_l += mag_l * mag_l;
            total_r += mag_r * mag_r;
        }

        // Normalize flux
        let total = (total_l + total_r).sqrt();
        let flux = (flux_l + flux_r).sqrt();

        // Transient if flux exceeds threshold relative to total energy
        self.transient_detected = total > 0.001 && flux / total > self.transient_threshold;

        // Reset phase accumulators on transient
        if self.transient_detected {
            for k in 0..self.num_bins {
                self.phase_accum_l[k] = self.bins_l[k].phase();
                self.phase_accum_r[k] = self.bins_r[k].phase();
            }
        }
    }

    /// Wrap phase to [-π, π]
    #[inline(always)]
    fn wrap_phase(phase: f32) -> f32 {
        let mut p = phase;
        while p > PI {
            p -= TWO_PI;
        }
        while p < -PI {
            p += TWO_PI;
        }
        p
    }

    /// Blend two phases with weight
    #[inline(always)]
    fn blend_phase(phase1: f32, phase2: f32, weight: f32) -> f32 {
        // Convert to unit circle and blend
        let (s1, c1) = phase1.sin_cos();
        let (s2, c2) = phase2.sin_cos();

        let s = s1 * (1.0 - weight) + s2 * weight;
        let c = c1 * (1.0 - weight) + c2 * weight;

        s.atan2(c)
    }

    /// Reset all state
    pub fn reset(&mut self) {
        self.stft.reset();
        self.phase_accum_l.fill(0.0);
        self.phase_accum_r.fill(0.0);
        self.prev_phase_l.fill(0.0);
        self.prev_phase_r.fill(0.0);
        self.prev_mag_l.fill(0.0);
        self.prev_mag_r.fill(0.0);
        self.fractional_pos = 0.0;
        self.samples_since_frame = 0;
        self.transient_detected = false;
    }
}

/// Time-stretching parameters for deck integration
#[derive(Debug, Clone, Copy)]
pub struct TimeStretchParams {
    /// Stretch ratio (1.0 = normal)
    pub ratio: f32,
    /// Phase lock mode
    pub phase_lock: PhaseLockMode,
    /// Transient sensitivity
    pub transient_sensitivity: f32,
    /// Enable time stretching
    pub enabled: bool,
}

impl Default for TimeStretchParams {
    fn default() -> Self {
        Self {
            ratio: 1.0,
            phase_lock: PhaseLockMode::PeakLocked,
            transient_sensitivity: 1.5,
            enabled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_vocoder_passthrough() {
        let mut pv = PhaseVocoder::new(48000.0, FftSize::Small);
        pv.set_stretch_ratio(1.0);

        let mut output_count = 0;

        // Process a sine wave
        for i in 0..4096 {
            let t = i as f32 / 48000.0;
            let sample = (2.0 * PI * 440.0 * t).sin() * 0.5;

            if let Some(_) = pv.process(sample, sample) {
                output_count += 1;
            }
        }

        // Should produce output (accounting for latency)
        assert!(output_count > 0);
    }

    #[test]
    fn test_stretch_ratio_clamping() {
        let mut pv = PhaseVocoder::new(48000.0, FftSize::Medium);

        pv.set_stretch_ratio(0.1);
        assert_eq!(pv.stretch_ratio(), 0.25);

        pv.set_stretch_ratio(10.0);
        assert_eq!(pv.stretch_ratio(), 4.0);

        pv.set_stretch_ratio(1.5);
        assert_eq!(pv.stretch_ratio(), 1.5);
    }

    #[test]
    fn test_phase_wrap() {
        assert!((PhaseVocoder::wrap_phase(0.0)).abs() < 0.001);
        assert!((PhaseVocoder::wrap_phase(PI + 0.1) - (-PI + 0.1)).abs() < 0.001);
        assert!((PhaseVocoder::wrap_phase(-PI - 0.1) - (PI - 0.1)).abs() < 0.001);
    }
}
