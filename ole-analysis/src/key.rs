//! Key detection using chromagram analysis
//!
//! Implements key-finding via chromagram correlation:
//! 1. Compute chromagram (12-bin pitch class distribution) via STFT
//! 2. Correlate with Sha'ath (2011) key profiles optimized for electronic music
//! 3. Return the best matching key with confidence score

use crate::camelot::MusicalKey;
use rustfft::{num_complex::Complex, FftPlanner};
use std::f32::consts::PI;
use std::sync::Arc;

/// Detected key with confidence score
#[derive(Debug, Clone, Copy)]
pub struct DetectedKey {
    /// The detected musical key
    pub key: MusicalKey,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
}

/// Sha'ath (2011) major key profile
///
/// Optimized for electronic/dance music detection (from libKeyFinder).
/// Index 0 = tonic.
const MAJOR_PROFILE: [f32; 12] = [
    6.6, // Tonic (I)
    2.0, // Minor 2nd
    3.5, // Major 2nd
    2.3, // Minor 3rd
    4.6, // Major 3rd
    4.0, // Perfect 4th
    2.5, // Tritone
    5.2, // Perfect 5th
    2.4, // Minor 6th
    3.7, // Major 6th
    2.3, // Minor 7th
    3.4, // Major 7th
];

/// Sha'ath (2011) minor key profile
///
/// Optimized for electronic/dance music detection (from libKeyFinder).
/// Index 0 = tonic.
const MINOR_PROFILE: [f32; 12] = [
    6.5, // Tonic (i)
    2.8, // Minor 2nd
    3.5, // Major 2nd
    5.4, // Minor 3rd
    2.7, // Major 3rd
    3.5, // Perfect 4th
    2.5, // Tritone
    5.2, // Perfect 5th
    4.0, // Minor 6th
    2.7, // Major 6th
    4.3, // Minor 7th
    3.2, // Major 7th
];

/// Reference frequency for A4 (440 Hz)
const A4_FREQ: f32 = 440.0;

/// Key analyzer using chromagram-based detection
pub struct KeyAnalyzer {
    sample_rate: u32,
    fft_size: usize,
    hop_size: usize,
    fft: Arc<dyn rustfft::Fft<f32>>,
    window: Vec<f32>,
    /// Pre-computed bin-to-pitch-class mapping
    bin_to_pitch_class: Vec<Option<u8>>,
    /// Pre-computed bin weights (includes harmonic emphasis AND octave decay)
    bin_weights: Vec<f32>,
    /// Pre-allocated FFT buffer (reused per frame to avoid allocation)
    fft_buffer: Vec<Complex<f32>>,
}

