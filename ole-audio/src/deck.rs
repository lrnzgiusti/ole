//! Deck implementation - track playback with pitch/tempo control

use ole_analysis::{
    BeatGrid, BeatGridAnalyzer, BpmDetector, EnhancedWaveform, SpectrumAnalyzer, SpectrumData,
};
use std::sync::Arc;

/// Playback state for a deck
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

/// State tracking for smooth sync transitions
#[derive(Debug, Clone, Default)]
pub struct SyncTransition {
    /// Target tempo to reach
    pub target_tempo: f32,
    /// Starting tempo
    pub start_tempo: f32,
    /// Target phase offset in samples to apply
    pub target_phase_offset: f64,
    /// Phase offset already applied
    pub applied_phase_offset: f64,
    /// Transition progress (0.0 - 1.0)
    pub progress: f32,
    /// Duration of transition in samples
    pub duration_samples: u64,
    /// Samples processed in transition
    pub samples_processed: u64,
    /// Whether transition is active
    pub active: bool,
}

/// Beat grid info for UI display
#[derive(Debug, Clone, Default)]
pub struct BeatGridInfo {
    pub bpm: f32,
    pub confidence: f32,
    pub has_grid: bool,
    /// First beat offset in seconds (for rendering beat markers on waveform)
    pub first_beat_offset_secs: f64,
}

/// Size of scope buffer for oscilloscope display
pub const SCOPE_SAMPLES_SIZE: usize = 512;

/// Complete deck state for UI rendering
#[derive(Debug, Clone)]
pub struct DeckState {
    pub playback: PlaybackState,
    pub position: f64,       // seconds
    pub duration: f64,       // seconds
    pub tempo: f32,          // 1.0 = original speed
    pub pitch: f32,          // semitones shift
    pub gain: f32,           // 0.0 - 2.0
    pub bpm: Option<f32>,    // detected BPM (adjusted for tempo)
    pub key: Option<String>, // Camelot notation: "8A", "12B"
    pub track_name: Option<String>,
    pub spectrum: SpectrumData,
    pub beat_phase: f32, // current phase within beat (0.0 - 1.0)
    pub beat_grid_info: Option<BeatGridInfo>,
    pub waveform_overview: Arc<Vec<f32>>, // pre-computed peaks for waveform display
    pub enhanced_waveform: Arc<EnhancedWaveform>, // enhanced waveform with frequency bands
    pub peak_level: f32,                  // current peak level (0.0-1.0+, >1.0 = clipping)
    pub peak_hold: f32,                   // peak hold level (decays slowly after hold time)
    pub is_clipping: bool,                // true if clipping detected
    pub cue_points: [Option<f64>; 8],     // cue point positions in seconds (1-8)
    /// Recent audio samples for oscilloscope display (stereo interleaved: [L, R, L, R, ...])
    pub scope_samples: Box<[f32; SCOPE_SAMPLES_SIZE * 2]>,
}

impl Default for DeckState {
    fn default() -> Self {
        Self {
            playback: PlaybackState::Stopped,
            position: 0.0,
            duration: 0.0,
            tempo: 1.0,
            pitch: 0.0,
            gain: 1.0,
            bpm: None,
            key: None,
            track_name: None,
            spectrum: SpectrumData::default(),
            beat_phase: 0.0,
            beat_grid_info: None,
            waveform_overview: Arc::new(Vec::new()),
            enhanced_waveform: Arc::new(EnhancedWaveform::default()),
            peak_level: 0.0,
            peak_hold: 0.0,
            is_clipping: false,
            cue_points: [None; 8],
            scope_samples: Box::new([0.0; SCOPE_SAMPLES_SIZE * 2]),
        }
    }
}

