//! Audio file loading and decoding

use ole_analysis::{EnhancedWaveform, WaveformAnalyzer};
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use thiserror::Error;

/// Errors that can occur during track loading
#[derive(Error, Debug)]
pub enum LoadError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("No audio track found in file")]
    NoAudioTrack,
    #[error("Unsupported format")]
    UnsupportedFormat,
    #[error("Decode error: {0}")]
    Decode(String),
}

/// Track metadata
#[derive(Debug, Clone, Default)]
pub struct TrackMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_secs: f64,
    pub sample_rate: u32,
    pub channels: u16,
}

/// A loaded and decoded audio track
pub struct LoadedTrack {
    /// Interleaved stereo samples (f32, normalized to -1.0 to 1.0)
    pub samples: Vec<f32>,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Number of channels (typically 2 for stereo)
    pub channels: u16,
    /// Track metadata
    pub metadata: TrackMetadata,
    /// Pre-computed waveform overview for display (downsampled peaks)
    pub waveform_overview: Vec<f32>,
    /// Enhanced waveform with frequency band analysis
    pub enhanced_waveform: EnhancedWaveform,
}

/// Audio file loader using Symphonia
pub struct TrackLoader {
    target_sample_rate: u32,
}

impl Default for TrackLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl TrackLoader {
    /// Create a new track loader with default 48kHz sample rate
    pub fn new() -> Self {
        Self::with_sample_rate(48000)
    }

    /// Create a new track loader with specific sample rate
    pub fn with_sample_rate(target_sample_rate: u32) -> Self {
        Self { target_sample_rate }
    }

    /// Load and decode an audio file
    pub fn load(&self, path: &Path) -> Result<LoadedTrack, LoadError> {
        // Open the file
        let file = std::fs::File::open(path)?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        // Create hint from file extension
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        // Probe the format
        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| LoadError::Decode(e.to_string()))?;

        let mut format = probed.format;

        // Find first audio track
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or(LoadError::NoAudioTrack)?;

        let track_id = track.id;
        let codec_params = track.codec_params.clone();

        // Get track info
        let source_sample_rate = codec_params.sample_rate.unwrap_or(44100);
        let channels = codec_params
            .channels
            .map(|c| c.count() as u16)
            .unwrap_or(2);

        // Create decoder
        let mut decoder = symphonia::default::get_codecs()
            .make(&codec_params, &DecoderOptions::default())
            .map_err(|e| LoadError::Decode(e.to_string()))?;

        // Extract metadata
        let mut metadata = self.extract_metadata(&mut format, path);
        metadata.sample_rate = source_sample_rate;
        metadata.channels = channels;

        // Decode all samples
        let mut samples: Vec<f32> = Vec::new();

