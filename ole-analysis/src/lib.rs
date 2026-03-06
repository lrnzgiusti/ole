//! Audio analysis module for OLE
//!
//! Provides spectrum analysis, BPM detection, beat grid analysis,
//! waveform analysis, and musical key detection capabilities.

mod beatgrid;
mod bpm;
mod camelot;
mod key;
mod spectrum;
pub mod phrase;
mod waveform;

pub use beatgrid::{BeatGrid, BeatGridAnalyzer};
pub use bpm::BpmDetector;
pub use camelot::{CamelotKey, MusicalKey};
pub use key::{DetectedKey, KeyAnalyzer};
pub use phrase::{PhraseMarker, PhraseType};
pub use spectrum::{SpectrumAnalyzer, SpectrumData, SPECTRUM_BANDS};
pub use waveform::{EnhancedWaveform, FrequencyBand, WaveformAnalyzer, WaveformPoint};

/// Compute overall RMS energy level of a stereo track, normalized 0.0–1.0.
///
/// Input: interleaved stereo samples. Returns 0.0 for empty input.
pub fn compute_energy_level(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let rms = (sum_sq / samples.len() as f64).sqrt() as f32;
    // Normalize: typical mastered music RMS is ~0.1–0.3.
    // Map so that 0.25 RMS → ~1.0, with clamp.
    (rms * 4.0).min(1.0)
}
