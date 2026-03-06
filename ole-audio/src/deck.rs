//! Deck implementation - track playback with pitch/tempo control

use ole_analysis::{
    BeatGrid, BeatGridAnalyzer, BpmDetector, EnhancedWaveform, PhraseMarker, SpectrumAnalyzer,
    SpectrumData,
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

/// Loop state for a deck
#[derive(Debug, Clone, Copy, Default)]
pub struct LoopState {
    /// Loop start position in samples
    pub loop_in: Option<f64>,
    /// Loop end position in samples
    pub loop_out: Option<f64>,
    /// Whether the loop is currently active
    pub active: bool,
    /// Saved position for loop roll (returns here when loop roll ends)
    pub roll_return_position: Option<f64>,
}

/// Quantize resolution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QuantizeResolution {
    #[default]
    OneBeat,
    HalfBeat,
    QuarterBeat,
}

impl QuantizeResolution {
    pub fn beat_fraction(self) -> f64 {
        match self {
            Self::OneBeat => 1.0,
            Self::HalfBeat => 0.5,
            Self::QuarterBeat => 0.25,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::OneBeat => "1",
            Self::HalfBeat => "1/2",
            Self::QuarterBeat => "1/4",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::OneBeat => Self::HalfBeat,
            Self::HalfBeat => Self::QuarterBeat,
            Self::QuarterBeat => Self::OneBeat,
        }
    }
}

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
    // Loop state
    pub loop_in: Option<f64>,   // loop start in seconds
    pub loop_out: Option<f64>,  // loop end in seconds
    pub loop_active: bool,
    // Quantize
    pub quantize_enabled: bool,
    pub quantize_resolution: QuantizeResolution,
    // Key lock
    pub key_lock: bool,
    // Slip mode
    pub slip_enabled: bool,
    pub slip_position: Option<f64>, // shadow position in seconds (None = not slipping)
    // Phrase intelligence
    pub energy_curve: Arc<Vec<f32>>,
    pub phrase_markers: Arc<Vec<PhraseMarker>>,
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
            loop_in: None,
            loop_out: None,
            loop_active: false,
            quantize_enabled: false,
            quantize_resolution: QuantizeResolution::default(),
            key_lock: false,
            slip_enabled: false,
            slip_position: None,
            energy_curve: Arc::new(Vec::new()),
            phrase_markers: Arc::new(Vec::new()),
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
    /// Fade-in envelope samples remaining (after seek/nudge to prevent clicks)
    fade_in_samples: u32,
    /// Fade-out envelope samples remaining (for smooth pause/stop)
    fade_out_samples: u32,
    /// Pending state to transition to after fade-out completes
    pending_state: Option<PlaybackState>,
    /// Smoothed gain for click-free volume changes
    smoothed_gain: f32,
    /// Loop state
    loop_state: LoopState,
    /// Quantize mode
    quantize_enabled: bool,
    quantize_resolution: QuantizeResolution,
    /// Key lock (pitch-independent tempo)
    key_lock: bool,
    /// Slip mode: shadow position continues advancing during loops/cue jumps
    slip_enabled: bool,
    /// Shadow position in samples (where playback would be without slip actions)
    slip_position: Option<f64>,
    /// Energy curve for phrase visualization
    energy_curve: Arc<Vec<f32>>,
    /// Phrase boundary markers
    phrase_markers: Arc<Vec<PhraseMarker>>,
}

impl Deck {
    /// Size of scope buffer (512 stereo samples = 1024 floats)
    const SCOPE_BUFFER_SIZE: usize = 1024;

    /// Fade-in duration in samples (~20ms at 48kHz) to prevent clicks after seek/nudge
    /// 20ms is safer for live performance than 10ms
    const FADE_IN_SAMPLES: u32 = 960;

    /// Fade-out duration in samples (~20ms at 48kHz) to prevent clicks on pause/stop
    const FADE_OUT_SAMPLES: u32 = 960;

    /// Gain smoothing coefficient (higher = slower smoothing, ~0.999 = 20ms time constant)
    const GAIN_SMOOTH_COEFF: f32 = 0.995;