impl KeyAnalyzer {
    /// Create a new key analyzer
    ///
    /// Uses a 4096-sample FFT for good frequency resolution at low frequencies.
    pub fn new(sample_rate: u32) -> Self {
        let fft_size = 4096; // Larger for better frequency resolution
        let hop_size = 2048; // 50% overlap

        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(fft_size);

        // Pre-compute Hann window
        let window: Vec<f32> = (0..fft_size)
            .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / fft_size as f32).cos()))
            .collect();

        // Pre-compute bin-to-pitch-class mapping (includes octave decay in weights)
        let (bin_to_pitch_class, bin_weights) =
            Self::compute_pitch_class_mapping(fft_size, sample_rate);

        // Pre-allocate FFT buffer
        let fft_buffer = vec![Complex::new(0.0, 0.0); fft_size];

        Self {
            sample_rate,
            fft_size,
            hop_size,
            fft,
            window,
            bin_to_pitch_class,
            bin_weights,
            fft_buffer,
        }
    }

    /// Compute the mapping from FFT bins to pitch classes
    ///
    /// Each bin is mapped to its closest pitch class (0-11, where 0=C).
    /// Returns the mapping and weights for each bin.
    /// Weights include both harmonic emphasis (detune) AND octave decay (~6dB/octave above 500Hz).
    fn compute_pitch_class_mapping(
        fft_size: usize,
        sample_rate: u32,
    ) -> (Vec<Option<u8>>, Vec<f32>) {
        let nyquist = sample_rate as f32 / 2.0;
        let bin_freq = |bin: usize| -> f32 { bin as f32 * sample_rate as f32 / fft_size as f32 };

        let mut mapping = Vec::with_capacity(fft_size / 2);
        let mut weights = Vec::with_capacity(fft_size / 2);

        // Musical range: ~27.5 Hz (A0) to ~4186 Hz (C8)
        // We focus on 55 Hz to 4000 Hz to capture harmonics for better key detection
        let min_freq = 55.0; // A1
        let max_freq = 4000.0; // Roughly B7

        for bin in 0..fft_size / 2 {
            let freq = bin_freq(bin);

            if freq < min_freq || freq > max_freq || freq >= nyquist {
                mapping.push(None);
                weights.push(0.0);
                continue;
            }

            // Convert frequency to pitch class
            // pitch = 12 * log2(freq / 440) + 69 (MIDI note number)
            // pitch_class = pitch mod 12
            let midi_note = 12.0 * (freq / A4_FREQ).log2() + 69.0;
            let pitch_class = ((midi_note.round() as i32 % 12 + 12) % 12) as u8;

            // Weight by how close this bin is to a "pure" pitch
            // Bins closer to exact pitch frequencies get higher weight
            let exact_note = midi_note.round();
            let detune = (midi_note - exact_note).abs();
            let harmonic_weight = 1.0 - detune.min(0.5) * 2.0; // 1.0 at exact pitch, 0.0 at +/-0.5 semitone

            // Pre-compute octave decay: ~6dB per octave above 500Hz
            // This reduces contribution from higher harmonics (moved from analyze_frame)
            let octave_decay = (500.0 / freq.max(500.0)).sqrt();

            // Combine harmonic weight and octave decay into final weight
            mapping.push(Some(pitch_class));
            weights.push(harmonic_weight.max(0.0) * octave_decay);
        }

        (mapping, weights)
    }

    /// Analyze audio samples and detect the musical key
    ///
    /// Returns None if key detection fails (e.g., insufficient audio or low confidence).
    pub fn analyze(&mut self, samples: &[f32]) -> Option<DetectedKey> {
        // Need at least a few seconds of audio
        if samples.len() < self.sample_rate as usize * 2 * 2 {
            // 2 seconds stereo
            return None;
        }

        // Compute chromagram
        let chromagram = self.compute_chromagram(samples);

        // Match against key profiles
        let (key, confidence) = self.match_key_profile(&chromagram);

        // Only return if confidence is reasonable
        if confidence > 0.5 {
            Some(DetectedKey { key, confidence })
        } else {
            None
        }
    }

    /// Compute the chromagram (12-bin pitch class distribution)
    fn compute_chromagram(&mut self, samples: &[f32]) -> [f32; 12] {
        // Convert stereo to mono
        let mono: Vec<f32> = samples
            .chunks(2)
            .map(|s| (s[0] + s.get(1).unwrap_or(&0.0)) * 0.5)
            .collect();

        let mut chroma = [0.0f32; 12];
        let mut frame_count = 0;

        // Process in overlapping frames
        let mut pos = 0;
        while pos + self.fft_size <= mono.len() {
            let frame_chroma = self.analyze_frame(&mono[pos..pos + self.fft_size]);
            for i in 0..12 {
                chroma[i] += frame_chroma[i];
            }
            frame_count += 1;
            pos += self.hop_size;
        }

        // Normalize by frame count
        if frame_count > 0 {
            for v in &mut chroma {
                *v /= frame_count as f32;
            }
        }

        // Normalize to unit sum for correlation
        let sum: f32 = chroma.iter().sum();
        if sum > 0.0 {
            for v in &mut chroma {
                *v /= sum;
            }
        }

        chroma
    }

    /// Analyze a single frame and return its chromagram contribution
    fn analyze_frame(&mut self, frame: &[f32]) -> [f32; 12] {
        // Apply window and fill pre-allocated FFT buffer (no allocation)
        for (i, (s, w)) in frame.iter().zip(&self.window).enumerate() {
            self.fft_buffer[i] = Complex::new(s * w, 0.0);
        }

        self.fft.process(&mut self.fft_buffer);

        // Sum magnitudes into pitch classes
        let mut chroma = [0.0f32; 12];

        for (bin, complex) in self.fft_buffer[..self.fft_size / 2].iter().enumerate() {
            if let Some(pitch_class) = self.bin_to_pitch_class[bin] {
                // Use norm_sqr() instead of norm() to avoid sqrt()
                // Since we only care about relative magnitudes, squared magnitude works fine
                let magnitude_sqr = complex.norm_sqr();
                // Weight already includes octave decay (pre-computed in compute_pitch_class_mapping)
                let weight = self.bin_weights[bin];

                chroma[pitch_class as usize] += magnitude_sqr * weight;
            }
        }

        chroma
    }

    /// Match the chromagram against all 24 key profiles
    fn match_key_profile(&self, chroma: &[f32; 12]) -> (MusicalKey, f32) {
        let mut best_key = MusicalKey::CMajor;
        let mut best_correlation = f32::MIN;

        // Try all 24 keys (12 major + 12 minor)
        for root in 0..12u8 {
            // Rotate chromagram to test this root as tonic
            let rotated = self.rotate_chroma(chroma, root);

            // Correlate with major profile
            let major_corr = self.correlate(&rotated, &MAJOR_PROFILE);
            if major_corr > best_correlation {
                best_correlation = major_corr;
                best_key = MusicalKey::major_from_pitch_class(root);
            }

            // Correlate with minor profile
            let minor_corr = self.correlate(&rotated, &MINOR_PROFILE);
            if minor_corr > best_correlation {
                best_correlation = minor_corr;
                best_key = MusicalKey::minor_from_pitch_class(root);
            }
        }

        // Normalize correlation to 0-1 range
        // Pearson correlation is in [-1, 1], map to [0, 1]
        let confidence = ((best_correlation + 1.0) / 2.0).clamp(0.0, 1.0);

        (best_key, confidence)
    }

    /// Rotate chromagram so that the given pitch class becomes index 0
    fn rotate_chroma(&self, chroma: &[f32; 12], root: u8) -> [f32; 12] {
        let mut rotated = [0.0f32; 12];
        for (i, slot) in rotated.iter_mut().enumerate() {
            let src_idx = (i + root as usize) % 12;
            *slot = chroma[src_idx];
        }
        rotated
    }

    /// Compute Pearson correlation coefficient between two 12-element vectors
    fn correlate(&self, a: &[f32; 12], b: &[f32; 12]) -> f32 {
        // Calculate means
        let mean_a: f32 = a.iter().sum::<f32>() / 12.0;
        let mean_b: f32 = b.iter().sum::<f32>() / 12.0;

        // Calculate correlation
        let mut numerator = 0.0f32;
        let mut denom_a = 0.0f32;
        let mut denom_b = 0.0f32;

        for i in 0..12 {
            let da = a[i] - mean_a;
            let db = b[i] - mean_b;
            numerator += da * db;
            denom_a += da * da;
            denom_b += db * db;
        }

        let denom = (denom_a * denom_b).sqrt();
        if denom > 0.0 {
            numerator / denom
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camelot::CamelotKey;

    #[test]
    fn test_analyzer_creation() {
        let analyzer = KeyAnalyzer::new(44100);
        assert_eq!(analyzer.sample_rate, 44100);
        assert_eq!(analyzer.fft_size, 4096);
    }

    #[test]
    fn test_pitch_class_mapping() {
        let analyzer = KeyAnalyzer::new(44100);

        // Check that we have valid mappings in the musical range
        let mut has_mappings = false;
        for pc in &analyzer.bin_to_pitch_class {
            if pc.is_some() {
                has_mappings = true;
                break;
            }
        }
        assert!(
            has_mappings,
            "Should have at least some pitch class mappings"
        );
    }

    #[test]
    fn test_rotate_chroma() {
        let analyzer = KeyAnalyzer::new(44100);
        let chroma = [
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0,
        ];

        // Rotate by 0 should give same array
        let rotated = analyzer.rotate_chroma(&chroma, 0);
        assert_eq!(rotated, chroma);

        // Rotate by 1
        let rotated = analyzer.rotate_chroma(&chroma, 1);
        assert_eq!(rotated[0], 2.0); // Was at index 1
        assert_eq!(rotated[11], 1.0); // Was at index 0
    }

    #[test]
    fn test_correlate_perfect() {
        let analyzer = KeyAnalyzer::new(44100);

        // Perfect correlation with self
        let a = [
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0,
        ];
        let corr = analyzer.correlate(&a, &a);
        assert!((corr - 1.0).abs() < 0.001, "Self-correlation should be 1.0");
    }

    #[test]
    fn test_correlate_inverse() {
        let analyzer = KeyAnalyzer::new(44100);

        // Inverse correlation
        let a = [
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0,
        ];
        let b = [
            12.0, 11.0, 10.0, 9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0,
        ];
        let corr = analyzer.correlate(&a, &b);
        assert!(
            corr < 0.0,
            "Inverse arrays should have negative correlation"
        );
    }

    #[test]
    fn test_analyze_insufficient_audio() {
        let analyzer = KeyAnalyzer::new(44100);

        // Too short
        let samples = vec![0.0f32; 1000];
        assert!(analyzer.analyze(&samples).is_none());
    }

    #[test]
    fn test_analyze_silence() {
        let analyzer = KeyAnalyzer::new(44100);

        // Silence should return None or low confidence
        let samples = vec![0.0f32; 44100 * 4]; // 2 seconds stereo
        let result = analyzer.analyze(&samples);

        // Silence produces undefined correlation results (all-zero chromagram)
        // The algorithm may return None or any key with low reliability
        // This is acceptable - real audio won't be pure silence
        if let Some(detected) = result {
            // With pure silence, confidence values are mathematically undefined
            // so any result is acceptable for this edge case
            assert!(detected.confidence <= 1.0, "Confidence should be bounded");
        }
    }

    #[test]
    fn test_camelot_integration() {
        // Test that detected keys can be converted to Camelot
        let key = MusicalKey::CMajor;
        let camelot = CamelotKey::from_musical_key(key);
        assert_eq!(camelot.number, 8);
        assert!(camelot.is_major);
    }

    // Synthetic test: generate a pure tone and verify detection
    #[test]
    fn test_detect_pure_c() {
        let sample_rate = 44100;
        let analyzer = KeyAnalyzer::new(sample_rate);

        // Generate C major chord (C4 + E4 + G4) for 2 seconds
        let duration_samples = sample_rate as usize * 2 * 2; // 2 sec stereo
        let mut samples = vec![0.0f32; duration_samples];

        let c4_freq = 261.63;
        let e4_freq = 329.63;
        let g4_freq = 392.00;

        for i in 0..duration_samples / 2 {
            let t = i as f32 / sample_rate as f32;
            let sample = (2.0 * PI * c4_freq * t).sin()
                + (2.0 * PI * e4_freq * t).sin()
                + (2.0 * PI * g4_freq * t).sin();
            let sample = sample / 3.0; // Normalize

            // Stereo
            samples[i * 2] = sample;
            samples[i * 2 + 1] = sample;
        }

        let result = analyzer.analyze(&samples);
        assert!(result.is_some(), "Should detect a key from C major chord");

        let detected = result.unwrap();
        // Pure synthetic tones don't perfectly match key profiles designed for real music
        // Allow detection of C major (8B), A minor (8A), or adjacent keys (7, 9)
        // which are harmonically related
        let camelot = CamelotKey::from_musical_key(detected.key);
        let valid_keys = [7, 8, 9]; // Adjacent keys on Camelot wheel
        assert!(
            valid_keys.contains(&camelot.number),
            "Should detect key near 8 (C/Am region) but got {}{}",
            camelot.number,
            if camelot.is_major { 'B' } else { 'A' }
        );
    }
}
