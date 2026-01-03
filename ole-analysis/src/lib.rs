//! Audio analysis module for OLE
//!
//! Provides spectrum analysis, BPM detection, beat grid analysis,
//! and musical key detection capabilities.

mod beatgrid;
mod bpm;
mod camelot;
mod key;
mod spectrum;

pub use beatgrid::{BeatGrid, BeatGridAnalyzer};
pub use bpm::BpmDetector;
pub use camelot::{CamelotKey, MusicalKey};
pub use key::{DetectedKey, KeyAnalyzer};
pub use spectrum::{SpectrumAnalyzer, SpectrumData, SPECTRUM_BANDS};
