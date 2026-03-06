//! Phrase detection for waveform intelligence
//!
//! Detects structural boundaries (intro, build, drop, break, outro)
//! from energy curves derived from waveform data.

use crate::EnhancedWaveform;

/// Type of phrase section
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhraseType {
    Intro,
    Build,
    Drop,
    Break,
    Outro,
}

impl PhraseType {
    /// Short label for waveform display
    pub fn label(self) -> &'static str {
        match self {
            Self::Intro => "IN",
            Self::Build => "BLD",
            Self::Drop => "DRP",
            Self::Break => "BRK",
            Self::Outro => "OUT",
        }
    }
}

/// A detected phrase boundary marker
#[derive(Debug, Clone)]
pub struct PhraseMarker {
    /// Position in seconds from start of track
    pub position_secs: f64,
    /// Type of phrase section starting here
    pub phrase_type: PhraseType,
    /// Length of this section in bars
    pub length_bars: u16,
}

/// Compute an energy curve by aggregating waveform amplitude over windows.
///
/// Takes an EnhancedWaveform (typically 1000 points) and aggregates every
/// `window` points into one energy value, producing a smoothed energy curve.
pub fn compute_energy_curve(waveform: &EnhancedWaveform, window: usize) -> Vec<f32> {
    if waveform.points.is_empty() || window == 0 {
        return Vec::new();
    }

    waveform
        .points
        .chunks(window)
        .map(|chunk| {
            let sum: f32 = chunk.iter().map(|p| p.amplitude).sum();
            sum / chunk.len() as f32
        })
        .collect()
}

/// Detect phrase boundaries from energy data and beat grid info.
///
/// Walks along bar boundaries (4 or 8 bars) and classifies energy transitions
/// as intro, build, drop, break, or outro based on energy direction and position.
///
/// # Arguments
/// * `bpm` - Track BPM
/// * `first_beat_offset` - First beat offset in seconds
/// * `duration` - Track duration in seconds
/// * `energy_curve` - Smoothed energy curve from `compute_energy_curve()`
/// * `step_secs` - Seconds per energy curve point (duration / energy_curve.len())
pub fn detect_phrases(
    bpm: f32,
    first_beat_offset: f64,
    duration: f64,
    energy_curve: &[f32],
    step_secs: f64,
) -> Vec<PhraseMarker> {
    if energy_curve.is_empty() || bpm <= 0.0 || duration <= 0.0 || step_secs <= 0.0 {
        return Vec::new();
    }

    let beats_per_sec = bpm as f64 / 60.0;
    let secs_per_bar = 4.0 / beats_per_sec; // 4 beats per bar
    let phrase_bars = 8u16; // Analyze in 8-bar chunks
    let phrase_secs = phrase_bars as f64 * secs_per_bar;

    // Compute average energy of the full track for relative thresholds
    let avg_energy: f32 =
        energy_curve.iter().sum::<f32>() / energy_curve.len() as f32;
    let threshold = 0.30; // 30% relative change to detect transition

    let mut markers = Vec::new();
    let mut pos = first_beat_offset;

    // Walk 8-bar boundaries
    while pos + phrase_secs <= duration {
        let idx_start = ((pos / step_secs) as usize).min(energy_curve.len().saturating_sub(1));
        let idx_end =
            (((pos + phrase_secs) / step_secs) as usize).min(energy_curve.len().saturating_sub(1));

        if idx_start >= idx_end {
            pos += phrase_secs;
            continue;
        }

        // Energy at start and end of this phrase chunk
        let e_start = energy_curve[idx_start];
        let e_end = energy_curve[idx_end];
        let e_max = energy_curve[idx_start..=idx_end]
            .iter()
            .cloned()
            .fold(0.0f32, f32::max);

        let fraction = pos / duration; // Position as fraction of track

        // Classify based on energy direction and track position
        let delta = (e_end - e_start) / avg_energy.max(0.001);

        let phrase_type = if fraction < 0.05 && e_start < avg_energy * 0.5 {
            // Very start of track with low energy = Intro
            Some(PhraseType::Intro)
        } else if fraction > 0.85 && e_end < avg_energy * 0.5 {
            // Near end with low energy = Outro
            Some(PhraseType::Outro)
        } else if delta > threshold && e_end > avg_energy {
            // Rising energy = Build
            Some(PhraseType::Build)
        } else if delta < -threshold && e_start > avg_energy {
            // Falling energy from high level = Break
            Some(PhraseType::Break)
        } else if e_max > avg_energy * 1.3 && e_start > avg_energy {
            // Sustained high energy = Drop
            Some(PhraseType::Drop)
        } else {
            None
        };

        if let Some(pt) = phrase_type {
            // Avoid duplicate markers too close together
            let dominated = markers
                .last()
                .is_some_and(|m: &PhraseMarker| (pos - m.position_secs) < secs_per_bar * 4.0);

            if !dominated {
                markers.push(PhraseMarker {
                    position_secs: pos,
                    phrase_type: pt,
                    length_bars: phrase_bars,
                });
            }
        }

        pos += phrase_secs;
    }

    markers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FrequencyBand, WaveformPoint};

    #[test]
    fn test_compute_energy_curve_empty() {
        let wf = EnhancedWaveform::default();
        assert!(compute_energy_curve(&wf, 10).is_empty());
    }

    #[test]
    fn test_compute_energy_curve_basic() {
        let points: Vec<WaveformPoint> = (0..100)
            .map(|i| WaveformPoint {
                amplitude: i as f32 / 100.0,
                band: FrequencyBand::Mid,
            })
            .collect();
        let wf = EnhancedWaveform {
            points,
            duration_secs: 240.0,
        };
        let curve = compute_energy_curve(&wf, 10);
        assert_eq!(curve.len(), 10);
        // First window (0..9) avg ≈ 0.045, last window (90..99) avg ≈ 0.945
        assert!(curve[0] < curve[9]);
    }

    #[test]
    fn test_detect_phrases_flat_energy() {
        // Flat energy should produce minimal markers
        let curve = vec![0.5; 100];
        let markers = detect_phrases(128.0, 0.0, 240.0, &curve, 2.4);
        // Flat energy = no transitions detected (no build/break/drop)
        // Might detect intro/outro based on position
        for m in &markers {
            assert!(m.phrase_type == PhraseType::Intro || m.phrase_type == PhraseType::Outro);
        }
    }

    #[test]
    fn test_detect_phrases_spike() {
        // Low → high spike at middle
        let mut curve = vec![0.1; 100];
        for i in 40..60 {
            curve[i] = 0.9;
        }
        let markers = detect_phrases(128.0, 0.0, 240.0, &curve, 2.4);
        // Should detect build or drop around the spike
        assert!(!markers.is_empty());
    }
}
