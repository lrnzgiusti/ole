//! Enhanced waveform data structures with frequency band analysis

use rustfft::{num_complex::Complex, FftPlanner};
use std::f32::consts::PI;
use std::sync::Arc;

/// Dominant frequency band for a waveform point
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrequencyBand {
    /// Bass frequencies (<250Hz) - kicks, bass
    Bass,
    /// Mid frequencies (250Hz-4kHz) - vocals, instruments
    #[default]
    Mid,
    /// High frequencies (>4kHz) - hi-hats, cymbals, air
    High,
}

/// Single point in the enhanced waveform
#[derive(Debug, Clone, Copy, Default)]
pub struct WaveformPoint {
    /// Peak amplitude (0.0-1.0)
    pub amplitude: f32,
    /// Dominant frequency band at this point
    pub band: FrequencyBand,
}

/// Enhanced waveform data for a track
#[derive(Debug, Clone)]
pub struct EnhancedWaveform {
    /// Waveform points (typically 1000 points for full track)
    pub points: Vec<WaveformPoint>,
    /// Total track duration in seconds
    pub duration_secs: f64,
}

impl Default for EnhancedWaveform {
    fn default() -> Self {
        Self {
            points: Vec::new(),
            duration_secs: 0.0,
        }
    }
}

impl EnhancedWaveform {
    /// Create a new enhanced waveform
    pub fn new(points: Vec<WaveformPoint>, duration_secs: f64) -> Self {
        Self { points, duration_secs }
    }

    /// Create an empty waveform with given number of points
    pub fn empty(num_points: usize) -> Self {
        Self {
            points: vec![WaveformPoint::default(); num_points],
            duration_secs: 0.0,
        }
    }

    /// Wrap in Arc for efficient sharing
    pub fn into_arc(self) -> Arc<Self> {
        Arc::new(self)
    }

    /// Get the number of points
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Get amplitude at a normalized position (0.0-1.0)
    pub fn amplitude_at(&self, position: f64) -> f32 {
        if self.points.is_empty() {
            return 0.0;
        }
        let idx = ((position * self.points.len() as f64) as usize).min(self.points.len() - 1);
        self.points[idx].amplitude
    }

    /// Get frequency band at a normalized position (0.0-1.0)
    pub fn band_at(&self, position: f64) -> FrequencyBand {
        if self.points.is_empty() {
            return FrequencyBand::Mid;
        }
        let idx = ((position * self.points.len() as f64) as usize).min(self.points.len() - 1);
        self.points[idx].band
    }
}

/// Waveform analyzer that generates enhanced waveform with frequency analysis
pub struct WaveformAnalyzer {
    sample_rate: u32,
    fft_size: usize,
    fft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    window: Vec<f32>,
    fft_buffer: Vec<Complex<f32>>,
}

