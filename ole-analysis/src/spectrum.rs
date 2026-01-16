//! FFT-based spectrum analyzer for real-time visualization

use rustfft::{num_complex::Complex, FftPlanner};
use std::f32::consts::PI;

/// Number of frequency bands in the spectrum display
pub const SPECTRUM_BANDS: usize = 32;

/// Spectrum data for visualization
#[derive(Clone, Copy, Debug, Default)]
pub struct SpectrumData {
    /// Magnitude per band (0.0 - 1.0)
    pub bands: [f32; SPECTRUM_BANDS],
    /// Peak level (0.0 - 1.0)
    pub peak: f32,
}

/// Real-time FFT spectrum analyzer
pub struct SpectrumAnalyzer {
    sample_rate: u32,
    fft_size: usize,
    fft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    window: Vec<f32>,
    frequency_bands: [(f32, f32); SPECTRUM_BANDS],
    smoothing: f32,
    previous_magnitudes: [f32; SPECTRUM_BANDS],
    /// Pre-allocated FFT buffer to avoid allocation in analyze()
    fft_buffer: Vec<Complex<f32>>,
}

impl SpectrumAnalyzer {
    /// Create a new spectrum analyzer
    pub fn new(sample_rate: u32) -> Self {
        let fft_size = 2048;
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(fft_size);

        // Pre-compute Hann window
        let window: Vec<f32> = (0..fft_size)
            .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / fft_size as f32).cos()))
            .collect();

        // Create logarithmically spaced frequency bands (20Hz - 20kHz)
        let mut bands = [(0.0f32, 0.0f32); SPECTRUM_BANDS];
        let min_freq = 20.0f32;
        let max_freq = 20000.0f32.min(sample_rate as f32 / 2.0);
        let log_min = min_freq.ln();
        let log_max = max_freq.ln();

        for (i, band) in bands.iter_mut().enumerate() {
            let t0 = i as f32 / SPECTRUM_BANDS as f32;
            let t1 = (i + 1) as f32 / SPECTRUM_BANDS as f32;
            *band = (
                (log_min + t0 * (log_max - log_min)).exp(),
                (log_min + t1 * (log_max - log_min)).exp(),
            );
        }

        Self {
            sample_rate,
            fft_size,
            fft,
            window,
            frequency_bands: bands,
            smoothing: 0.7,
            previous_magnitudes: [0.0; SPECTRUM_BANDS],
            // Pre-allocate FFT buffer to avoid allocation in analyze()
            fft_buffer: vec![Complex::new(0.0, 0.0); fft_size],
        }
    }

    /// Analyze a buffer of mono samples and return band magnitudes
    pub fn analyze(&mut self, samples: &[f32]) -> [f32; SPECTRUM_BANDS] {
        // Prepare FFT input buffer with windowing (reuse pre-allocated buffer)
        let sample_count = samples.len().min(self.fft_size);
        for (i, &sample) in samples.iter().enumerate().take(sample_count) {
            let windowed = sample * self.window.get(i).copied().unwrap_or(0.0);
            self.fft_buffer[i] = Complex::new(windowed, 0.0);
        }
        // Zero pad the rest
        for buf in self.fft_buffer.iter_mut().skip(sample_count) {
            *buf = Complex::new(0.0, 0.0);
        }

        // Perform FFT
        self.fft.process(&mut self.fft_buffer);

        // Calculate magnitudes per band
        let mut magnitudes = [0.0f32; SPECTRUM_BANDS];
        let bin_width = self.sample_rate as f32 / self.fft_size as f32;

        for (i, &(low, high)) in self.frequency_bands.iter().enumerate() {
            let start_bin = (low / bin_width) as usize;
            let end_bin = ((high / bin_width) as usize).min(self.fft_size / 2);

            if start_bin < end_bin {
                let sum: f32 = self.fft_buffer[start_bin..end_bin]
                    .iter()
                    .map(|c| c.norm())
                    .sum();
                magnitudes[i] = sum / (end_bin - start_bin) as f32;
            }
        }

        // Normalize to 0-1 range (approximate based on typical values)
        let max_magnitude = magnitudes.iter().cloned().fold(0.0f32, f32::max);
        if max_magnitude > 0.0 {
            for mag in &mut magnitudes {
                *mag /= max_magnitude.max(100.0);
                *mag = mag.clamp(0.0, 1.0);
            }
        }

        // Apply smoothing
        for (mag, prev) in magnitudes
            .iter_mut()
            .zip(self.previous_magnitudes.iter_mut())
        {
            *mag = *prev * self.smoothing + *mag * (1.0 - self.smoothing);
            *prev = *mag;
        }

        magnitudes
    }

    /// Get the peak level from samples (0.0 - 1.0)
    pub fn peak_level(samples: &[f32]) -> f32 {
        samples
            .iter()
            .map(|s| s.abs())
            .fold(0.0f32, f32::max)
            .min(1.0)
    }

    /// Process samples and return SpectrumData
    pub fn process(&mut self, samples: &[f32]) -> SpectrumData {
        let bands = self.analyze(samples);
        let peak = Self::peak_level(samples);
        SpectrumData { bands, peak }
    }
}
