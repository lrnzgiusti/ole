//! Application state management (Elm architecture)

use crate::theme::{Theme, CRT_AMBER, CRT_GREEN, CYBERPUNK};
use crate::widgets::{LibraryState, ScopeMode};
use ole_audio::{AudioEvent, DeckState, FilterMode, FilterType, LufsValues, MasteringPreset};
use ole_input::Mode;

/// Number of spectrum bands
pub const SPECTRUM_BANDS: usize = 32;
/// Number of frames to keep in afterglow history
const AFTERGLOW_HISTORY: usize = 15;

/// CRT intensity preset levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CrtIntensity {
    /// All CRT effects disabled
    Off,
    /// Subtle effects - barely noticeable
    #[default]
    Subtle,
    /// Medium effects - noticeable but not distracting
    Medium,
    /// Heavy effects - strong retro feel
    Heavy,
}

impl CrtIntensity {
    /// Cycle to next intensity level
    pub fn cycle(self) -> Self {
        match self {
            CrtIntensity::Off => CrtIntensity::Subtle,
            CrtIntensity::Subtle => CrtIntensity::Medium,
            CrtIntensity::Medium => CrtIntensity::Heavy,
            CrtIntensity::Heavy => CrtIntensity::Off,
        }
    }

    /// Get display name
    pub fn name(self) -> &'static str {
        match self {
            CrtIntensity::Off => "OFF",
            CrtIntensity::Subtle => "SUBTLE",
            CrtIntensity::Medium => "MEDIUM",
            CrtIntensity::Heavy => "HEAVY",
        }
    }

    /// Get scanline intensity for this preset (0.0-1.0)
    pub fn scanline_intensity(self) -> f32 {
        match self {
            CrtIntensity::Off => 0.0,
            CrtIntensity::Subtle => 0.20,
            CrtIntensity::Medium => 0.40,
            CrtIntensity::Heavy => 0.60,
        }
    }

    /// Get glow intensity for this preset (0.0-1.0)
    pub fn glow_intensity(self) -> f32 {
        match self {
            CrtIntensity::Off => 0.0,
            CrtIntensity::Subtle => 0.3,
            CrtIntensity::Medium => 0.5,
            CrtIntensity::Heavy => 0.7,
        }
    }

    /// Get noise intensity for this preset (0.0-1.0)
    pub fn noise_intensity(self) -> f32 {
        match self {
            CrtIntensity::Off => 0.0,
            CrtIntensity::Subtle => 0.02,
            CrtIntensity::Medium => 0.05,
            CrtIntensity::Heavy => 0.10,
        }
    }

    /// Get chromatic aberration offset for this preset (pixels)
    pub fn chromatic_offset(self) -> u8 {
        match self {
            CrtIntensity::Off => 0,
            CrtIntensity::Subtle => 1,
            CrtIntensity::Medium => 2,
            CrtIntensity::Heavy => 3,
        }
    }
}

/// CRT visual effects state for phosphor persistence and other retro effects
#[derive(Debug, Clone)]
pub struct CrtEffects {
    /// Phosphor afterglow history for spectrum (per-band previous heights)
    pub spectrum_history: [[f32; AFTERGLOW_HISTORY]; SPECTRUM_BANDS],
    pub spectrum_history_idx: usize,

    /// Peak hold for VU meters (UI-side tracking for classic analog behavior)
    pub vu_peak_a: f32,
    pub vu_peak_b: f32,
    pub vu_peak_hold_frames_a: u16,
    pub vu_peak_hold_frames_b: u16,

    /// Screen flicker state (triggered on track load)
    pub flicker_frames_remaining: u8,
    pub flicker_intensity: f32,

    /// Scanline phase offset (for rolling effect)
    pub scanline_offset: u8,
    /// Whether scanlines are enabled
    pub scanlines_enabled: bool,

    // --- New CRT screen effects ---
    /// Master toggle for all CRT post-processing effects
    pub crt_enabled: bool,

    /// Current CRT intensity preset
    pub intensity: CrtIntensity,

    /// Phosphor glow enabled
    pub glow_enabled: bool,
    /// Phosphor glow intensity (0.0-1.0)
    pub glow_intensity: f32,

    /// Static/noise enabled
    pub noise_enabled: bool,
    /// Static/noise intensity (0.0-1.0)
    pub noise_intensity: f32,

    /// Chromatic aberration enabled
    pub chromatic_enabled: bool,
    /// Chromatic aberration offset in pixels (1-3)
    pub chromatic_offset: u8,
}