/// A single DJ deck with audio playback capabilities
pub struct Deck {
    /// Audio samples (interleaved stereo) - Arc to avoid copying through channels
    samples: Arc<Vec<f32>>,
    /// Sample rate of loaded audio
    sample_rate: u32,
    /// Current playback position in samples
    position: f64,
    /// Playback state
    state: PlaybackState,
    /// Playback speed (1.0 = normal)
    tempo: f32,
    /// Pitch shift in semitones
    pitch: f32,
    /// Volume gain
    gain: f32,
    /// Track name
    track_name: Option<String>,
    /// Detected key in Camelot notation (e.g., "8A", "12B")
    key: Option<String>,
    /// Detected BPM (from beat grid or legacy detector)
    bpm: Option<f32>,
    /// Beat grid for phase-aligned sync
    beat_grid: Option<BeatGrid>,
    /// Sync transition state for smooth syncing
    sync_transition: SyncTransition,
    /// Spectrum analyzer
    spectrum_analyzer: SpectrumAnalyzer,
    /// BPM detector (legacy, used as fallback)
    bpm_detector: BpmDetector,
    /// Current spectrum data
    current_spectrum: SpectrumData,
    /// Pre-computed waveform overview for display - Arc to avoid cloning
    waveform_overview: Arc<Vec<f32>>,
    /// Enhanced waveform with frequency band analysis
    enhanced_waveform: Arc<EnhancedWaveform>,
    /// Cue points (up to 8), stored as sample positions
    cue_points: [Option<f64>; 8],
    /// Current peak level for metering
    peak_level: f32,
    /// Peak hold level (max peak that decays slowly)
    peak_hold: f32,
    /// Peak hold counter (samples until decay starts)
    peak_hold_samples: u32,
    /// Clipping indicator
    is_clipping: bool,
    /// Pre-allocated buffer for spectrum analysis (avoid allocation in process())
    spectrum_buffer: Vec<f32>,
    /// Ring buffer for oscilloscope display (last N stereo samples)
    /// Fixed size to avoid allocation in audio thread
    scope_buffer: Box<[f32; Self::SCOPE_BUFFER_SIZE]>,
    /// Write position in scope buffer
    scope_write_pos: usize,
}

impl Deck {
    /// Size of scope buffer (512 stereo samples = 1024 floats)
    const SCOPE_BUFFER_SIZE: usize = 1024;

    /// Create a new empty deck
    pub fn new(target_sample_rate: u32) -> Self {
        Self {
            samples: Arc::new(Vec::new()),
            sample_rate: target_sample_rate,
            position: 0.0,
            state: PlaybackState::Stopped,
            tempo: 1.0,
            pitch: 0.0,
            gain: 1.0,
            track_name: None,
            key: None,
            bpm: None,
            beat_grid: None,
            sync_transition: SyncTransition::default(),
            spectrum_analyzer: SpectrumAnalyzer::new(target_sample_rate),
            bpm_detector: BpmDetector::new(target_sample_rate),
            current_spectrum: SpectrumData::default(),
            waveform_overview: Arc::new(Vec::new()),
            enhanced_waveform: Arc::new(EnhancedWaveform::default()),
            cue_points: [None; 8],
            peak_level: 0.0,
            peak_hold: 0.0,
            peak_hold_samples: 0,
            is_clipping: false,
            // Pre-allocate buffer for spectrum analysis (4096 mono samples max)
            spectrum_buffer: Vec::with_capacity(4096),
            // Scope buffer for oscilloscope visualization
            scope_buffer: Box::new([0.0; Self::SCOPE_BUFFER_SIZE]),
            scope_write_pos: 0,
        }
    }

    /// Load audio samples into the deck
    /// Uses Arc to avoid copying large sample data
    pub fn load(
        &mut self,
        samples: Arc<Vec<f32>>,
        sample_rate: u32,
        name: Option<String>,
        waveform: Arc<Vec<f32>>,
        enhanced_waveform: Arc<EnhancedWaveform>,
        key: Option<String>,
    ) {
        self.samples = samples;
        self.sample_rate = sample_rate;
        self.position = 0.0;
        self.state = PlaybackState::Stopped;
        self.track_name = name;
        self.key = key;
        self.bpm = None;
        self.beat_grid = None;
        self.sync_transition = SyncTransition::default();
        self.bpm_detector = BpmDetector::new(sample_rate);
        self.waveform_overview = waveform;
        self.enhanced_waveform = enhanced_waveform;

        // Analyze beat grid from first 30 seconds of audio
        if !self.samples.is_empty() {
            let analyzer = BeatGridAnalyzer::new(sample_rate);
            // Analyze first 30 seconds (or full track if shorter)
            let analysis_samples = self.samples.len().min(sample_rate as usize * 60); // 30 seconds stereo

            if let Some(grid) = analyzer.analyze(&self.samples[..analysis_samples]) {
                self.bpm = Some(grid.bpm);
                self.beat_grid = Some(grid);
            } else {
                // Fallback to legacy BPM detector
                let analysis_samples = self.samples.len().min(sample_rate as usize * 10);
                for chunk in self.samples[..analysis_samples].chunks(1024) {
                    let mono: Vec<f32> = chunk
                        .chunks(2)
                        .map(|s| {
                            if s.len() == 2 {
                                (s[0] + s[1]) * 0.5
                            } else {
                                s[0]
                            }
                        })
                        .collect();
                    self.bpm_detector.process(&mono);
                }
                self.bpm = self.bpm_detector.bpm();
            }
        }
    }

