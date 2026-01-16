//! Beat grid analysis for professional DJ beat synchronization
//!
//! Provides accurate BPM detection via spectral flux onset detection
//! and beat grid generation for phase-aligned beatmatching.

use rustfft::{num_complex::Complex, FftPlanner};
use std::f32::consts::PI;
use std::sync::Arc;

/// Represents a beat grid for a track
#[derive(Debug, Clone)]
pub struct BeatGrid {
    /// BPM of the track
    pub bpm: f32,
    /// Offset of the first beat in samples (stereo interleaved)
    pub first_beat_offset: u64,
    /// Sample rate used for calculations
    pub sample_rate: u32,
    /// Number of samples per beat (cached for performance)
    samples_per_beat: f64,
    /// Confidence score (0.0 - 1.0) indicating detection reliability
    pub confidence: f32,
}

impl BeatGrid {
    /// Create a new beat grid
    pub fn new(bpm: f32, first_beat_offset: u64, sample_rate: u32, confidence: f32) -> Self {
        // Samples per beat for stereo interleaved audio
        let samples_per_beat = (60.0 / bpm as f64) * sample_rate as f64 * 2.0;
        Self {
            bpm,
            first_beat_offset,
            sample_rate,
            samples_per_beat,
            confidence,
        }
    }

    /// Get the beat number (can be fractional) at a given sample position
    pub fn beat_at_position(&self, position: f64) -> f64 {
        (position - self.first_beat_offset as f64) / self.samples_per_beat
    }

    /// Get the phase (0.0 - 1.0) within the current beat at a given position
    pub fn phase_at_position(&self, position: f64) -> f32 {
        let beat = self.beat_at_position(position);
        beat.fract().abs() as f32
    }

    /// Get sample position for a specific beat number
    pub fn position_for_beat(&self, beat: f64) -> f64 {
        self.first_beat_offset as f64 + (beat * self.samples_per_beat)
    }

    /// Get samples per beat adjusted for a tempo multiplier
    pub fn samples_per_beat_at_tempo(&self, tempo: f32) -> f64 {
        self.samples_per_beat / tempo as f64
    }

    /// Get the base samples per beat (at tempo 1.0)
    pub fn samples_per_beat(&self) -> f64 {
        self.samples_per_beat
    }
}

/// Analyzer that builds a beat grid from audio samples using spectral flux onset detection
pub struct BeatGridAnalyzer {
    sample_rate: u32,
    hop_size: usize,
    fft_size: usize,
    fft: Arc<dyn rustfft::Fft<f32>>,
    window: Vec<f32>,
}