    /// S-curve fade for perceptually smooth transitions (zero slope at endpoints)
    /// Input: linear 0.0-1.0, Output: S-curved 0.0-1.0
    #[inline(always)]
    fn s_curve(t: f32) -> f32 {
        // Attempt to use the branch-free multiply-add form
        // Smoothstep: 3t² - 2t³ = t² * (3 - 2t)
        t * t * (3.0 - 2.0 * t)
    }

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
            // Fade-in to prevent clicks after seek/nudge
            fade_in_samples: 0,
            // Fade-out for smooth pause/stop
            fade_out_samples: 0,
            pending_state: None,
            // Smoothed gain starts at target
            smoothed_gain: 1.0,
            // Loop state
            loop_state: LoopState::default(),
            // Quantize
            quantize_enabled: false,
            quantize_resolution: QuantizeResolution::default(),
            // Key lock
            key_lock: false,
            // Slip mode
            slip_enabled: false,
            slip_position: None,
            // Phrase data
            energy_curve: Arc::new(Vec::new()),
            phrase_markers: Arc::new(Vec::new()),
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
        self.loop_state = LoopState::default();
        self.slip_position = None;

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
            // Cancel any pending fade-out
            self.fade_out_samples = 0;
            self.pending_state = None;
            // Trigger fade-in for smooth start
            self.fade_in_samples = Self::FADE_IN_SAMPLES;
            self.state = PlaybackState::Playing;
        }
    }

    /// Pause playback (with fade-out to prevent clicks)
    pub fn pause(&mut self) {
        if self.state == PlaybackState::Playing {
            // Start fade-out, defer actual pause until fade completes
            self.fade_out_samples = Self::FADE_OUT_SAMPLES;
            self.pending_state = Some(PlaybackState::Paused);
        } else {
            self.state = PlaybackState::Paused;
        }
    }

    /// Stop playback and reset position (with fade-out to prevent clicks)
    pub fn stop(&mut self) {
        if self.state == PlaybackState::Playing || self.fade_out_samples > 0 {
            // Start or continue fade-out, but change destination to Stopped
            if self.fade_out_samples == 0 {
                self.fade_out_samples = Self::FADE_OUT_SAMPLES;
            }
            self.pending_state = Some(PlaybackState::Stopped);
        } else {
            self.state = PlaybackState::Stopped;
            self.position = 0.0;
        }
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
        // Trigger fade-in to prevent click at new position
        self.fade_in_samples = Self::FADE_IN_SAMPLES;
    }

    /// Nudge position forward/backward by given seconds
    pub fn nudge(&mut self, delta_secs: f64) {
        let current_secs = self.position / (self.sample_rate as f64 * 2.0);
        self.seek(current_secs + delta_secs);
    }

    /// Nudge by fraction of a beat (e.g., 0.0625 = 1/16 beat)
    /// More musical than time-based nudge for beat alignment
    pub fn beat_nudge(&mut self, beat_fraction: f32) {
        if let Some(grid) = &self.beat_grid {
            let samples_per_beat = grid.samples_per_beat_at_tempo(self.tempo);
            let nudge_samples = beat_fraction as f64 * samples_per_beat;
            let new_pos = (self.position + nudge_samples).clamp(0.0, self.samples.len() as f64);
            self.position = new_pos;
            // Trigger fade-in to prevent click
            self.fade_in_samples = Self::FADE_IN_SAMPLES;
        } else if let Some(bpm) = self.bpm {
            // Fallback: calculate from BPM
            let beats_per_sec = bpm as f64 / 60.0;
            let samples_per_beat = (self.sample_rate as f64 * 2.0) / beats_per_sec;
            let nudge_samples = beat_fraction as f64 * samples_per_beat;
            let new_pos = (self.position + nudge_samples).clamp(0.0, self.samples.len() as f64);
            self.position = new_pos;
            // Trigger fade-in to prevent click
            self.fade_in_samples = Self::FADE_IN_SAMPLES;
        }
    }

    /// Jump by N beats (positive = forward, negative = backward)
    pub fn beatjump(&mut self, beats: i32) {
        if let Some(grid) = &self.beat_grid {
            let samples_per_beat = grid.samples_per_beat_at_tempo(self.tempo);
            let jump_samples = beats as f64 * samples_per_beat;
            let new_pos = (self.position + jump_samples).clamp(0.0, self.samples.len() as f64);
            self.position = new_pos;
            // Trigger fade-in to prevent click
            self.fade_in_samples = Self::FADE_IN_SAMPLES;
        } else if let Some(bpm) = self.bpm {
            // Fallback: calculate from BPM
            let beats_per_sec = bpm as f64 / 60.0;
            let samples_per_beat = (self.sample_rate as f64 * 2.0) / beats_per_sec;
            let jump_samples = beats as f64 * samples_per_beat;
            let new_pos = (self.position + jump_samples).clamp(0.0, self.samples.len() as f64);
            self.position = new_pos;
            // Trigger fade-in to prevent click
            self.fade_in_samples = Self::FADE_IN_SAMPLES;
        }
    }

    /// Set cue point at current position (1-8)
    pub fn set_cue(&mut self, cue_num: u8) {
        if (1..=8).contains(&cue_num) {
            let pos = self.maybe_quantize_position(self.position);
            self.cue_points[(cue_num - 1) as usize] = Some(pos);
        }
    }

    /// Jump to cue point (1-8)
    pub fn jump_cue(&mut self, cue_num: u8) {
        if (1..=8).contains(&cue_num) {
            if let Some(pos) = self.cue_points[(cue_num - 1) as usize] {
                // Save slip position before jumping
                if self.slip_enabled && self.slip_position.is_none() {
                    self.slip_position = Some(self.position);
                }
                self.position = pos;
                self.fade_in_samples = Self::FADE_IN_SAMPLES;
            }
        }
    }

    /// Get cue point position (for UI display)
    pub fn get_cue(&self, cue_num: u8) -> Option<f64> {
        if (1..=8).contains(&cue_num) {
            self.cue_points[(cue_num - 1) as usize]
        } else {
            None
        }
    }

    // --- Loop methods ---

    /// Set loop-in point at current position
    pub fn set_loop_in(&mut self) {
        let pos = self.maybe_quantize_position(self.position);
        self.loop_state.loop_in = Some(pos);
    }

    /// Set loop-out point at current position and activate loop
    pub fn set_loop_out(&mut self) {
        let pos = self.maybe_quantize_position(self.position);
        self.loop_state.loop_out = Some(pos);
        // Auto-activate loop if both points are set
        if self.loop_state.loop_in.is_some() {
            self.loop_state.active = true;
        }
    }

    /// Toggle loop on/off
    pub fn toggle_loop(&mut self) {
        if self.loop_state.loop_in.is_some() && self.loop_state.loop_out.is_some() {
            self.loop_state.active = !self.loop_state.active;
        }
    }

    /// Clear the loop
    pub fn clear_loop(&mut self) {
        self.loop_state.active = false;
        self.loop_state.loop_in = None;
        self.loop_state.loop_out = None;
    }

    /// Create an auto-loop of the given number of beats at the current position
    pub fn auto_loop(&mut self, beats: f32) {
        let samples_per_beat = self.samples_per_beat();
        if samples_per_beat <= 0.0 {
            return;
        }
        let loop_in = self.maybe_quantize_position(self.position);
        let loop_length = beats as f64 * samples_per_beat;
        let loop_out = (loop_in + loop_length).min(self.samples.len() as f64);
        self.loop_state.loop_in = Some(loop_in);
        self.loop_state.loop_out = Some(loop_out);
        self.loop_state.active = true;
    }

    /// Halve the loop length (keep loop_in, move loop_out to midpoint)
    pub fn loop_halve(&mut self) {
        if let (Some(loop_in), Some(loop_out)) = (self.loop_state.loop_in, self.loop_state.loop_out) {
            let length = loop_out - loop_in;
            if length > 256.0 { // Minimum loop size ~5ms at 48kHz
                self.loop_state.loop_out = Some(loop_in + length / 2.0);
            }
        }
    }

    /// Double the loop length (keep loop_in, extend loop_out)
    pub fn loop_double(&mut self) {
        if let (Some(loop_in), Some(loop_out)) = (self.loop_state.loop_in, self.loop_state.loop_out) {
            let length = loop_out - loop_in;
            let new_out = (loop_in + length * 2.0).min(self.samples.len() as f64);
            self.loop_state.loop_out = Some(new_out);
        }
    }

    /// Start a loop roll: auto-loop N beats but remember the original position
    pub fn start_loop_roll(&mut self, beats: f32) {
        // Save current position for return
        self.loop_state.roll_return_position = Some(self.position);
        self.auto_loop(beats);
    }

    /// End a loop roll: deactivate loop and return to the shadow position
    pub fn end_loop_roll(&mut self) {
        if let Some(return_pos) = self.loop_state.roll_return_position.take() {
            self.loop_state.active = false;
            // Calculate where playback would have been
            self.position = return_pos;
            self.fade_in_samples = Self::FADE_IN_SAMPLES;
        } else {
            self.loop_state.active = false;
        }
    }

    /// Get samples per beat (using beat grid or BPM fallback)
    fn samples_per_beat(&self) -> f64 {
        if let Some(grid) = &self.beat_grid {
            grid.samples_per_beat_at_tempo(self.tempo)
        } else if let Some(bpm) = self.bpm {
            let beats_per_sec = bpm as f64 * self.tempo as f64 / 60.0;
            if beats_per_sec > 0.0 {
                (self.sample_rate as f64 * 2.0) / beats_per_sec
            } else {
                0.0
            }
        } else {
            0.0
        }
    }

    // --- Quantize methods ---

    /// Enable/disable quantize mode
    pub fn set_quantize(&mut self, enabled: bool) {
        self.quantize_enabled = enabled;
    }

    /// Toggle quantize mode
    pub fn toggle_quantize(&mut self) {
        self.quantize_enabled = !self.quantize_enabled;
    }

    /// Set quantize resolution
    pub fn set_quantize_resolution(&mut self, resolution: QuantizeResolution) {
        self.quantize_resolution = resolution;
    }

    /// Cycle quantize resolution
    pub fn cycle_quantize_resolution(&mut self) {
        self.quantize_resolution = self.quantize_resolution.next();
    }

    /// Snap a position to the nearest beat grid point if quantize is enabled
    fn maybe_quantize_position(&self, pos: f64) -> f64 {
        if !self.quantize_enabled {
            return pos;
        }
        self.snap_to_grid(pos)
    }

    /// Snap a sample position to the nearest beat grid line
    fn snap_to_grid(&self, pos: f64) -> f64 {
        let spb = self.samples_per_beat();
        if spb <= 0.0 {
            return pos;
        }
        let resolution_samples = spb * self.quantize_resolution.beat_fraction();
        let grid_offset = self.beat_grid.as_ref().map(|g| g.first_beat_offset as f64).unwrap_or(0.0);
        let relative = pos - grid_offset;
        let beats = (relative / resolution_samples).round();
        (grid_offset + beats * resolution_samples).clamp(0.0, self.samples.len() as f64)
    }

    // --- Key lock methods ---

    /// Set key lock
    pub fn set_key_lock(&mut self, enabled: bool) {
        self.key_lock = enabled;
    }

    /// Toggle key lock
    pub fn toggle_key_lock(&mut self) {
        self.key_lock = !self.key_lock;
    }

    /// Get key lock state
    pub fn is_key_locked(&self) -> bool {
        self.key_lock
    }

    // --- Slip mode methods ---

    /// Set slip mode
    pub fn set_slip(&mut self, enabled: bool) {
        self.slip_enabled = enabled;
        if !enabled {
            // Jump to shadow position if we have one
            if let Some(shadow_pos) = self.slip_position.take() {
                self.position = shadow_pos;
                self.fade_in_samples = Self::FADE_IN_SAMPLES;
            }
        }
    }

    /// Set phrase intelligence data (energy curve + phrase markers)
    pub fn set_phrase_data(
        &mut self,
        energy_curve: Arc<Vec<f32>>,
        phrase_markers: Arc<Vec<PhraseMarker>>,
    ) {
        self.energy_curve = energy_curve;
        self.phrase_markers = phrase_markers;
    }

    /// Toggle slip mode
    pub fn toggle_slip(&mut self) {
        let new_state = !self.slip_enabled;
        self.set_slip(new_state);
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
        // Trigger fade-in to prevent click
        self.fade_in_samples = Self::FADE_IN_SAMPLES;
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

        // Convert loop positions from samples to seconds
        let loop_in_secs = self.loop_state.loop_in.map(|pos| pos / sample_rate_stereo);
        let loop_out_secs = self.loop_state.loop_out.map(|pos| pos / sample_rate_stereo);
        let slip_pos_secs = self.slip_position.map(|pos| pos / sample_rate_stereo);

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
            loop_in: loop_in_secs,
            loop_out: loop_out_secs,
            loop_active: self.loop_state.active,
            quantize_enabled: self.quantize_enabled,
            quantize_resolution: self.quantize_resolution,
            key_lock: self.key_lock,
            slip_enabled: self.slip_enabled,
            slip_position: slip_pos_secs,
            energy_curve: self.energy_curve.clone(),
            phrase_markers: self.phrase_markers.clone(),
        }
    }

    /// Process and return audio samples for output buffer
    /// Returns stereo interleaved samples
    pub fn process(&mut self, output: &mut [f32]) {
        // Check if we should output audio:
        // - Playing state: normal playback
        // - Fading out: continue playing during fade to prevent clicks
        let is_fading_out = self.fade_out_samples > 0;
        if (self.state != PlaybackState::Playing && !is_fading_out) || self.samples.is_empty() {
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

            // Smooth gain to prevent clicks during volume changes
            self.smoothed_gain = Self::GAIN_SMOOTH_COEFF * self.smoothed_gain
                + (1.0 - Self::GAIN_SMOOTH_COEFF) * self.gain;

            // Calculate fade envelope (handles both fade-in and fade-out)
            // Uses S-curve for perceptually smooth transitions
            let fade_envelope = if self.fade_out_samples > 0 {
                // Fade-out: 1.0 -> 0.0 with S-curve
                let linear = self.fade_out_samples as f32 / Self::FADE_OUT_SAMPLES as f32;
                self.fade_out_samples -= 1;

                // Check if fade-out completed
                if self.fade_out_samples == 0 {
                    if let Some(pending) = self.pending_state.take() {
                        self.state = pending;
                        if pending == PlaybackState::Stopped {
                            self.position = 0.0;
                        }
                    }
                }
                Self::s_curve(linear)
            } else if self.fade_in_samples > 0 {
                // Fade-in: 0.0 -> 1.0 with S-curve
                let linear = 1.0 - (self.fade_in_samples as f32 / Self::FADE_IN_SAMPLES as f32);
                self.fade_in_samples -= 1;
                Self::s_curve(linear)
            } else {
                1.0
            };

            // Combined gain: smoothed gain * fade envelope
            let effective_gain = self.smoothed_gain * fade_envelope;

            if pos + 1 >= sample_count {
                // End of track - trigger fade-out if not already fading
                if self.fade_out_samples == 0 && self.pending_state.is_none() {
                    self.fade_out_samples = Self::FADE_OUT_SAMPLES;
                    self.pending_state = Some(PlaybackState::Stopped);
                }
                // Output silence (fade envelope already calculated above handles transition)
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

                frame[0] = (l0 + frac * (l1 - l0)) * effective_gain;
                frame[1] = (r0 + frac * (r1 - r0)) * effective_gain;
            } else {
                frame[0] = self.samples[pos_even] * effective_gain;
                frame[1] = self.samples[pos_even + 1] * effective_gain;
            }

            // Track peak level inline (avoid separate iteration)
            current_peak = current_peak.max(frame[0].abs()).max(frame[1].abs());

            // Collect mono samples for spectrum analysis
            self.spectrum_buffer.push((frame[0] + frame[1]) * 0.5);

            // Advance position based on tempo
            self.position += 2.0 * self.tempo as f64;

            // Update slip shadow position (advances regardless of loops)
            if self.slip_enabled {
                if let Some(ref mut shadow) = self.slip_position {
                    *shadow += 2.0 * self.tempo as f64;
                }
            }

            // Loop boundary: wrap position back to loop_in when reaching loop_out
            if self.loop_state.active {
                if let (Some(loop_in), Some(loop_out)) = (self.loop_state.loop_in, self.loop_state.loop_out) {
                    if loop_out > loop_in && self.position >= loop_out {
                        self.position = loop_in + (self.position - loop_out) % (loop_out - loop_in);
                    }
                }
            }
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