    /// Check if deck has a track loaded
    pub fn is_loaded(&self) -> bool {
        !self.samples.is_empty()
    }

    /// Start playback
    pub fn play(&mut self) {
        if self.is_loaded() {
            self.state = PlaybackState::Playing;
        }
    }

    /// Pause playback
    pub fn pause(&mut self) {
        self.state = PlaybackState::Paused;
    }

    /// Stop playback and reset position
    pub fn stop(&mut self) {
        self.state = PlaybackState::Stopped;
        self.position = 0.0;
    }

    /// Toggle play/pause
    pub fn toggle(&mut self) {
        match self.state {
            PlaybackState::Playing => self.pause(),
            PlaybackState::Paused | PlaybackState::Stopped => self.play(),
        }
    }

    /// Set playback position in seconds
    pub fn seek(&mut self, position_secs: f64) {
        let max_pos = self.duration();
        self.position = (position_secs * self.sample_rate as f64 * 2.0)
            .clamp(0.0, max_pos * self.sample_rate as f64 * 2.0);
    }

    /// Nudge position forward/backward by given seconds
    pub fn nudge(&mut self, delta_secs: f64) {
        let current_secs = self.position / (self.sample_rate as f64 * 2.0);
        self.seek(current_secs + delta_secs);
    }

    /// Jump by N beats (positive = forward, negative = backward)
    pub fn beatjump(&mut self, beats: i32) {
        if let Some(grid) = &self.beat_grid {
            let samples_per_beat = grid.samples_per_beat_at_tempo(self.tempo);
            let jump_samples = beats as f64 * samples_per_beat;
            let new_pos = (self.position + jump_samples).clamp(0.0, self.samples.len() as f64);
            self.position = new_pos;
        } else if let Some(bpm) = self.bpm {
            // Fallback: calculate from BPM
            let beats_per_sec = bpm as f64 / 60.0;
            let samples_per_beat = (self.sample_rate as f64 * 2.0) / beats_per_sec;
            let jump_samples = beats as f64 * samples_per_beat;
            let new_pos = (self.position + jump_samples).clamp(0.0, self.samples.len() as f64);
            self.position = new_pos;
        }
    }

    /// Set cue point at current position (1-4)
    pub fn set_cue(&mut self, cue_num: u8) {
        if (1..=4).contains(&cue_num) {
            self.cue_points[(cue_num - 1) as usize] = Some(self.position);
        }
    }

    /// Jump to cue point (1-4)
    pub fn jump_cue(&mut self, cue_num: u8) {
        if (1..=4).contains(&cue_num) {
            if let Some(pos) = self.cue_points[(cue_num - 1) as usize] {
                self.position = pos;
            }
        }
    }

    /// Get cue point position (for UI display)
    pub fn get_cue(&self, cue_num: u8) -> Option<f64> {
        if (1..=4).contains(&cue_num) {
            self.cue_points[(cue_num - 1) as usize]
        } else {
            None
        }
    }

    /// Set tempo (playback speed)
    pub fn set_tempo(&mut self, tempo: f32) {
        self.tempo = tempo.clamp(0.5, 2.0);
    }

    /// Adjust tempo by delta
    pub fn adjust_tempo(&mut self, delta: f32) {
        self.set_tempo(self.tempo + delta);
    }