impl BeatGridAnalyzer {
    /// Create a new beat grid analyzer
    pub fn new(sample_rate: u32) -> Self {
        let fft_size = 2048;
        let hop_size = 512; // ~11.6ms at 44.1kHz - good for transient detection
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(fft_size);

        // Pre-compute Hann window
        let window: Vec<f32> = (0..fft_size)
            .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / fft_size as f32).cos()))
            .collect();

        Self {
            sample_rate,
            hop_size,
            fft_size,
            fft,
            window,
        }
    }

    /// Analyze audio samples and build a beat grid
    ///
    /// This should be called on track load with the first 30 seconds or so of audio.
    /// Returns None if beat detection fails.
    pub fn analyze(&self, samples: &[f32]) -> Option<BeatGrid> {
        if samples.len() < self.sample_rate as usize * 4 {
            // Need at least 4 seconds of audio
            return None;
        }

        // 1. Compute spectral flux onset detection function
        let onset_function = self.compute_onset_function(samples);

        if onset_function.len() < 100 {
            return None;
        }

        // 2. Find onset peaks
        let onsets = self.find_onset_peaks(&onset_function);

        // 3. Estimate BPM using autocorrelation
        let (bpm, confidence) = self.estimate_bpm_autocorrelation(&onset_function)?;

        // 4. Find first downbeat
        let first_beat = self.find_first_downbeat(&onsets, bpm);

        Some(BeatGrid::new(bpm, first_beat, self.sample_rate, confidence))
    }

    /// Compute spectral flux onset detection function
    ///
    /// Spectral flux measures the change in magnitude spectrum between consecutive frames.
    /// Transients (kicks, snares) cause large positive flux values.
    fn compute_onset_function(&self, samples: &[f32]) -> Vec<f32> {
        // Convert stereo to mono
        let mono: Vec<f32> = samples
            .chunks(2)
            .map(|s| (s[0] + s.get(1).unwrap_or(&0.0)) * 0.5)
            .collect();

        let mut onset_fn = Vec::new();
        let mut prev_spectrum: Option<Vec<f32>> = None;

        let mut frame_start = 0;
        while frame_start + self.fft_size <= mono.len() {
            let frame = &mono[frame_start..frame_start + self.fft_size];

            // Apply window and compute FFT
            let mut buffer: Vec<Complex<f32>> = frame
                .iter()
                .zip(&self.window)
                .map(|(s, w)| Complex::new(s * w, 0.0))
                .collect();

            self.fft.process(&mut buffer);

            // Get magnitude spectrum (only positive frequencies)
            let spectrum: Vec<f32> = buffer[..self.fft_size / 2]
                .iter()
                .map(|c| c.norm())
                .collect();

            // Compute spectral flux (half-wave rectified difference)
            // Only count increases in magnitude - decreases don't indicate onsets
            if let Some(ref prev) = prev_spectrum {
                let flux: f32 = spectrum
                    .iter()
                    .zip(prev.iter())
                    .map(|(curr, prev)| (curr - prev).max(0.0))
                    .sum();
                onset_fn.push(flux);
            }

            prev_spectrum = Some(spectrum);
            frame_start += self.hop_size;
        }

        // Normalize onset function
        let max = onset_fn.iter().cloned().fold(0.0f32, f32::max);
        if max > 0.0 {
            for v in &mut onset_fn {
                *v /= max;
            }
        }

        onset_fn
    }

    /// Find peaks in the onset detection function
    fn find_onset_peaks(&self, onset_fn: &[f32]) -> Vec<usize> {
        // Adaptive threshold: mean + 0.5 * std_dev
        let mean: f32 = onset_fn.iter().sum::<f32>() / onset_fn.len() as f32;
        let variance: f32 =
            onset_fn.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / onset_fn.len() as f32;
        let std_dev = variance.sqrt();
        let threshold = (mean + 0.5 * std_dev).max(0.1);

        // Minimum distance between peaks: 50ms
        let min_distance = (self.sample_rate as f32 * 0.05) as usize / self.hop_size;
        let min_distance = min_distance.max(1);

        let mut peaks = Vec::new();
        let mut last_peak: isize = -(min_distance as isize);

        for i in 1..onset_fn.len().saturating_sub(1) {
            if onset_fn[i] > threshold
                && onset_fn[i] > onset_fn[i - 1]
                && onset_fn[i] >= onset_fn[i + 1]
                && (i as isize - last_peak) >= min_distance as isize
            {
                peaks.push(i);
                last_peak = i as isize;
            }
        }

        peaks
    }

    /// Compute correlation at a specific lag
    fn correlation_at_lag(&self, onset_fn: &[f32], lag: usize) -> f32 {
        if lag == 0 || lag >= onset_fn.len() / 2 {
            return 0.0;
        }

        let mut correlation: f32 = 0.0;
        let mut norm_a: f32 = 0.0;
        let mut norm_b: f32 = 0.0;

        for i in 0..(onset_fn.len() - lag) {
            correlation += onset_fn[i] * onset_fn[i + lag];
            norm_a += onset_fn[i] * onset_fn[i];
            norm_b += onset_fn[i + lag] * onset_fn[i + lag];
        }

        let norm = (norm_a * norm_b).sqrt();
        if norm > 0.0 {
            correlation / norm
        } else {
            0.0
        }
    }

    /// Estimate BPM using autocorrelation of the onset function
    ///
    /// Autocorrelation finds periodic patterns by correlating the signal with delayed versions of itself.
    /// The lag with highest correlation corresponds to the beat period.
    fn estimate_bpm_autocorrelation(&self, onset_fn: &[f32]) -> Option<(f32, f32)> {
        if onset_fn.len() < 500 {
            return None;
        }

        // BPM range: 60-200 BPM
        // Convert to lag range in onset function frames
        let frames_per_second = self.sample_rate as f32 / self.hop_size as f32;
        let min_lag = (frames_per_second * 60.0 / 200.0) as usize; // 200 BPM
        let max_lag = (frames_per_second * 60.0 / 60.0) as usize; // 60 BPM

        // Use first ~10 seconds for analysis
        let analysis_len = onset_fn.len().min(max_lag * 8);
        let analysis = &onset_fn[..analysis_len];

        let mut best_lag = min_lag;
        let mut best_correlation = 0.0f32;

        for lag in min_lag..max_lag.min(analysis_len / 2) {
            let correlation = self.correlation_at_lag(analysis, lag);

            if correlation > best_correlation {
                best_correlation = correlation;
                best_lag = lag;
            }
        }

        // Convert lag to BPM
        let seconds_per_beat = best_lag as f32 / frames_per_second;
        if seconds_per_beat <= 0.0 {
            return None;
        }

        let raw_bpm = 60.0 / seconds_per_beat;

        // Octave disambiguation: check half and double BPM correlations
        let final_bpm = self.disambiguate_octave(analysis, raw_bpm, frames_per_second);

        // Confidence is based on correlation strength
        let confidence = best_correlation.clamp(0.0, 1.0);

        Some((final_bpm, confidence))
    }

    /// Disambiguate between octave-related BPM values (e.g., 77 vs 154)
    ///
    /// When we detect a BPM in the ambiguous range (65-95 BPM), we check if
    /// double the BPM also has strong correlation. For dance/electronic music,
    /// the higher tempo is usually correct.
    fn disambiguate_octave(&self, onset_fn: &[f32], raw_bpm: f32, frames_per_second: f32) -> f32 {
        // If BPM is very low, definitely double it
        if raw_bpm < 65.0 {
            return raw_bpm * 2.0;
        }

        // If BPM is very high, definitely halve it
        if raw_bpm > 185.0 {
            return raw_bpm / 2.0;
        }

        // For BPM in the ambiguous range (65-95), check if doubled BPM is better
        if (65.0..=95.0).contains(&raw_bpm) {
            let doubled_bpm = raw_bpm * 2.0;

            // Calculate lag for the original and doubled BPM
            let original_lag = (frames_per_second * 60.0 / raw_bpm) as usize;
            let doubled_lag = (frames_per_second * 60.0 / doubled_bpm) as usize;

            let original_corr = self.correlation_at_lag(onset_fn, original_lag);
            let doubled_corr = self.correlation_at_lag(onset_fn, doubled_lag);

            // For dance music, prefer the doubled BPM if:
            // 1. The doubled correlation is at least 70% as strong as original
            // 2. The doubled BPM falls in the typical DJ range (120-180)
            let doubled_is_reasonable = (120.0..=180.0).contains(&doubled_bpm);
            let correlation_ratio = doubled_corr / original_corr.max(0.001);

            if doubled_is_reasonable && correlation_ratio > 0.7 {
                return doubled_bpm;
            }
        }

        // For BPM in high-but-maybe-double range (170-185), check if halving is better
        if (170.0..=185.0).contains(&raw_bpm) {
            let halved_bpm = raw_bpm / 2.0;

            let original_lag = (frames_per_second * 60.0 / raw_bpm) as usize;
            let halved_lag = (frames_per_second * 60.0 / halved_bpm) as usize;

            let original_corr = self.correlation_at_lag(onset_fn, original_lag);
            let halved_corr = self.correlation_at_lag(onset_fn, halved_lag);

            // Only halve if the halved correlation is significantly stronger
            // This prevents accidentally halving legitimate 170+ BPM tracks
            if halved_corr > original_corr * 1.2 {
                return halved_bpm;
            }
        }

        raw_bpm
    }

    /// Find the first downbeat by aligning a beat grid to the strongest onsets
    fn find_first_downbeat(&self, onsets: &[usize], bpm: f32) -> u64 {
        if onsets.is_empty() {
            return 0;
        }

        // Beat interval in onset function frames
        let frames_per_second = self.sample_rate as f32 / self.hop_size as f32;
        let beat_interval_frames = (frames_per_second * 60.0 / bpm) as usize;

        if beat_interval_frames == 0 {
            return 0;
        }

        // Score each of the first N onsets as potential first beat
        // by counting how many subsequent beats align with detected onsets
        let candidates = onsets.len().min(32);
        let mut best_onset = onsets[0];
        let mut best_score = 0.0f32;

        for &onset in &onsets[..candidates] {
            let mut score = 0.0;

            // Check alignment for the next 16 expected beats
            for beat_num in 0..16 {
                let expected = onset + beat_num * beat_interval_frames;
                let tolerance = beat_interval_frames / 6; // ~16% tolerance

                // Find closest actual onset
                for &actual in onsets {
                    if actual.abs_diff(expected) <= tolerance {
                        // Weight earlier beats more heavily
                        score += 1.0 / (beat_num as f32 + 1.0);
                        break;
                    }
                }
            }

            if score > best_score {
                best_score = score;
                best_onset = onset;
            }
        }

        // Convert from onset function frame index to sample position (stereo)
        best_onset as u64 * self.hop_size as u64 * 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beat_grid_phase() {
        let grid = BeatGrid::new(120.0, 0, 44100, 1.0);

        // At position 0, phase should be 0
        assert!((grid.phase_at_position(0.0) - 0.0).abs() < 0.01);

        // At half a beat, phase should be 0.5
        let half_beat = grid.samples_per_beat() / 2.0;
        assert!((grid.phase_at_position(half_beat) - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_beat_grid_with_offset() {
        let offset = 44100; // 0.5 seconds at 44.1kHz stereo
        let grid = BeatGrid::new(120.0, offset, 44100, 1.0);

        // Before offset, beat number should be negative
        assert!(grid.beat_at_position(0.0) < 0.0);

        // At offset, should be beat 0
        assert!((grid.beat_at_position(offset as f64) - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_tempo_adjustment() {
        let grid = BeatGrid::new(120.0, 0, 44100, 1.0);

        let normal = grid.samples_per_beat_at_tempo(1.0);
        let faster = grid.samples_per_beat_at_tempo(2.0);

        // At 2x tempo, samples per beat should be half
        assert!((faster - normal / 2.0).abs() < 1.0);
    }
}
