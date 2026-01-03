//! BPM detection using onset detection and autocorrelation

use std::collections::VecDeque;

/// BPM detector using energy-based onset detection
///
/// Note: This is a legacy fallback detector. For accurate beat detection,
/// prefer using `BeatGridAnalyzer` which uses spectral flux analysis.
pub struct BpmDetector {
    sample_rate: u32,
    energy_history: VecDeque<f32>,
    /// Onset times - using VecDeque for O(1) pop_front
    onset_times: VecDeque<f32>,
    detected_bpm: Option<f32>,
    time_accumulated: f32,
}

impl BpmDetector {
    /// Create a new BPM detector
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            energy_history: VecDeque::with_capacity(100),
            onset_times: VecDeque::new(),
            detected_bpm: None,
            time_accumulated: 0.0,
        }
    }

    /// Process a chunk of audio samples
    pub fn process(&mut self, samples: &[f32]) {
        // Calculate RMS energy of this chunk
        let energy: f32 = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
        let energy = energy.sqrt();

        // Add to history
        self.energy_history.push_back(energy);
        if self.energy_history.len() > 50 {
            self.energy_history.pop_front();
        }

        // Calculate local average
        let avg_energy: f32 =
            self.energy_history.iter().sum::<f32>() / self.energy_history.len() as f32;

        // Detect onset (energy significantly above average)
        if energy > avg_energy * 1.5 && energy > 0.01 {
            // Check if enough time has passed since last onset (debounce)
            let last_onset = self.onset_times.back().copied().unwrap_or(-1.0);
            if self.time_accumulated - last_onset > 0.1 {
                self.onset_times.push_back(self.time_accumulated);

                // Keep only recent onsets (last 30 seconds) - O(1) with VecDeque
                while let Some(&first) = self.onset_times.front() {
                    if self.time_accumulated - first > 30.0 {
                        self.onset_times.pop_front();
                    } else {
                        break;
                    }
                }

                // Calculate BPM if we have enough onsets
                if self.onset_times.len() >= 8 {
                    self.calculate_bpm();
                }
            }
        }

        self.time_accumulated += samples.len() as f32 / self.sample_rate as f32;
    }

    /// Calculate BPM from onset intervals
    fn calculate_bpm(&mut self) {
        if self.onset_times.len() < 4 {
            return;
        }

        // Calculate intervals between onsets
        let mut intervals: Vec<f32> = Vec::new();
        for i in 1..self.onset_times.len() {
            let interval = self.onset_times[i] - self.onset_times[i - 1];
            if interval > 0.2 && interval < 2.0 {
                // Filter unreasonable intervals
                intervals.push(interval);
            }
        }

        if intervals.is_empty() {
            return;
        }

        // Build histogram of intervals (quantized to 10ms)
        let mut histogram = [0u32; 200]; // 0.0 to 2.0 seconds
        for &interval in &intervals {
            let idx = ((interval * 100.0) as usize).min(199);
            histogram[idx] += 1;
        }

        // Find the most common interval
        let (peak_idx, _) = histogram
            .iter()
            .enumerate()
            .max_by_key(|(_, &count)| count)
            .unwrap();

        let peak_interval = peak_idx as f32 / 100.0;
        if peak_interval > 0.0 {
            let bpm = 60.0 / peak_interval;

            // Normalize to reasonable DJ range (70-180 BPM)
            let normalized_bpm = if bpm < 70.0 {
                bpm * 2.0
            } else if bpm > 180.0 {
                bpm / 2.0
            } else {
                bpm
            };

            self.detected_bpm = Some(normalized_bpm);
        }
    }

    /// Get the detected BPM (if available)
    pub fn bpm(&self) -> Option<f32> {
        self.detected_bpm
    }

    /// Reset the detector
    pub fn reset(&mut self) {
        self.energy_history.clear();
        self.onset_times.clear();
        self.detected_bpm = None;
        self.time_accumulated = 0.0;
    }
}