        loop {
            let packet = match format.next_packet() {
                Ok(p) => p,
                Err(symphonia::core::errors::Error::IoError(ref e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                Err(_) => break,
            };

            if packet.track_id() != track_id {
                continue;
            }

            let decoded = match decoder.decode(&packet) {
                Ok(d) => d,
                Err(_) => continue,
            };

            // Convert to f32 interleaved
            let spec = *decoded.spec();
            let duration = decoded.capacity() as u64;

            let mut sample_buf = SampleBuffer::<f32>::new(duration, spec);
            sample_buf.copy_interleaved_ref(decoded);
            samples.extend_from_slice(sample_buf.samples());
        }

        // Calculate duration
        let total_frames = samples.len() / channels as usize;
        metadata.duration_secs = total_frames as f64 / source_sample_rate as f64;

        // Resample if needed
        let (samples, final_sample_rate) = if source_sample_rate != self.target_sample_rate {
            (
                self.resample(&samples, source_sample_rate, channels)?,
                self.target_sample_rate,
            )
        } else {
            (samples, source_sample_rate)
        };

        // Generate waveform overview
        let waveform_overview = self.generate_waveform_overview(&samples, 1000);

        // Generate enhanced waveform with frequency analysis
        let mut waveform_analyzer = WaveformAnalyzer::new(final_sample_rate);
        let enhanced_waveform = waveform_analyzer.analyze(&samples, 1000, metadata.duration_secs);

        Ok(LoadedTrack {
            samples,
            sample_rate: final_sample_rate,
            channels,
            metadata,
            waveform_overview,
            enhanced_waveform,
        })
    }

    /// Resample audio to target sample rate
    fn resample(
        &self,
        samples: &[f32],
        source_rate: u32,
        channels: u16,
    ) -> Result<Vec<f32>, LoadError> {
        use rubato::{FftFixedInOut, Resampler};

        let channels_usize = channels as usize;
        let frames = samples.len() / channels_usize;

        // Create resampler
        let mut resampler = FftFixedInOut::<f32>::new(
            source_rate as usize,
            self.target_sample_rate as usize,
            1024,
            channels_usize,
        )
        .map_err(|e| LoadError::Decode(e.to_string()))?;

        // Deinterleave
        let deinterleaved: Vec<Vec<f32>> = (0..channels_usize)
            .map(|ch| {
                (0..frames)
                    .map(|f| samples[f * channels_usize + ch])
                    .collect()
            })
            .collect();

        // Process in chunks
        let chunk_size = resampler.input_frames_next();
        let mut output: Vec<Vec<f32>> = vec![Vec::new(); channels_usize];

        let mut pos = 0;
        while pos + chunk_size <= frames {
            let input_refs: Vec<&[f32]> = deinterleaved
                .iter()
                .map(|ch| &ch[pos..pos + chunk_size])
                .collect();

            let resampled = resampler
                .process(&input_refs, None)
                .map_err(|e| LoadError::Decode(e.to_string()))?;

            for (ch, data) in resampled.into_iter().enumerate() {
                output[ch].extend(data);
            }

            pos += chunk_size;
        }

        // Handle remaining samples (pad with zeros)
        if pos < frames {
            let remaining = frames - pos;
            let padded: Vec<Vec<f32>> = deinterleaved
                .iter()
                .map(|ch| {
                    let mut v = ch[pos..].to_vec();
                    v.resize(chunk_size, 0.0);
                    v
                })
                .collect();

            let input_refs: Vec<&[f32]> = padded.iter().map(|v| v.as_slice()).collect();

            if let Ok(resampled) = resampler.process(&input_refs, None) {
                for (ch, data) in resampled.into_iter().enumerate() {
                    // Only take the proportional amount of output
                    let output_frames = (remaining * self.target_sample_rate as usize)
                        / source_rate as usize;
                    output[ch].extend(&data[..output_frames.min(data.len())]);
                }
            }
        }

        // Reinterleave
        let output_frames = output[0].len();
        let mut interleaved = Vec::with_capacity(output_frames * channels_usize);
        for frame_idx in 0..output_frames {
            for channel in &output {
                interleaved.push(channel[frame_idx]);
            }
        }

        Ok(interleaved)
    }

    /// Generate a downsampled waveform for display
    fn generate_waveform_overview(&self, samples: &[f32], target_points: usize) -> Vec<f32> {
        if samples.is_empty() {
            return vec![0.0; target_points];
        }

        let chunk_size = (samples.len() / target_points).max(1);

        samples
            .chunks(chunk_size)
            .map(|chunk| chunk.iter().map(|s| s.abs()).fold(0.0f32, f32::max))
            .collect()
    }

    /// Extract metadata from format reader
    fn extract_metadata(
        &self,
        format: &mut Box<dyn symphonia::core::formats::FormatReader>,
        path: &Path,
    ) -> TrackMetadata {
        let mut metadata = TrackMetadata {
            title: path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown")
                .to_string(),
            artist: "Unknown".to_string(),
            album: "Unknown".to_string(),
            ..Default::default()
        };

        if let Some(meta) = format.metadata().current() {
            for tag in meta.tags() {
                match tag.std_key {
                    Some(symphonia::core::meta::StandardTagKey::TrackTitle) => {
                        metadata.title = tag.value.to_string();
                    }
                    Some(symphonia::core::meta::StandardTagKey::Artist) => {
                        metadata.artist = tag.value.to_string();
                    }
                    Some(symphonia::core::meta::StandardTagKey::Album) => {
                        metadata.album = tag.value.to_string();
                    }
                    _ => {}
                }
            }
        }

        metadata
    }
}
