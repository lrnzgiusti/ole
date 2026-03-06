//! Recording - capture master output to WAV file

use std::path::Path;

/// Maximum recording duration (~30 minutes at 48kHz stereo)
const MAX_RECORDING_SAMPLES: usize = 48000 * 2 * 60 * 30;

pub struct RecordingState {
    pub is_recording: bool,
    buffer: Vec<f32>,
    sample_rate: u32,
}

impl RecordingState {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            is_recording: false,
            buffer: Vec::new(),
            sample_rate,
        }
    }

    pub fn start(&mut self) {
        self.buffer.clear();
        // Pre-allocate full capacity to avoid allocations in audio callback
        self.buffer.reserve(MAX_RECORDING_SAMPLES);
        self.is_recording = true;
    }

    pub fn stop(&mut self) {
        self.is_recording = false;
    }

    /// Add samples from the master output (called in audio callback)
    pub fn add_samples(&mut self, samples: &[f32]) {
        if !self.is_recording {
            return;
        }
        // Safety limit to prevent unbounded memory usage
        if self.buffer.len() + samples.len() > MAX_RECORDING_SAMPLES {
            self.is_recording = false;
            return;
        }
        self.buffer.extend_from_slice(samples);
    }

    /// Duration of recording in seconds
    pub fn duration_secs(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.buffer.len() as f64 / (self.sample_rate as f64 * 2.0)
    }

    /// Save recording to WAV file
    pub fn save_wav(&self, path: &Path) -> Result<(), String> {
        if self.buffer.is_empty() {
            return Err("No recording to save".to_string());
        }

        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: self.sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let mut writer = hound::WavWriter::create(path, spec)
            .map_err(|e| format!("Failed to create WAV file: {}", e))?;

        for &sample in &self.buffer {
            writer.write_sample(sample)
                .map_err(|e| format!("Failed to write sample: {}", e))?;
        }

        writer.finalize()
            .map_err(|e| format!("Failed to finalize WAV: {}", e))?;

        Ok(())
    }

    /// Take the buffer out (for saving on the main thread)
    pub fn take_buffer(&mut self) -> (Vec<f32>, u32) {
        let buf = std::mem::take(&mut self.buffer);
        (buf, self.sample_rate)
    }
}
