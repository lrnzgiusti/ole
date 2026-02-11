use ole_audio::{AudioEvent, DeckState, DelayModulation, FilterMode, FilterType, LufsValues, MasteringPreset, VinylPreset};
use ole_library::CachedAnalysis;
use ole_analysis::CamelotKey;

pub const SPECTRUM_BANDS: usize = 32;
pub const AFTERGLOW_HISTORY: usize = 15;
pub const WATERFALL_DEPTH: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusedPane {
    #[default]
    DeckA,
    DeckB,
    Crossfader,
    Effects,
    Library,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MessageType {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaveformZoom {
    #[default]
    X1,
    X2,
    X4,
    X8,
}

impl WaveformZoom {
    pub fn zoom_in(self) -> Self {
        match self {
            Self::X1 => Self::X2,
            Self::X2 => Self::X4,
            Self::X4 => Self::X8,
            Self::X8 => Self::X8,
        }
    }
    pub fn zoom_out(self) -> Self {
        match self {
            Self::X1 => Self::X1,
            Self::X2 => Self::X1,
            Self::X4 => Self::X2,
            Self::X8 => Self::X4,
        }
    }
    pub fn viewport_fraction(self) -> f64 {
        match self {
            Self::X1 => 1.0,
            Self::X2 => 0.5,
            Self::X4 => 0.25,
            Self::X8 => 0.125,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::X1 => "1x",
            Self::X2 => "2x",
            Self::X4 => "4x",
            Self::X8 => "8x",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScopeMode {
    #[default]
    TimeDomain,
    Lissajous,
    StereoField,
    Waterfall,
}

#[derive(Debug, Clone, Default)]
pub struct LibraryState {
    pub tracks: Vec<CachedAnalysis>,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub filter_key: Option<String>,
    pub current_playing_key: Option<String>,
    pub is_scanning: bool,
    pub scan_progress: (usize, usize),
    pub search_query: String,
    /// Set to true when selection changes; consumed by library widget to scroll once
    pub needs_scroll: bool,
}

impl LibraryState {
    pub fn set_tracks(&mut self, tracks: Vec<CachedAnalysis>) {
        self.tracks = tracks;
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.needs_scroll = true;
    }

    pub fn select_next(&mut self) {
        let count = self.filtered_tracks().len();
        if count > 0 && self.selected_index < count - 1 {
            self.selected_index += 1;
            self.needs_scroll = true;
        }
    }

    pub fn select_prev(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.needs_scroll = true;
        }
    }

    pub fn select_first(&mut self) {
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.needs_scroll = true;
    }

    pub fn select_last(&mut self) {
        let count = self.filtered_tracks().len();
        if count > 0 {
            self.selected_index = count - 1;
            self.needs_scroll = true;
        }
    }

    pub fn filtered_tracks(&self) -> Vec<&CachedAnalysis> {
        self.tracks
            .iter()
            .filter(|t| {
                // Key filter
                if let Some(ref filter) = self.filter_key {
                    if !t.key.as_ref().map(|k| k == filter).unwrap_or(false) {
                        return false;
                    }
                }
                // Search filter
                if !self.search_query.is_empty() {
                    let q = self.search_query.to_lowercase();
                    if !t.title.to_lowercase().contains(&q)
                        && !t.artist.to_lowercase().contains(&q)
                    {
                        return false;
                    }
                }
                true
            })
            .collect()
    }

    pub fn selected_track(&self) -> Option<&CachedAnalysis> {
        self.filtered_tracks().get(self.selected_index).copied()
    }

    pub fn set_filter(&mut self, key: Option<String>) {
        self.filter_key = key;
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.needs_scroll = true;
    }

    pub fn filter_compatible(&mut self) {
        if let Some(ref current_key) = self.current_playing_key {
            if let Some(camelot) = CamelotKey::parse(current_key) {
                let compatible: Vec<String> =
                    camelot.compatible_keys().iter().map(|k| k.to_string()).collect();
                if !compatible.is_empty() {
                    self.filter_key = Some(compatible[0].clone());
                    self.selected_index = 0;
                    self.scroll_offset = 0;
                    self.needs_scroll = true;
                }
            }
        }
    }

    pub fn jump_to_key(&mut self, position: u8, is_minor: bool) -> bool {
        let key_str = format!("{}{}", position, if is_minor { 'A' } else { 'B' });
        self.filter_key = None;
        for (i, track) in self.tracks.iter().enumerate() {
            if track.key.as_ref().map(|k| k == &key_str).unwrap_or(false) {
                self.selected_index = i;
                self.scroll_offset = i.saturating_sub(5);
                self.needs_scroll = true;
                return true;
            }
        }
        false
    }

    pub fn jump_to_bpm(&mut self, target_bpm: u16) -> bool {
        self.filter_key = None;
        let target = target_bpm as f32;
        for (i, track) in self.tracks.iter().enumerate() {
            if let Some(bpm) = track.bpm {
                if (bpm - target).abs() <= 3.0 {
                    self.selected_index = i;
                    self.scroll_offset = i.saturating_sub(5);
                    self.needs_scroll = true;
                    return true;
                }
            }
        }
        let mut closest_idx = 0;
        let mut closest_diff = f32::MAX;
        for (i, track) in self.tracks.iter().enumerate() {
            if let Some(bpm) = track.bpm {
                let diff = (bpm - target).abs();
                if diff < closest_diff {
                    closest_diff = diff;
                    closest_idx = i;
                }
            }
        }
        if closest_diff < f32::MAX {
            self.selected_index = closest_idx;
            self.scroll_offset = closest_idx.saturating_sub(5);
            self.needs_scroll = true;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EnergyParticle {
    /// Position along the bridge (0.0 = deck A, 1.0 = deck B)
    pub pos: f32,
    /// Vertical offset for wave motion
    pub wave_offset: f32,
    /// Brightness (0.0-1.0)
    pub brightness: f32,
    /// Speed (pixels per frame equivalent)
    pub speed: f32,
    /// Size
    pub size: f32,
}

pub struct GuiState {
    // Deck states
    pub deck_a: DeckState,
    pub deck_b: DeckState,

    // Mixer
    pub crossfader: f32,
    pub master_volume: f32,

    // Filter states
    pub filter_a_enabled: bool,
    pub filter_a_cutoff: f32,
    pub filter_a_type: FilterType,
    pub filter_a_level: u8,
    pub filter_a_mode: FilterMode,
    pub filter_b_enabled: bool,
    pub filter_b_cutoff: f32,
    pub filter_b_type: FilterType,
    pub filter_b_level: u8,
    pub filter_b_mode: FilterMode,

    // Delay
    pub delay_a_enabled: bool,
    pub delay_a_level: u8,
    pub delay_a_modulation: DelayModulation,
    pub delay_b_enabled: bool,
    pub delay_b_level: u8,
    pub delay_b_modulation: DelayModulation,

    // Reverb
    pub reverb_a_enabled: bool,
    pub reverb_a_level: u8,
    pub reverb_b_enabled: bool,
    pub reverb_b_level: u8,

    // Vinyl
    pub vinyl_a_enabled: bool,
    pub vinyl_a_preset: VinylPreset,
    pub vinyl_b_enabled: bool,
    pub vinyl_b_preset: VinylPreset,

    // Time stretch
    pub time_stretch_a_enabled: bool,
    pub time_stretch_a_ratio: f32,
    pub time_stretch_b_enabled: bool,
    pub time_stretch_b_ratio: f32,

    // Mastering
    pub mastering_enabled: bool,
    pub mastering_preset: MasteringPreset,
    pub mastering_lufs: LufsValues,
    pub mastering_gain_reduction: f32,

    // UI state
    pub mode: ole_input::Mode,
    pub focused: FocusedPane,
    pub command_buffer: String,
    pub message: Option<String>,
    pub message_type: MessageType,
    pub show_help: bool,
    pub help_scroll: f32,

    // Library
    pub library: LibraryState,
    pub show_library: bool,

    // Visualization
    pub show_scope: bool,
    pub scope_mode: ScopeMode,

    // Animation
    pub frame_count: u64,
    pub beat_pulse_a: f32,
    pub beat_pulse_b: f32,
    prev_beat_phase_a: f32,
    prev_beat_phase_b: f32,

    // Waveform zoom
    pub zoom_a: WaveformZoom,
    pub zoom_b: WaveformZoom,

    // Sync quality
    pub sync_quality: f32,

    // Spectrum afterglow
    pub spectrum_history: [[f32; AFTERGLOW_HISTORY]; SPECTRUM_BANDS],
    pub spectrum_history_idx: usize,

    // VU peak hold
    pub vu_peak_a: f32,
    pub vu_peak_b: f32,
    vu_peak_hold_frames_a: u16,
    vu_peak_hold_frames_b: u16,

    // Waterfall (scrolling spectrogram)
    pub waterfall_a: Box<[[f32; SPECTRUM_BANDS]; WATERFALL_DEPTH]>,
    pub waterfall_b: Box<[[f32; SPECTRUM_BANDS]; WATERFALL_DEPTH]>,
    pub waterfall_idx: usize,

    // Energy bridge particles
    pub energy_particles: Vec<EnergyParticle>,

    // VFX
    pub scanlines_enabled: bool,
    pub glow_enabled: bool,
    pub noise_enabled: bool,
    pub chromatic_enabled: bool,
    pub crt_intensity: u8, // 0=off, 1=subtle, 2=medium, 3=heavy
    pub glitch_frames: u8,
    pub glitch_intensity: f32,

    // Message auto-clear
    pub message_frame: u64,

    // Should quit
    pub should_quit: bool,
}

impl Default for GuiState {
    fn default() -> Self {
        Self {
            deck_a: DeckState::default(),
            deck_b: DeckState::default(),
            crossfader: 0.0,
            master_volume: 1.0,
            filter_a_enabled: false,
            filter_a_cutoff: 1000.0,
            filter_a_type: FilterType::LowPass,
            filter_a_level: 0,
            filter_a_mode: FilterMode::default(),
            filter_b_enabled: false,
            filter_b_cutoff: 1000.0,
            filter_b_type: FilterType::LowPass,
            filter_b_level: 0,
            filter_b_mode: FilterMode::default(),
            delay_a_enabled: false,
            delay_a_level: 0,
            delay_a_modulation: DelayModulation::default(),
            delay_b_enabled: false,
            delay_b_level: 0,
            delay_b_modulation: DelayModulation::default(),
            reverb_a_enabled: false,
            reverb_a_level: 0,
            reverb_b_enabled: false,
            reverb_b_level: 0,
            vinyl_a_enabled: false,
            vinyl_a_preset: VinylPreset::default(),
            vinyl_b_enabled: false,
            vinyl_b_preset: VinylPreset::default(),
            time_stretch_a_enabled: false,
            time_stretch_a_ratio: 1.0,
            time_stretch_b_enabled: false,
            time_stretch_b_ratio: 1.0,
            mastering_enabled: true,
            mastering_preset: MasteringPreset::default(),
            mastering_lufs: LufsValues::default(),
            mastering_gain_reduction: 0.0,
            mode: ole_input::Mode::Normal,
            focused: FocusedPane::DeckA,
            command_buffer: String::new(),
            message: None,
            message_type: MessageType::Info,
            show_help: false,
            help_scroll: 0.0,
            library: LibraryState::default(),
            show_library: true,
            show_scope: false,
            scope_mode: ScopeMode::default(),
            frame_count: 0,
            beat_pulse_a: 0.0,
            beat_pulse_b: 0.0,
            prev_beat_phase_a: 0.0,
            prev_beat_phase_b: 0.0,
            zoom_a: WaveformZoom::default(),
            zoom_b: WaveformZoom::default(),
            sync_quality: 0.0,
            spectrum_history: [[0.0; AFTERGLOW_HISTORY]; SPECTRUM_BANDS],
            spectrum_history_idx: 0,
            vu_peak_a: 0.0,
            vu_peak_b: 0.0,
            vu_peak_hold_frames_a: 0,
            vu_peak_hold_frames_b: 0,
            waterfall_a: Box::new([[0.0; SPECTRUM_BANDS]; WATERFALL_DEPTH]),
            waterfall_b: Box::new([[0.0; SPECTRUM_BANDS]; WATERFALL_DEPTH]),
            waterfall_idx: 0,
            energy_particles: Vec::with_capacity(64),
            scanlines_enabled: true,
            glow_enabled: true,
            noise_enabled: false,
            chromatic_enabled: false,
            crt_intensity: 1,
            glitch_frames: 0,
            glitch_intensity: 0.0,
            message_frame: 0,
            should_quit: false,
        }
    }
}

impl GuiState {
    pub fn handle_audio_event(&mut self, event: AudioEvent) {
        match event {
            AudioEvent::StateUpdate {
                deck_a,
                deck_b,
                crossfader,
                master_volume,
                filter_a_enabled,
                filter_a_cutoff,
                filter_a_type,
                filter_a_level,
                filter_a_mode,
                filter_b_enabled,
                filter_b_cutoff,
                filter_b_type,
                filter_b_level,
                filter_b_mode,
                delay_a_enabled,
                delay_a_level,
                delay_a_modulation,
                delay_b_enabled,
                delay_b_level,
                delay_b_modulation,
                reverb_a_enabled,
                reverb_a_level,
                reverb_b_enabled,
                reverb_b_level,
                vinyl_a_enabled,
                vinyl_a_preset,
                vinyl_b_enabled,
                vinyl_b_preset,
                time_stretch_a_enabled,
                time_stretch_a_ratio,
                time_stretch_b_enabled,
                time_stretch_b_ratio,
                mastering_enabled,
                mastering_preset,
                mastering_lufs,
                mastering_gain_reduction,
            } => {
                self.deck_a = *deck_a;
                self.deck_b = *deck_b;
                self.crossfader = crossfader;
                self.master_volume = master_volume;
                self.filter_a_enabled = filter_a_enabled;
                self.filter_a_cutoff = filter_a_cutoff;
                self.filter_a_type = filter_a_type;
                self.filter_a_level = filter_a_level;
                self.filter_a_mode = filter_a_mode;
                self.filter_b_enabled = filter_b_enabled;
                self.filter_b_cutoff = filter_b_cutoff;
                self.filter_b_type = filter_b_type;
                self.filter_b_level = filter_b_level;
                self.filter_b_mode = filter_b_mode;
                self.delay_a_enabled = delay_a_enabled;
                self.delay_a_level = delay_a_level;
                self.delay_a_modulation = delay_a_modulation;
                self.delay_b_enabled = delay_b_enabled;
                self.delay_b_level = delay_b_level;
                self.delay_b_modulation = delay_b_modulation;
                self.reverb_a_enabled = reverb_a_enabled;
                self.reverb_a_level = reverb_a_level;
                self.reverb_b_enabled = reverb_b_enabled;
                self.reverb_b_level = reverb_b_level;
                self.vinyl_a_enabled = vinyl_a_enabled;
                self.vinyl_a_preset = vinyl_a_preset;
                self.vinyl_b_enabled = vinyl_b_enabled;
                self.vinyl_b_preset = vinyl_b_preset;
                self.time_stretch_a_enabled = time_stretch_a_enabled;
                self.time_stretch_a_ratio = time_stretch_a_ratio;
                self.time_stretch_b_enabled = time_stretch_b_enabled;
                self.time_stretch_b_ratio = time_stretch_b_ratio;
                self.mastering_enabled = mastering_enabled;
                self.mastering_preset = mastering_preset;
                self.mastering_lufs = mastering_lufs;
                self.mastering_gain_reduction = mastering_gain_reduction;
            }
            AudioEvent::TrackLoaded { deck } => {
                self.set_success(format!("Track loaded to deck {}", deck));
                self.glitch_frames = 8;
                self.glitch_intensity = 1.0;
            }
            AudioEvent::Error(msg) => {
                self.set_error(format!("Error: {}", msg));
            }
        }
    }

    pub fn set_mode(&mut self, mode: ole_input::Mode) {
        self.mode = mode;
        if mode != ole_input::Mode::Command {
            self.command_buffer.clear();
        }
    }

    pub fn set_message(&mut self, msg: impl Into<String>) {
        self.message = Some(msg.into());
        self.message_type = MessageType::Info;
        self.message_frame = self.frame_count;
    }

    pub fn set_success(&mut self, msg: impl Into<String>) {
        self.message = Some(msg.into());
        self.message_type = MessageType::Success;
        self.message_frame = self.frame_count;
    }

    pub fn set_warning(&mut self, msg: impl Into<String>) {
        self.message = Some(msg.into());
        self.message_type = MessageType::Warning;
        self.message_frame = self.frame_count;
    }

    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.message = Some(msg.into());
        self.message_type = MessageType::Error;
        self.message_frame = self.frame_count;
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
        if self.show_help {
            self.help_scroll = 0.0;
        }
    }

    pub fn toggle_library(&mut self) {
        self.show_library = !self.show_library;
    }

    pub fn toggle_scope(&mut self) {
        self.show_scope = !self.show_scope;
    }

    pub fn cycle_scope_mode(&mut self) {
        self.scope_mode = match self.scope_mode {
            ScopeMode::TimeDomain => ScopeMode::Lissajous,
            ScopeMode::Lissajous => ScopeMode::StereoField,
            ScopeMode::StereoField => ScopeMode::Waterfall,
            ScopeMode::Waterfall => ScopeMode::TimeDomain,
        };
    }

    pub fn cycle_focus(&mut self) {
        self.focused = match self.focused {
            FocusedPane::DeckA => FocusedPane::DeckB,
            FocusedPane::DeckB => FocusedPane::DeckA,
            _ => FocusedPane::DeckA,
        };
    }

    pub fn update_animations(&mut self) {
        self.frame_count = self.frame_count.wrapping_add(1);

        // Beat pulse
        const PULSE_DECAY: f32 = 0.85;
        if self.prev_beat_phase_a > 0.9 && self.deck_a.beat_phase < 0.1 {
            self.beat_pulse_a = 1.0;
        }
        self.prev_beat_phase_a = self.deck_a.beat_phase;

        if self.prev_beat_phase_b > 0.9 && self.deck_b.beat_phase < 0.1 {
            self.beat_pulse_b = 1.0;
        }
        self.prev_beat_phase_b = self.deck_b.beat_phase;

        self.beat_pulse_a *= PULSE_DECAY;
        self.beat_pulse_b *= PULSE_DECAY;
        if self.beat_pulse_a < 0.01 {
            self.beat_pulse_a = 0.0;
        }
        if self.beat_pulse_b < 0.01 {
            self.beat_pulse_b = 0.0;
        }

        // Sync quality
        self.sync_quality = self.calculate_sync_quality();

        // VU peak hold
        self.update_peak_hold();

        // Spectrum afterglow
        self.update_spectrum_history();

        // Glitch decay
        if self.glitch_frames > 0 {
            self.glitch_frames -= 1;
            self.glitch_intensity *= 0.85;
            if self.glitch_frames == 0 {
                self.glitch_intensity = 0.0;
            }
        }

        // Energy bridge particles
        self.update_energy_particles();

        // Auto-clear messages after ~3 seconds (~90 frames at 30fps)
        if self.message.is_some() && self.frame_count.saturating_sub(self.message_frame) > 90 {
            self.message = None;
        }
    }

    fn calculate_sync_quality(&self) -> f32 {
        let has_grid_a = self.deck_a.beat_grid_info.as_ref().is_some_and(|g| g.has_grid);
        let has_grid_b = self.deck_b.beat_grid_info.as_ref().is_some_and(|g| g.has_grid);
        if !has_grid_a || !has_grid_b {
            return 0.0;
        }
        let phase_diff = (self.deck_a.beat_phase - self.deck_b.beat_phase).abs();
        let normalized = if phase_diff > 0.5 { 1.0 - phase_diff } else { phase_diff };
        1.0 - (normalized * 2.0)
    }

    fn update_peak_hold(&mut self) {
        const HOLD_FRAMES: u16 = 20;
        const DECAY_RATE: f32 = 0.92;

        if self.deck_a.peak_level > self.vu_peak_a {
            self.vu_peak_a = self.deck_a.peak_level;
            self.vu_peak_hold_frames_a = HOLD_FRAMES;
        } else if self.vu_peak_hold_frames_a > 0 {
            self.vu_peak_hold_frames_a -= 1;
        } else {
            self.vu_peak_a *= DECAY_RATE;
            if self.vu_peak_a < 0.001 { self.vu_peak_a = 0.0; }
        }

        if self.deck_b.peak_level > self.vu_peak_b {
            self.vu_peak_b = self.deck_b.peak_level;
            self.vu_peak_hold_frames_b = HOLD_FRAMES;
        } else if self.vu_peak_hold_frames_b > 0 {
            self.vu_peak_hold_frames_b -= 1;
        } else {
            self.vu_peak_b *= DECAY_RATE;
            if self.vu_peak_b < 0.001 { self.vu_peak_b = 0.0; }
        }
    }

    fn update_spectrum_history(&mut self) {
        for (i, &value) in self.deck_a.spectrum.bands.iter().take(SPECTRUM_BANDS).enumerate() {
            self.spectrum_history[i][self.spectrum_history_idx] = value;
        }
        self.spectrum_history_idx = (self.spectrum_history_idx + 1) % AFTERGLOW_HISTORY;

        // Update waterfall (scrolling spectrogram)
        let wf_idx = self.waterfall_idx;
        for (i, &value) in self.deck_a.spectrum.bands.iter().take(SPECTRUM_BANDS).enumerate() {
            self.waterfall_a[wf_idx][i] = value;
        }
        for (i, &value) in self.deck_b.spectrum.bands.iter().take(SPECTRUM_BANDS).enumerate() {
            self.waterfall_b[wf_idx][i] = value;
        }
        self.waterfall_idx = (self.waterfall_idx + 1) % WATERFALL_DEPTH;
    }

    pub fn update_energy_particles(&mut self) {
        let crossfader_norm = (self.crossfader + 1.0) / 2.0; // -1..1 → 0..1
        let energy_a = self.deck_a.peak_level;
        let energy_b = self.deck_b.peak_level;
        let total_energy = (energy_a + energy_b).max(0.001);

        // Spawn particles on beat pulses
        if self.beat_pulse_a > 0.8 || self.beat_pulse_b > 0.8 {
            let spawn_count = (total_energy * 4.0).ceil() as usize;
            for j in 0..spawn_count.min(8) {
                let from_a = self.beat_pulse_a > self.beat_pulse_b;
                let base_pos = if from_a { 0.0 } else { 1.0 };
                self.energy_particles.push(EnergyParticle {
                    pos: base_pos,
                    wave_offset: (self.frame_count as f32 * 0.1 + j as f32 * 0.7).sin() * 0.3,
                    brightness: 1.0,
                    speed: if from_a { 0.015 + j as f32 * 0.003 } else { -0.015 - j as f32 * 0.003 },
                    size: 1.5 + total_energy * 2.0,
                });
            }
        }

        // Also spawn ambient particles based on crossfader energy flow
        if self.frame_count.is_multiple_of(3) && total_energy > 0.05 {
            let flow_dir = if crossfader_norm < 0.45 {
                0.01 // flowing toward A
            } else if crossfader_norm > 0.55 {
                -0.01 // flowing toward B
            } else {
                0.0 // balanced
            };
            if flow_dir != 0.0 {
                self.energy_particles.push(EnergyParticle {
                    pos: if flow_dir > 0.0 { 1.0 } else { 0.0 },
                    wave_offset: (self.frame_count as f32 * 0.07).sin() * 0.2,
                    brightness: total_energy.min(1.0) * 0.6,
                    speed: -flow_dir * (1.0 + total_energy),
                    size: 1.0 + total_energy,
                });
            }
        }

        // Update existing particles
        self.energy_particles.retain_mut(|p| {
            p.pos += p.speed;
            p.brightness *= 0.96;
            p.wave_offset += (p.pos * 3.0).sin() * 0.01;
            // Remove if out of bounds or faded
            p.pos >= -0.05 && p.pos <= 1.05 && p.brightness > 0.02
        });

        // Cap particle count
        if self.energy_particles.len() > 64 {
            self.energy_particles.drain(0..self.energy_particles.len() - 64);
        }
    }
}