impl Default for CrtEffects {
    fn default() -> Self {
        let intensity = CrtIntensity::default();
        Self {
            spectrum_history: [[0.0; AFTERGLOW_HISTORY]; SPECTRUM_BANDS],
            spectrum_history_idx: 0,
            vu_peak_a: 0.0,
            vu_peak_b: 0.0,
            vu_peak_hold_frames_a: 0,
            vu_peak_hold_frames_b: 0,
            flicker_frames_remaining: 0,
            flicker_intensity: 0.0,
            scanline_offset: 0,
            scanlines_enabled: true,
            // New CRT effects - most disabled by default to prevent visual artifacts
            crt_enabled: true,
            intensity,
            glow_enabled: false, // Disabled by default - causes color bleeding
            glow_intensity: intensity.glow_intensity(),
            noise_enabled: false, // Disabled by default - causes visual artifacts
            noise_intensity: intensity.noise_intensity(),
            chromatic_enabled: false, // Disabled by default - causes color shifting on UI elements
            chromatic_offset: intensity.chromatic_offset(),
        }
    }
}

impl CrtEffects {
    /// Update peak hold state each frame (call from main loop)
    pub fn update_peak_hold(&mut self, level_a: f32, level_b: f32) {
        const HOLD_FRAMES: u16 = 20; // ~667ms at 30fps
        const DECAY_RATE: f32 = 0.92;

        // Deck A
        if level_a > self.vu_peak_a {
            self.vu_peak_a = level_a;
            self.vu_peak_hold_frames_a = HOLD_FRAMES;
        } else if self.vu_peak_hold_frames_a > 0 {
            self.vu_peak_hold_frames_a -= 1;
        } else {
            self.vu_peak_a *= DECAY_RATE;
            if self.vu_peak_a < 0.001 {
                self.vu_peak_a = 0.0;
            }
        }

        // Deck B
        if level_b > self.vu_peak_b {
            self.vu_peak_b = level_b;
            self.vu_peak_hold_frames_b = HOLD_FRAMES;
        } else if self.vu_peak_hold_frames_b > 0 {
            self.vu_peak_hold_frames_b -= 1;
        } else {
            self.vu_peak_b *= DECAY_RATE;
            if self.vu_peak_b < 0.001 {
                self.vu_peak_b = 0.0;
            }
        }
    }

    /// Update spectrum afterglow history (call each frame with current spectrum data)
    pub fn update_spectrum_history(&mut self, spectrum: &[f32]) {
        // Store current spectrum values in history ring buffer
        for (i, &value) in spectrum.iter().take(SPECTRUM_BANDS).enumerate() {
            self.spectrum_history[i][self.spectrum_history_idx] = value;
        }
        self.spectrum_history_idx = (self.spectrum_history_idx + 1) % AFTERGLOW_HISTORY;
    }

    /// Update flicker decay (call each frame)
    pub fn update_flicker(&mut self) {
        if self.flicker_frames_remaining > 0 {
            self.flicker_frames_remaining -= 1;
            self.flicker_intensity *= 0.85;
        }
    }

    /// Trigger screen flicker (call on track load)
    pub fn trigger_flicker(&mut self) {
        self.flicker_frames_remaining = 8;
        self.flicker_intensity = 1.0;
    }

    /// Update scanline offset for rolling effect
    /// Note: Rolling disabled to prevent visual flickering on waveforms
    pub fn update_scanlines(&mut self) {
        // Rolling scanlines disabled - causes flickering on waveforms
        // self.scanline_offset = self.scanline_offset.wrapping_add(1);
    }

    /// Toggle scanlines on/off
    pub fn toggle_scanlines(&mut self) {
        self.scanlines_enabled = !self.scanlines_enabled;
    }

    /// Toggle all CRT post-processing effects (master switch)
    pub fn toggle_crt(&mut self) {
        self.crt_enabled = !self.crt_enabled;
    }

    /// Toggle phosphor glow effect
    pub fn toggle_glow(&mut self) {
        self.glow_enabled = !self.glow_enabled;
    }

    /// Toggle static/noise effect
    pub fn toggle_noise(&mut self) {
        self.noise_enabled = !self.noise_enabled;
    }

    /// Toggle chromatic aberration effect
    pub fn toggle_chromatic(&mut self) {
        self.chromatic_enabled = !self.chromatic_enabled;
    }

