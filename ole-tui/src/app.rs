//! Application state management (Elm architecture)

use crate::theme::{Theme, CRT_AMBER, CRT_GREEN, CYBERPUNK};
use crate::widgets::LibraryState;
use ole_audio::{AudioEvent, DeckState, FilterType, FilterMode};
use ole_input::Mode;

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

    // Library state
    pub library: LibraryState,
    pub show_library: bool,

    // Theme
    pub theme: Theme,

    // Animation frame counter
    pub frame_count: u64,
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
            // Library state
            library: LibraryState::default(),
            show_library: true,
            // Theme & animation
            theme: Theme::default(),
            frame_count: 0,
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
            }
            AudioEvent::TrackLoaded { deck } => {
                self.set_success(format!("Track loaded to deck {}", deck));
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
    }

    /// Toggle library display
    pub fn toggle_library(&mut self) {
        self.show_library = !self.show_library;
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