impl WaveformAnalyzer {
    /// Create a new waveform analyzer
    pub fn new(sample_rate: u32) -> Self {
        let fft_size = 512; // Smaller FFT for better time resolution
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(fft_size);

        // Pre-compute Hann window
        let window: Vec<f32> = (0..fft_size)
            .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / fft_size as f32).cos()))
            .collect();

        Self {
            sample_rate,
            fft_size,
            fft,
            window,
            fft_buffer: vec![Complex::new(0.0, 0.0); fft_size],
        }
    }

    /// Analyze a chunk of samples and determine dominant frequency band
    fn analyze_chunk(&mut self, samples: &[f32]) -> FrequencyBand {
        // Prepare FFT input with windowing
        let sample_count = samples.len().min(self.fft_size);
        for i in 0..sample_count {
            let windowed = samples[i] * self.window.get(i).copied().unwrap_or(0.0);
            self.fft_buffer[i] = Complex::new(windowed, 0.0);
        }
        for i in sample_count..self.fft_size {
            self.fft_buffer[i] = Complex::new(0.0, 0.0);
        }

        // Perform FFT
        self.fft.process(&mut self.fft_buffer);

        // Calculate energy in each frequency band
        let bin_width = self.sample_rate as f32 / self.fft_size as f32;

        // Bass: 0-250Hz
        let bass_end_bin = (250.0 / bin_width) as usize;
        // Mid: 250Hz-4kHz
        let mid_end_bin = (4000.0 / bin_width) as usize;
        // High: >4kHz (up to Nyquist)
        let nyquist_bin = self.fft_size / 2;

        let bass_energy: f32 = self.fft_buffer[1..bass_end_bin.min(nyquist_bin)]
            .iter()
            .map(|c| c.norm_sqr())
            .sum();

        let mid_energy: f32 = self.fft_buffer[bass_end_bin..mid_end_bin.min(nyquist_bin)]
            .iter()
            .map(|c| c.norm_sqr())
            .sum();

        let high_energy: f32 = self.fft_buffer[mid_end_bin..nyquist_bin]
            .iter()
            .map(|c| c.norm_sqr())
            .sum();

        // Normalize by band width
        let bass_avg = if bass_end_bin > 1 { bass_energy / (bass_end_bin - 1) as f32 } else { 0.0 };
        let mid_avg = if mid_end_bin > bass_end_bin { mid_energy / (mid_end_bin - bass_end_bin) as f32 } else { 0.0 };
        let high_avg = if nyquist_bin > mid_end_bin { high_energy / (nyquist_bin - mid_end_bin) as f32 } else { 0.0 };

        // Determine dominant band
        if bass_avg >= mid_avg && bass_avg >= high_avg {
            FrequencyBand::Bass
        } else if high_avg >= mid_avg {
            FrequencyBand::High
        } else {
            FrequencyBand::Mid
        }
    }

    /// Generate enhanced waveform from interleaved stereo samples
    pub fn analyze(&mut self, samples: &[f32], target_points: usize, duration_secs: f64) -> EnhancedWaveform {
        if samples.is_empty() {
            return EnhancedWaveform::empty(target_points);
        }

        let channels = 2; // Assume stereo
        let total_frames = samples.len() / channels;
        let frames_per_point = (total_frames / target_points).max(1);

        let mut points = Vec::with_capacity(target_points);

        for point_idx in 0..target_points {
            let start_frame = point_idx * frames_per_point;
            let end_frame = ((point_idx + 1) * frames_per_point).min(total_frames);

            if start_frame >= total_frames {
                points.push(WaveformPoint::default());
                continue;
            }

            // Calculate peak amplitude (mono mixdown)
            let mut max_amplitude = 0.0f32;
            let mut mono_samples = Vec::with_capacity(end_frame - start_frame);

            for frame in start_frame..end_frame {
                let idx = frame * channels;
                if idx + 1 < samples.len() {
                    let mono = (samples[idx] + samples[idx + 1]) * 0.5;
                    max_amplitude = max_amplitude.max(mono.abs());
                    mono_samples.push(mono);
                }
            }

            // Analyze frequency band for this chunk
            let band = if mono_samples.len() >= self.fft_size / 4 {
                self.analyze_chunk(&mono_samples)
            } else {
                FrequencyBand::Mid
            };

            points.push(WaveformPoint {
                amplitude: max_amplitude.min(1.0),
                band,
            });
        }

        EnhancedWaveform::new(points, duration_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_waveform() {
        let wf = EnhancedWaveform::empty(100);
        assert_eq!(wf.len(), 100);
        assert_eq!(wf.amplitude_at(0.5), 0.0);
        assert_eq!(wf.band_at(0.5), FrequencyBand::Mid);
    }

    #[test]
    fn test_waveform_access() {
        let points = vec![
            WaveformPoint { amplitude: 0.5, band: FrequencyBand::Bass },
            WaveformPoint { amplitude: 0.8, band: FrequencyBand::Mid },
            WaveformPoint { amplitude: 0.3, band: FrequencyBand::High },
        ];
        let wf = EnhancedWaveform::new(points, 3.0);

        assert_eq!(wf.amplitude_at(0.0), 0.5);
        assert_eq!(wf.band_at(0.0), FrequencyBand::Bass);
        assert_eq!(wf.amplitude_at(0.5), 0.8);
        assert_eq!(wf.band_at(0.99), FrequencyBand::High);
    }
}