    /// Cycle through CRT intensity presets (Off -> Subtle -> Medium -> Heavy -> Off)
    pub fn cycle_intensity(&mut self) {
        self.intensity = self.intensity.cycle();
        // Update all effect parameters based on preset
        self.glow_intensity = self.intensity.glow_intensity();
        self.noise_intensity = self.intensity.noise_intensity();
        self.chromatic_offset = self.intensity.chromatic_offset();
    }
}

/// Zoom level for waveform display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaveformZoom {
    /// Full track view (100%)
    #[default]
    X1,
    /// Half track view (50%)
    X2,
    /// Quarter track view (25%)
    X4,
    /// Eighth track view (12.5%)
    X8,
}

impl WaveformZoom {
    /// Zoom in one level
    pub fn zoom_in(self) -> Self {
        match self {
            Self::X1 => Self::X2,
            Self::X2 => Self::X4,
            Self::X4 => Self::X8,
            Self::X8 => Self::X8, // Max zoom
        }
    }

    /// Zoom out one level
    pub fn zoom_out(self) -> Self {
        match self {
            Self::X1 => Self::X1, // Min zoom
            Self::X2 => Self::X1,
            Self::X4 => Self::X2,
            Self::X8 => Self::X4,
        }
    }

    /// Get the fraction of the track visible at this zoom level
    pub fn viewport_fraction(self) -> f64 {
        match self {
            Self::X1 => 1.0,
            Self::X2 => 0.5,
            Self::X4 => 0.25,
            Self::X8 => 0.125,
        }
    }

    /// Get display name for UI
    pub fn label(self) -> &'static str {
        match self {
            Self::X1 => "1x",
            Self::X2 => "2x",
            Self::X4 => "4x",
            Self::X8 => "8x",
        }
    }
}

/// Which pane is currently focused
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusedPane {
    #[default]
    DeckA,
    DeckB,
    Crossfader,
    Effects,
    Library,
}

/// Message type for colored status messages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MessageType {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

/// Application state
pub struct AppState {
    // Deck states (updated from audio engine)
    pub deck_a: DeckState,
    pub deck_b: DeckState,

    // Mixer state
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

    // Delay states
    pub delay_a_enabled: bool,
    pub delay_a_level: u8,
    pub delay_b_enabled: bool,
    pub delay_b_level: u8,

    // Reverb states
    pub reverb_a_enabled: bool,
    pub reverb_a_level: u8,
    pub reverb_b_enabled: bool,
    pub reverb_b_level: u8,

    // UI state
    pub mode: Mode,
    pub focused: FocusedPane,
    pub command_buffer: String,
    pub message: Option<String>,
    pub message_type: MessageType,
    pub show_help: bool,
    pub help_scroll: u16,

    // Library state
    pub library: LibraryState,
    pub show_library: bool,

    // Scope/Spectrum toggle
    pub show_scope: bool,
    pub scope_mode: ScopeMode,

    // Theme
    pub theme: Theme,

    // Animation state
    pub frame_count: u64,
    /// Beat pulse intensity for deck A (0.0-1.0, decays each frame)
    pub beat_pulse_a: f32,
    /// Beat pulse intensity for deck B (0.0-1.0, decays each frame)
    pub beat_pulse_b: f32,
    /// Previous beat phase for deck A (to detect downbeat crossing)
    prev_beat_phase_a: f32,
    /// Previous beat phase for deck B
    prev_beat_phase_b: f32,

    // Waveform zoom levels
    pub zoom_a: WaveformZoom,
    pub zoom_b: WaveformZoom,

    // CRT visual effects
    pub crt_effects: CrtEffects,

    // Sync quality (0.0-1.0, used for glow effect when decks are phase-locked)
    pub sync_quality: f32,