    /// Set gain
    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain.clamp(0.0, 2.0);
    }

    /// Adjust gain by delta
    pub fn adjust_gain(&mut self, delta: f32) {
        self.set_gain(self.gain + delta);
    }

    /// Get track duration in seconds
    pub fn duration(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.samples.len() as f64 / (self.sample_rate as f64 * 2.0) // stereo
    }

    /// Get current position in seconds
    pub fn position_secs(&self) -> f64 {
        self.position / (self.sample_rate as f64 * 2.0)
    }

    /// Get current BPM (adjusted for tempo)
    pub fn current_bpm(&self) -> Option<f32> {
        self.bpm.map(|b| b * self.tempo)
    }

    /// Get beat grid reference
    pub fn beat_grid(&self) -> Option<&BeatGrid> {
        self.beat_grid.as_ref()
    }

    /// Calculate current beat phase (0.0 - 1.0), accounting for tempo
    pub fn beat_phase(&self) -> Option<f32> {
        let grid = self.beat_grid.as_ref()?;

        // Get samples per beat adjusted for current tempo
        let samples_per_beat = grid.samples_per_beat_at_tempo(self.tempo);

        // Calculate phase
        let position_from_first_beat = self.position - grid.first_beat_offset as f64;
        let beat_position = position_from_first_beat / samples_per_beat;

        Some(beat_position.fract().abs() as f32)
    }

    /// Get current beat number (which beat we're on in the track)
    pub fn current_beat_number(&self) -> Option<u32> {
        let grid = self.beat_grid.as_ref()?;
        let samples_per_beat = grid.samples_per_beat_at_tempo(self.tempo);
        let position_from_first_beat = self.position - grid.first_beat_offset as f64;

        if position_from_first_beat < 0.0 {
            return Some(0);
        }

        Some((position_from_first_beat / samples_per_beat).floor() as u32)
    }

    /// Calculate position offset needed to align phase with target
    /// Returns the number of samples to nudge (positive = forward, negative = backward)
    pub fn phase_offset_to_align(&self, target_phase: f32) -> Option<f64> {
        let grid = self.beat_grid.as_ref()?;
        let current_phase = self.beat_phase()?;

        // Calculate shortest path to align (can go forward or backward)
        let mut phase_diff = target_phase - current_phase;

        // Normalize to -0.5 to 0.5 (shortest path to alignment)
        if phase_diff > 0.5 {
            phase_diff -= 1.0;
        } else if phase_diff < -0.5 {
            phase_diff += 1.0;
        }

        // Convert phase difference to samples
        let samples_per_beat = grid.samples_per_beat_at_tempo(self.tempo);
        Some(phase_diff as f64 * samples_per_beat)
    }

    /// Nudge position by a given number of samples
    pub fn nudge_samples(&mut self, samples: f64) {
        let new_pos = self.position + samples;
        let max_pos = self.samples.len() as f64;
        self.position = new_pos.clamp(0.0, max_pos);
    }

    /// Start a smooth sync transition
    pub fn start_sync_transition(
        &mut self,
        target_tempo: f32,
        phase_offset: f64,
        duration_samples: u64,
    ) {
        self.sync_transition = SyncTransition {
            target_tempo,
            start_tempo: self.tempo,
            target_phase_offset: phase_offset,
            applied_phase_offset: 0.0,
            progress: 0.0,
            duration_samples,
            samples_processed: 0,
            active: true,
        };
    }

    /// Check if sync transition is in progress
    pub fn is_syncing(&self) -> bool {
        self.sync_transition.active
    }

    /// Get deck state for UI
    pub fn state(&self) -> DeckState {
        let beat_grid_info = self.beat_grid.as_ref().map(|g| {
            // Convert first beat offset from samples to seconds
            let sample_rate_stereo = self.sample_rate as f64 * 2.0;
            let first_beat_offset_secs = g.first_beat_offset as f64 / sample_rate_stereo;
            BeatGridInfo {
                bpm: g.bpm,
                confidence: g.confidence,
                has_grid: true,
                first_beat_offset_secs,
            }
        });

        // Convert cue points from sample positions to seconds
        let sample_rate_stereo = self.sample_rate as f64 * 2.0;
        let cue_points = self
            .cue_points
            .map(|opt| opt.map(|pos| pos / sample_rate_stereo));

        // Copy scope buffer for oscilloscope display
        // We read from the ring buffer in order, starting from write position
        let mut scope_samples = Box::new([0.0f32; SCOPE_SAMPLES_SIZE * 2]);
        for i in 0..Self::SCOPE_BUFFER_SIZE {
            let src_idx = (self.scope_write_pos + i) % Self::SCOPE_BUFFER_SIZE;
            scope_samples[i] = self.scope_buffer[src_idx];
        }

        DeckState {
            playback: self.state,
            position: self.position_secs(),
            duration: self.duration(),
            tempo: self.tempo,
            pitch: self.pitch,
            gain: self.gain,
            bpm: self.current_bpm(),
            key: self.key.clone(),
            track_name: self.track_name.clone(),
            spectrum: self.current_spectrum,
            beat_phase: self.beat_phase().unwrap_or(0.0),
            beat_grid_info,
            waveform_overview: self.waveform_overview.clone(),
            enhanced_waveform: self.enhanced_waveform.clone(),
            peak_level: self.peak_level,
            peak_hold: self.peak_hold,
            is_clipping: self.is_clipping,
            cue_points,
            scope_samples,
        }
    }

    /// Process and return audio samples for output buffer
    /// Returns stereo interleaved samples
    pub fn process(&mut self, output: &mut [f32]) {
        if self.state != PlaybackState::Playing || self.samples.is_empty() {
            // Fill with silence
            for sample in output.iter_mut() {
                *sample = 0.0;
            }
            return;
        }

        // Update sync transition if active
        self.update_sync_transition(output.len() as u64);

        let sample_count = self.samples.len();

        // Reuse pre-allocated buffer for spectrum analysis
        self.spectrum_buffer.clear();

        // Track peak during sample generation to avoid second iteration
        let mut current_peak = 0.0f32;

        for frame in output.chunks_mut(2) {
            let pos = self.position as usize;

            if pos + 1 >= sample_count {
                // End of track
                self.state = PlaybackState::Stopped;
                self.position = 0.0;
                frame[0] = 0.0;
                frame[1] = 0.0;
                continue;
            }

            // Linear interpolation for smoother playback at non-integer positions
            let frac = self.position.fract() as f32;
            let pos_even = pos & !1; // Ensure we start at left channel

            if pos_even + 3 < sample_count {
                let l0 = self.samples[pos_even];
                let r0 = self.samples[pos_even + 1];
                let l1 = self.samples[pos_even + 2];
                let r1 = self.samples[pos_even + 3];

                frame[0] = (l0 + frac * (l1 - l0)) * self.gain;
                frame[1] = (r0 + frac * (r1 - r0)) * self.gain;
            } else {
                frame[0] = self.samples[pos_even] * self.gain;
                frame[1] = self.samples[pos_even + 1] * self.gain;
            }

            // Track peak level inline (avoid separate iteration)
            current_peak = current_peak.max(frame[0].abs()).max(frame[1].abs());

            // Collect mono samples for spectrum analysis
            self.spectrum_buffer.push((frame[0] + frame[1]) * 0.5);

            // Advance position based on tempo
            self.position += 2.0 * self.tempo as f64;
        }

        // Update spectrum
        if !self.spectrum_buffer.is_empty() {
            self.current_spectrum = self.spectrum_analyzer.process(&self.spectrum_buffer);
        }

        // Update scope buffer for oscilloscope display
        // Copy the processed output samples to the ring buffer
        for &sample in output.iter() {
            self.scope_buffer[self.scope_write_pos] = sample;
            self.scope_write_pos = (self.scope_write_pos + 1) % Self::SCOPE_BUFFER_SIZE;
        }

        // Track peak level with slow decay (current_peak already computed above)
        self.peak_level = self.peak_level * 0.95 + current_peak * 0.05; // Smooth decay
        self.is_clipping = current_peak > 0.99;

        // Peak hold: hold for ~1 second at 44.1kHz, then decay
        const HOLD_SAMPLES: u32 = 44100;
        const DECAY_RATE: f32 = 0.995;

        if current_peak > self.peak_hold {
            self.peak_hold = current_peak;
            self.peak_hold_samples = HOLD_SAMPLES;
        } else if self.peak_hold_samples > 0 {
            self.peak_hold_samples = self
                .peak_hold_samples
                .saturating_sub(output.len() as u32 / 2);
        } else {
            self.peak_hold *= DECAY_RATE;
        }
    }

    /// Update sync transition state (called from process())
    fn update_sync_transition(&mut self, samples_in_buffer: u64) {
        if !self.sync_transition.active {
            return;
        }

        self.sync_transition.samples_processed += samples_in_buffer;
        self.sync_transition.progress = (self.sync_transition.samples_processed as f32
            / self.sync_transition.duration_samples as f32)
            .min(1.0);

        // Smooth easing function (ease-in-out quadratic)
        let t = self.sync_transition.progress;
        let eased = if t < 0.5 {
            2.0 * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
        };

        // Interpolate tempo smoothly
        self.tempo = self.sync_transition.start_tempo
            + (self.sync_transition.target_tempo - self.sync_transition.start_tempo) * eased;

        // Apply phase offset gradually
        let target_offset = self.sync_transition.target_phase_offset;
        let offset_to_apply =
            target_offset * eased as f64 - self.sync_transition.applied_phase_offset;
        self.position += offset_to_apply;
        self.sync_transition.applied_phase_offset += offset_to_apply;

        // Clamp position to valid range
        let max_pos = self.samples.len() as f64;
        self.position = self.position.clamp(0.0, max_pos);

        // Complete transition
        if self.sync_transition.progress >= 1.0 {
            self.tempo = self.sync_transition.target_tempo;
            self.sync_transition.active = false;
        }
    }
}

impl Default for Deck {
    fn default() -> Self {
        Self::new(44100)
    }
}
