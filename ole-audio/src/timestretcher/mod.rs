//! State-of-the-art time-stretching module for pitch-independent tempo control.
//!
//! This module provides a phase vocoder implementation that allows changing
//! playback speed without affecting pitch - essential for DJ beat matching.
//!
//! # Features
//!
//! - **High-quality phase vocoder**: STFT-based with 75% overlap
//! - **Transient preservation**: Detects attacks and resets phase
//! - **Phase locking**: Prevents phasy artifacts on harmonics
//! - **Zero-allocation**: All buffers pre-allocated
//! - **Optimized FFT**: Split-radix with pre-computed twiddles
//!
//! # Usage
//!
//! ```rust,ignore
//! use ole_audio::timestretcher::{PhaseVocoder, FftSize};
//!
//! let mut vocoder = PhaseVocoder::new(FftSize::Medium);
//! vocoder.set_stretch_ratio(1.5); // 50% slower, same pitch
//!
//! // In audio callback
//! for (left, right) in input.iter() {
//!     if let Some((out_l, out_r)) = vocoder.process(*left, *right) {
//!         // Use output samples
//!     }
//! }
//! ```
//!
//! # Algorithm Details
//!
//! The phase vocoder works by:
//! 1. Windowing input into overlapping frames (STFT)
//! 2. Converting to frequency domain via FFT
//! 3. Manipulating phase to stretch/compress time
//! 4. Converting back via IFFT
//! 5. Overlap-add to reconstruct signal
//!
//! Key innovations in this implementation:
//! - **Peak-locked phase**: Bins near spectral peaks inherit peak's phase
//! - **Transient detection**: Spectral flux triggers phase reset
//! - **Fast math**: Custom sqrt, atan2, sincos approximations

mod phase;
mod stft;

pub use phase::{PhaseLockMode, PhaseVocoder, TimeStretchParams};
pub use stft::{Complex, FftSize, Stft};

/// Pitch shift without tempo change (future feature)
/// Uses phase vocoder + resampling
pub struct PitchShifter {
    vocoder: PhaseVocoder,
    /// Pitch shift in semitones
    semitones: f32,
    /// Resampling ratio to compensate for time stretch
    resample_ratio: f32,
    /// Resampling filter state
    resample_state_l: f32,
    resample_state_r: f32,
}

impl PitchShifter {
    /// Create pitch shifter
    pub fn new() -> Self {
        Self {
            vocoder: PhaseVocoder::new(FftSize::Medium),
            semitones: 0.0,
            resample_ratio: 1.0,
            resample_state_l: 0.0,
            resample_state_r: 0.0,
        }
    }

    /// Set pitch shift in semitones (-12 to +12)
    pub fn set_semitones(&mut self, semitones: f32) {
        self.semitones = semitones.clamp(-12.0, 12.0);

        // Calculate ratio: 2^(semitones/12)
        // Positive semitones = higher pitch = faster playback = stretch to compensate
        let pitch_ratio = 2.0f32.powf(self.semitones / 12.0);
        self.resample_ratio = pitch_ratio;
        self.vocoder.set_stretch_ratio(pitch_ratio);
    }

    /// Get current pitch shift
    pub fn semitones(&self) -> f32 {
        self.semitones
    }

    /// Process sample (simplified - full resampling would need interpolation)
    pub fn process(&mut self, left: f32, right: f32) -> Option<(f32, f32)> {
        // For now, just delegate to vocoder
        // Full implementation would add variable-rate resampling here
        self.vocoder.process(left, right)
    }

    /// Reset state
    pub fn reset(&mut self) {
        self.vocoder.reset();
        self.resample_state_l = 0.0;
        self.resample_state_r = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pitch_shifter_semitone_clamping() {
        let mut ps = PitchShifter::new();

        ps.set_semitones(24.0);
        assert_eq!(ps.semitones(), 12.0);

        ps.set_semitones(-24.0);
        assert_eq!(ps.semitones(), -12.0);

        ps.set_semitones(3.0);
        assert_eq!(ps.semitones(), 3.0);
    }
}