    // Mastering chain state
    pub mastering_enabled: bool,
    pub mastering_preset: MasteringPreset,
    pub mastering_lufs: LufsValues,
    pub mastering_gain_reduction: f32,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            deck_a: DeckState::default(),
            deck_b: DeckState::default(),
            crossfader: 0.0,
            master_volume: 1.0,
            // Filter state
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
            // Delay state
            delay_a_enabled: false,
            delay_a_level: 0,
            delay_b_enabled: false,
            delay_b_level: 0,
            // Reverb state
            reverb_a_enabled: false,
            reverb_a_level: 0,
            reverb_b_enabled: false,
            reverb_b_level: 0,
            // UI state
            mode: Mode::Normal,
            focused: FocusedPane::DeckA,
            command_buffer: String::new(),
            message: None,
            message_type: MessageType::Info,
            show_help: false,
            help_scroll: 0,
            // Library state
            library: LibraryState::default(),
            show_library: true,
            // Scope/Spectrum
            show_scope: false,
            scope_mode: ScopeMode::default(),
            // Theme & animation
            theme: Theme::default(),
            frame_count: 0,
            beat_pulse_a: 0.0,
            beat_pulse_b: 0.0,
            prev_beat_phase_a: 0.0,
            prev_beat_phase_b: 0.0,
            // Waveform zoom
            zoom_a: WaveformZoom::default(),
            zoom_b: WaveformZoom::default(),
            // CRT effects
            crt_effects: CrtEffects::default(),
            // Sync quality
            sync_quality: 0.0,
            // Mastering state
            mastering_enabled: true,
            mastering_preset: MasteringPreset::default(),
            mastering_lufs: LufsValues::default(),
            mastering_gain_reduction: 0.0,
        }
    }
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update state from audio engine event
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
                delay_b_enabled,
                delay_b_level,
                reverb_a_enabled,
                reverb_a_level,
                reverb_b_enabled,
                reverb_b_level,
                mastering_enabled,
                mastering_preset,
                mastering_lufs,
                mastering_gain_reduction,
                // New fields (vinyl, time stretch, delay modulation) - to be used in UI later
                ..
            } => {
                self.deck_a = *deck_a;
                self.deck_b = *deck_b;
                self.crossfader = crossfader;
                self.master_volume = master_volume;
                // Filter state
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
                // Delay state
                self.delay_a_enabled = delay_a_enabled;
                self.delay_a_level = delay_a_level;
                self.delay_b_enabled = delay_b_enabled;
                self.delay_b_level = delay_b_level;
                // Reverb state
                self.reverb_a_enabled = reverb_a_enabled;
                self.reverb_a_level = reverb_a_level;
                self.reverb_b_enabled = reverb_b_enabled;
                self.reverb_b_level = reverb_b_level;
                // Mastering state
                self.mastering_enabled = mastering_enabled;
                self.mastering_preset = mastering_preset;
                self.mastering_lufs = mastering_lufs;
                self.mastering_gain_reduction = mastering_gain_reduction;
            }
            AudioEvent::TrackLoaded { deck } => {
                self.set_success(format!("Track loaded to deck {}", deck));
                // Trigger CRT flicker effect on track load
                self.crt_effects.trigger_flicker();
            }
            AudioEvent::Error(msg) => {
                self.set_error(format!("Error: {}", msg));
            }
        }
    }

    /// Set current mode
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        if mode != Mode::Command {
            self.command_buffer.clear();
        }
    }

    /// Toggle help display
    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
        if self.show_help {
            self.help_scroll = 0; // Reset scroll when opening
        }
    }

    /// Scroll help up
    pub fn help_scroll_up(&mut self) {
        self.help_scroll = self.help_scroll.saturating_sub(3);
    }

    /// Scroll help down
    pub fn help_scroll_down(&mut self) {
        self.help_scroll = self.help_scroll.saturating_add(3);
    }

    /// Toggle library display
    pub fn toggle_library(&mut self) {
        self.show_library = !self.show_library;
    }

    /// Toggle between spectrum and scope view
    pub fn toggle_scope(&mut self) {
        self.show_scope = !self.show_scope;
    }

    /// Cycle scope mode (time domain -> lissajous -> time domain)
    pub fn cycle_scope_mode(&mut self) {
        self.scope_mode = match self.scope_mode {
            ScopeMode::TimeDomain => ScopeMode::Lissajous,
            ScopeMode::Lissajous => ScopeMode::TimeDomain,
        };
    }

    /// Set theme by name
    pub fn set_theme(&mut self, name: &str) {
        self.theme = match name.to_lowercase().as_str() {
            "green" | "phosphor" | "phosphor-green" => CRT_GREEN,
            "amber" | "orange" => CRT_AMBER,
            "cyber" | "cyberpunk" | "neon" => CYBERPUNK,
            _ => {
                self.set_error(format!("Unknown theme: {}. Use green/amber/cyber", name));
                return;
            }
        };
        self.set_success(format!("Theme set to: {}", self.theme.name));
    }

    /// Cycle focus between decks only (Tab toggles A/B)
    pub fn cycle_focus(&mut self) {
        self.focused = match self.focused {
            FocusedPane::DeckA => FocusedPane::DeckB,
            FocusedPane::DeckB => FocusedPane::DeckA,
            // If focused on something else, go to Deck A
            _ => FocusedPane::DeckA,
        };
    }

    /// Focus specific pane
    pub fn focus(&mut self, pane: FocusedPane) {
        self.focused = pane;
    }

    /// Clear any displayed message
    pub fn clear_message(&mut self) {
        self.message = None;
        self.message_type = MessageType::Info;
    }

    /// Set a message to display (info level)
    pub fn set_message(&mut self, msg: impl Into<String>) {
        self.message = Some(msg.into());
        self.message_type = MessageType::Info;
    }

    /// Set a success message (green)
    pub fn set_success(&mut self, msg: impl Into<String>) {
        self.message = Some(msg.into());
        self.message_type = MessageType::Success;
    }

    /// Set a warning message (yellow)
    pub fn set_warning(&mut self, msg: impl Into<String>) {
        self.message = Some(msg.into());
        self.message_type = MessageType::Warning;
    }

    /// Set an error message (red)
    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.message = Some(msg.into());
        self.message_type = MessageType::Error;
    }

    /// Update all CRT effects (call each frame after audio state update)
    pub fn update_crt_effects(&mut self) {
        // Update peak hold with current deck audio levels (not gain setting!)
        self.crt_effects
            .update_peak_hold(self.deck_a.peak_level, self.deck_b.peak_level);

        // Update spectrum history with current spectrum data
        self.crt_effects
            .update_spectrum_history(&self.deck_a.spectrum.bands);

        // Update flicker decay
        self.crt_effects.update_flicker();

        // Update scanline offset
        self.crt_effects.update_scanlines();
    }

    /// Update beat pulse animation state (call each frame)
    pub fn update_beat_pulse(&mut self) {
        const PULSE_DECAY: f32 = 0.85; // How fast the pulse fades (per frame)
        const PULSE_INTENSITY: f32 = 1.0; // Initial pulse brightness

        // Check for downbeat crossing on deck A
        // Downbeat is when phase wraps from high (>0.9) to low (<0.1)
        if self.prev_beat_phase_a > 0.9 && self.deck_a.beat_phase < 0.1 {
            self.beat_pulse_a = PULSE_INTENSITY;
        }
        self.prev_beat_phase_a = self.deck_a.beat_phase;

        // Check for downbeat crossing on deck B
        if self.prev_beat_phase_b > 0.9 && self.deck_b.beat_phase < 0.1 {
            self.beat_pulse_b = PULSE_INTENSITY;
        }
        self.prev_beat_phase_b = self.deck_b.beat_phase;

        // Decay pulses
        self.beat_pulse_a *= PULSE_DECAY;
        self.beat_pulse_b *= PULSE_DECAY;

        // Clamp to zero below threshold
        if self.beat_pulse_a < 0.01 {
            self.beat_pulse_a = 0.0;
        }
        if self.beat_pulse_b < 0.01 {
            self.beat_pulse_b = 0.0;
        }
    }

    /// Calculate sync quality between decks (0.0-1.0)
    /// Returns how well phase-locked the two decks are:
    /// - 1.0 = perfectly in sync (phases match)
    /// - 0.0 = completely out of sync (180° out of phase)
    pub fn calculate_sync_quality(&self) -> f32 {
        // Need both decks with beat grids to calculate sync
        let has_grid_a = self
            .deck_a
            .beat_grid_info
            .as_ref()
            .is_some_and(|g| g.has_grid);
        let has_grid_b = self
            .deck_b
            .beat_grid_info
            .as_ref()
            .is_some_and(|g| g.has_grid);

        if !has_grid_a || !has_grid_b {
            return 0.0;
        }

        // Calculate phase difference (0.0-1.0)
        let phase_diff = (self.deck_a.beat_phase - self.deck_b.beat_phase).abs();
        // Normalize: 0.5 diff = furthest apart, 0.0 or 1.0 = in sync
        let normalized_diff = if phase_diff > 0.5 {
            1.0 - phase_diff
        } else {
            phase_diff
        };
        // Convert to quality: 0.0 diff = 1.0 quality, 0.5 diff = 0.0 quality
        1.0 - (normalized_diff * 2.0)
    }

    /// Update sync quality (call each frame after audio state update)
    pub fn update_sync_quality(&mut self) {
        self.sync_quality = self.calculate_sync_quality();
    }
}

/// Main application wrapper
pub struct App {
    pub state: AppState,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            state: AppState::new(),
            should_quit: false,
        }
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
