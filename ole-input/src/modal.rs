//! Modal state machine for vim-style input handling

use crate::commands::{Command, DeckId, Direction, FilterType};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Input modes (vim-style)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Command,
    Effects,
    Help,
    Browser,
}

impl Mode {
    /// Get display name for the mode
    pub fn display_name(&self) -> &'static str {
        match self {
            Mode::Normal => "NORMAL",
            Mode::Command => "COMMAND",
            Mode::Effects => "EFFECTS",
            Mode::Help => "HELP",
            Mode::Browser => "BROWSE",
        }
    }
}

/// Handles keyboard input and converts to commands
pub struct InputHandler {
    mode: Mode,
    command_buffer: String,
    /// Effect sequence buffer for multi-key combos (e.g., d+1, f+l+5)
    effect_sequence: Vec<char>,
    /// Currently focused deck (effects apply to this deck)
    focused_deck: DeckId,
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            mode: Mode::Normal,
            command_buffer: String::new(),
            effect_sequence: Vec::with_capacity(4),
            focused_deck: DeckId::A,
        }
    }

    /// Set the currently focused deck
    pub fn set_focused_deck(&mut self, deck: DeckId) {
        self.focused_deck = deck;
    }

    /// Reset effect sequence buffer
    fn reset_effect_sequence(&mut self) {
        self.effect_sequence.clear();
    }

    /// Get current mode
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Get current command buffer (for display)
    pub fn command_buffer(&self) -> &str {
        &self.command_buffer
    }

    /// Handle a key event and return a command if applicable
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Command> {
        match self.mode {
            Mode::Normal => self.handle_normal_mode(key),
            Mode::Command => self.handle_command_mode(key),
            Mode::Effects => self.handle_effects_mode(key),
            Mode::Help => self.handle_help_mode(key),
            Mode::Browser => self.handle_browser_mode(key),
        }
    }

    fn handle_normal_mode(&mut self, key: KeyEvent) -> Option<Command> {
        match key.code {
            // Mode switching
            KeyCode::Char(':') => {
                self.mode = Mode::Command;
                self.command_buffer.clear();
                Some(Command::EnterCommandMode)
            }
            KeyCode::Char('e') => {
                self.mode = Mode::Effects;
                Some(Command::EnterEffectsMode)
            }
            KeyCode::Char('?') => {
                self.mode = Mode::Help;
                Some(Command::ToggleHelp)
            }
            KeyCode::Char('/') | KeyCode::Char('o') => {
                self.mode = Mode::Browser;
                Some(Command::EnterBrowserMode)
            }

            // Crossfader
            KeyCode::Char('h') | KeyCode::Left => Some(Command::MoveCrossfader(Direction::Left)),
            KeyCode::Char('l') | KeyCode::Right => Some(Command::MoveCrossfader(Direction::Right)),
            KeyCode::Char('\\') => Some(Command::CenterCrossfader),

            // Focus navigation
            KeyCode::Tab => Some(Command::CycleFocus),

            // Visualization toggle (spectrum <-> oscilloscope)
            KeyCode::Char('v') => Some(Command::ToggleScope),
            KeyCode::Char('V') => Some(Command::CycleScopeMode),

            // CRT screen effects
            KeyCode::Char('g') => Some(Command::CycleCrtIntensity), // Cycle Off/Subtle/Medium/Heavy
            KeyCode::Char('G') => Some(Command::ToggleCrt),         // Master CRT toggle

            // Mastering chain
            KeyCode::Char('p') => Some(Command::CycleMasteringPreset), // Cycle presets
            KeyCode::Char('P') => Some(Command::ToggleMastering),      // Toggle mastering on/off

            // Waveform zoom on focused deck
            KeyCode::Char('w') => Some(Command::ZoomIn(self.focused_deck)),
            KeyCode::Char('W') => Some(Command::ZoomOut(self.focused_deck)),

            // Beatjump on focused deck - multiple presets
            // j/k = 1 beat, Up/Down = 4 beats, J/K = 8 beats, n/N = 16 beats, m/M = 32 beats
            KeyCode::Char('j') => Some(Command::Beatjump(self.focused_deck, 1)), // Forward 1 beat
            KeyCode::Char('k') => Some(Command::Beatjump(self.focused_deck, -1)), // Back 1 beat
            KeyCode::Up => Some(Command::Beatjump(self.focused_deck, 4)),        // Forward 4 beats
            KeyCode::Down => Some(Command::Beatjump(self.focused_deck, -4)),     // Back 4 beats
            KeyCode::Char('J') => Some(Command::Beatjump(self.focused_deck, 8)), // Forward 8 beats
            KeyCode::Char('K') => Some(Command::Beatjump(self.focused_deck, -8)), // Back 8 beats
            KeyCode::Char('n') => Some(Command::Beatjump(self.focused_deck, 16)), // Forward 16 beats
            KeyCode::Char('N') => Some(Command::Beatjump(self.focused_deck, -16)), // Back 16 beats
            KeyCode::Char('m') => Some(Command::Beatjump(self.focused_deck, 32)), // Forward 32 beats
            KeyCode::Char('M') => Some(Command::Beatjump(self.focused_deck, -32)), // Back 32 beats

            // Cue points on focused deck (1-8 to jump, Shift+1-8 to set)
            KeyCode::Char('1') => Some(Command::JumpCue(self.focused_deck, 1)),
            KeyCode::Char('2') => Some(Command::JumpCue(self.focused_deck, 2)),
            KeyCode::Char('3') => Some(Command::JumpCue(self.focused_deck, 3)),
            KeyCode::Char('4') => Some(Command::JumpCue(self.focused_deck, 4)),
            KeyCode::Char('5') => Some(Command::JumpCue(self.focused_deck, 5)),
            KeyCode::Char('6') => Some(Command::JumpCue(self.focused_deck, 6)),
            KeyCode::Char('7') => Some(Command::JumpCue(self.focused_deck, 7)),
            KeyCode::Char('8') => Some(Command::JumpCue(self.focused_deck, 8)),
            KeyCode::Char('!') => Some(Command::SetCue(self.focused_deck, 1)),
            KeyCode::Char('@') => Some(Command::SetCue(self.focused_deck, 2)),
            KeyCode::Char('#') => Some(Command::SetCue(self.focused_deck, 3)),
            KeyCode::Char('$') => Some(Command::SetCue(self.focused_deck, 4)),
            KeyCode::Char('%') => Some(Command::SetCue(self.focused_deck, 5)),
            KeyCode::Char('^') => Some(Command::SetCue(self.focused_deck, 6)),
            KeyCode::Char('&') => Some(Command::SetCue(self.focused_deck, 7)),
            KeyCode::Char('*') => Some(Command::SetCue(self.focused_deck, 8)),

            // Deck A controls (lowercase)
            KeyCode::Char('a') => Some(Command::Toggle(DeckId::A)),
            KeyCode::Char('s') => Some(Command::Pause(DeckId::A)),
            KeyCode::Char('z') => Some(Command::Stop(DeckId::A)),
            // Fine nudge: 20ms for precise beat alignment
            KeyCode::Char('x') => Some(Command::Nudge(DeckId::A, -0.02)),
            KeyCode::Char('c') => Some(Command::Nudge(DeckId::A, 0.02)),

            // Deck B controls (uppercase)
            KeyCode::Char('A') => Some(Command::Toggle(DeckId::B)),
            KeyCode::Char('S') => Some(Command::Pause(DeckId::B)),
            KeyCode::Char('Z') => Some(Command::Stop(DeckId::B)),
            // Fine nudge: 20ms for precise beat alignment
            KeyCode::Char('X') => Some(Command::Nudge(DeckId::B, -0.02)),
            KeyCode::Char('C') => Some(Command::Nudge(DeckId::B, 0.02)),

            // Beat nudge: 1/16 beat for transient alignment (focused deck)
            KeyCode::Char('d') => Some(Command::BeatNudge(self.focused_deck, -0.0625)),
            KeyCode::Char('D') => Some(Command::BeatNudge(self.focused_deck, 0.0625)),

            // Tempo nudge
            KeyCode::Char('[') => Some(Command::AdjustTempo(DeckId::A, -0.01)),
            KeyCode::Char(']') => Some(Command::AdjustTempo(DeckId::A, 0.01)),
            KeyCode::Char('{') => Some(Command::AdjustTempo(DeckId::A, -0.1)),
            KeyCode::Char('}') => Some(Command::AdjustTempo(DeckId::A, 0.1)),

            // Tempo nudge deck B
            KeyCode::Char(',') => Some(Command::AdjustTempo(DeckId::B, -0.01)),
            KeyCode::Char('.') => Some(Command::AdjustTempo(DeckId::B, 0.01)),
            KeyCode::Char('<') => Some(Command::AdjustTempo(DeckId::B, -0.1)),
            KeyCode::Char('>') => Some(Command::AdjustTempo(DeckId::B, 0.1)),

            // BPM sync
            KeyCode::Char('b') => Some(Command::Sync(DeckId::B)), // Sync B to A
            KeyCode::Char('B') => Some(Command::Sync(DeckId::A)), // Sync A to B

            // Gain
            KeyCode::Char('-') => Some(Command::AdjustGain(DeckId::A, -0.05)),
            KeyCode::Char('=') => Some(Command::AdjustGain(DeckId::A, 0.05)),
            KeyCode::Char('_') => Some(Command::AdjustGain(DeckId::B, -0.05)),
            KeyCode::Char('+') => Some(Command::AdjustGain(DeckId::B, 0.05)),

            // Quit
            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Command::Quit)
            }

            KeyCode::Esc => Some(Command::Cancel),

            _ => None,
        }
    }

    fn handle_command_mode(&mut self, key: KeyEvent) -> Option<Command> {
        match key.code {
            KeyCode::Enter => {
                let cmd = self.parse_command();
                self.mode = Mode::Normal;
                let buffer = std::mem::take(&mut self.command_buffer);
                cmd.or(Some(Command::ExecuteCommand(buffer)))
            }
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.command_buffer.clear();
                Some(Command::EnterNormalMode)
            }
            KeyCode::Backspace => {
                self.command_buffer.pop();
                if self.command_buffer.is_empty() {
                    self.mode = Mode::Normal;
                    Some(Command::EnterNormalMode)
                } else {
                    None
                }
            }
            KeyCode::Char(c) => {
                self.command_buffer.push(c);
                None
            }
            _ => None,
        }
    }

    fn parse_command(&self) -> Option<Command> {
        let input = self.command_buffer.trim();

        // Handle simple commands first
        if input == "q" || input == "quit" {
            return Some(Command::Quit);
        }
        if input == "sync" {
            return Some(Command::Sync(DeckId::B));
        }
        if input == "help" {
            return Some(Command::ToggleHelp);
        }

        // Handle load command with potential quoted path
        if let Some(rest) = input.strip_prefix("load ") {
            let rest = rest.trim();

            // Parse deck identifier and get remaining path
            let (deck, path_part) = if let Some(p) = rest.strip_prefix("a ") {
                (DeckId::A, p)
            } else if let Some(p) = rest.strip_prefix("b ") {
                (DeckId::B, p)
            } else {
                return None;
            };

            // Extract path (handle quoted or unquoted)
            let path = path_part.trim();
            let path = if (path.starts_with('\'') && path.ends_with('\''))
                || (path.starts_with('"') && path.ends_with('"'))
            {
                // Remove surrounding quotes
                &path[1..path.len() - 1]
            } else {
                path
            };

            if !path.is_empty() {
                return Some(Command::LoadTrack(deck, path.into()));
            }
        }

        // Handle theme command
        if let Some(name) = input.strip_prefix("theme ") {
            let name = name.trim();
            if !name.is_empty() {
                return Some(Command::SetTheme(name.to_string()));
            }
        }

        // Handle scan command for library
        if let Some(path) = input.strip_prefix("scan ") {
            let path = path.trim();
            // Remove surrounding quotes if present
            let path = if (path.starts_with('\'') && path.ends_with('\''))
                || (path.starts_with('"') && path.ends_with('"'))
            {
                &path[1..path.len() - 1]
            } else {
                path
            };

            if !path.is_empty() {
                return Some(Command::LibraryScan(path.into()));
            }
        }

        // Toggle library visibility
        if input == "library" || input == "lib" {
            return Some(Command::LibraryToggle);
        }

        None
    }

    fn handle_effects_mode(&mut self, key: KeyEvent) -> Option<Command> {
        let deck = self.focused_deck;

        // Handle escape to exit effects mode
        if key.code == KeyCode::Esc {
            self.mode = Mode::Normal;
            self.reset_effect_sequence();
            return Some(Command::EnterNormalMode);
        }

        // Handle single-key effect toggles first
        if let KeyCode::Char(c) = key.code {
            match c {
                // Tape stop/start
                't' => {
                    self.mode = Mode::Normal;
                    return Some(Command::TriggerTapeStop(deck));
                }
                'T' => {
                    self.mode = Mode::Normal;
                    return Some(Command::TriggerTapeStart(deck));
                }
                // Flanger toggle
                'g' => {
                    self.mode = Mode::Normal;
                    return Some(Command::ToggleFlanger(deck));
                }
                // Bitcrusher toggle
                'c' => {
                    self.mode = Mode::Normal;
                    return Some(Command::ToggleBitcrusher(deck));
                }
                // Vinyl emulation toggle
                'v' => {
                    self.mode = Mode::Normal;
                    return Some(Command::ToggleVinyl(deck));
                }
                // Filter mode cycle
                'm' => {
                    self.mode = Mode::Normal;
                    return Some(Command::CycleFilterMode(deck));
                }
                // Delay modulation cycle
                'M' => {
                    self.mode = Mode::Normal;
                    return Some(Command::CycleDelayModulation(deck));
                }
                _ => {}
            }
        }

        // Handle character input for effect sequences (d+level, r+level, f+type+level)
        if let KeyCode::Char(c) = key.code {
            self.effect_sequence.push(c);

            // Check for complete sequences
            let result = self.check_effect_sequence();

            if result.is_some() {
                self.reset_effect_sequence();
                // Auto-return to normal mode after effect is set
                self.mode = Mode::Normal;
                return result;
            }

            // If sequence is too long or invalid prefix, reset
            if self.effect_sequence.len() > 3 || !self.is_valid_effect_prefix() {
                self.reset_effect_sequence();
            }
        }

        None
    }

    /// Check if current sequence is a valid prefix for an effect command
    fn is_valid_effect_prefix(&self) -> bool {
        matches!(
            self.effect_sequence.as_slice(),
            [] | ['d'] | ['r'] | ['f'] | ['f', 'l'] | ['f', 'b'] | ['f', 'h']
        )
    }

    /// Check if effect sequence is complete and return command if so
    fn check_effect_sequence(&self) -> Option<Command> {
        let deck = self.focused_deck;

        match self.effect_sequence.as_slice() {
            // Delay: d + 0-5 (0 = off)
            ['d', level] if ('0'..='5').contains(level) => {
                let lvl = (*level as u8) - b'0';
                Some(Command::SetDelayLevel(deck, lvl))
            }

            // Reverb: r + 0-5 (0 = off)
            ['r', level] if ('0'..='5').contains(level) => {
                let lvl = (*level as u8) - b'0';
                Some(Command::SetReverbLevel(deck, lvl))
            }

            // Filter off: f + 0
            ['f', '0'] => Some(Command::SetFilterPreset(deck, FilterType::LowPass, 0)),

            // Filter: f + l|b|h + 1-0 (where 0 = level 10)
            ['f', filter_char, level] => {
                let filter_type = match filter_char {
                    'l' => FilterType::LowPass,
                    'b' => FilterType::BandPass,
                    'h' => FilterType::HighPass,
                    _ => return None,
                };

                let lvl = match level {
                    '1'..='9' => (*level as u8) - b'0',
                    '0' => 10,
                    _ => return None,
                };

                Some(Command::SetFilterPreset(deck, filter_type, lvl))
            }

            _ => None,
        }
    }

    fn handle_help_mode(&mut self, key: KeyEvent) -> Option<Command> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                self.mode = Mode::Normal;
                Some(Command::ToggleHelp)
            }
            // Scrolling
            KeyCode::Char('j') | KeyCode::Down => Some(Command::HelpScrollDown),
            KeyCode::Char('k') | KeyCode::Up => Some(Command::HelpScrollUp),
            _ => None,
        }
    }

    fn handle_browser_mode(&mut self, key: KeyEvent) -> Option<Command> {
        match key.code {
            // Exit browser mode
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                Some(Command::EnterNormalMode)
            }

            // Navigation
            KeyCode::Char('j') | KeyCode::Down => Some(Command::LibrarySelectNext),
            KeyCode::Char('k') | KeyCode::Up => Some(Command::LibrarySelectPrev),
            KeyCode::Char('g') => Some(Command::LibrarySelectFirst),
            KeyCode::Char('G') => Some(Command::LibrarySelectLast),

            // Load to deck
            KeyCode::Char('a') => {
                self.mode = Mode::Normal;
                Some(Command::LibraryLoadToDeck(DeckId::A))
            }
            KeyCode::Char('b') => {
                self.mode = Mode::Normal;
                Some(Command::LibraryLoadToDeck(DeckId::B))
            }
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                Some(Command::LibraryLoadToDeck(self.focused_deck))
            }

            // Filter compatible keys (harmonic mixing)
            KeyCode::Char('f') => Some(Command::LibraryFilterCompatible),

            // Clear filter
            KeyCode::Char('c') => Some(Command::LibraryClearFilter),

            // Toggle library visibility
            KeyCode::Char('l') => Some(Command::LibraryToggle),

            // Quick jump to Camelot key (1-9, 0=10) - A keys (minor)
            KeyCode::Char('1') => Some(Command::LibraryJumpToKey(1, true)),
            KeyCode::Char('2') => Some(Command::LibraryJumpToKey(2, true)),
            KeyCode::Char('3') => Some(Command::LibraryJumpToKey(3, true)),
            KeyCode::Char('4') => Some(Command::LibraryJumpToKey(4, true)),
            KeyCode::Char('5') => Some(Command::LibraryJumpToKey(5, true)),
            KeyCode::Char('6') => Some(Command::LibraryJumpToKey(6, true)),
            KeyCode::Char('7') => Some(Command::LibraryJumpToKey(7, true)),
            KeyCode::Char('8') => Some(Command::LibraryJumpToKey(8, true)),
            KeyCode::Char('9') => Some(Command::LibraryJumpToKey(9, true)),
            KeyCode::Char('0') => Some(Command::LibraryJumpToKey(10, true)),
            KeyCode::Char('-') => Some(Command::LibraryJumpToKey(11, true)),
            KeyCode::Char('=') => Some(Command::LibraryJumpToKey(12, true)),

            // Shift+number for B keys (major)
            KeyCode::Char('!') => Some(Command::LibraryJumpToKey(1, false)),
            KeyCode::Char('@') => Some(Command::LibraryJumpToKey(2, false)),
            KeyCode::Char('#') => Some(Command::LibraryJumpToKey(3, false)),
            KeyCode::Char('$') => Some(Command::LibraryJumpToKey(4, false)),
            KeyCode::Char('%') => Some(Command::LibraryJumpToKey(5, false)),
            KeyCode::Char('^') => Some(Command::LibraryJumpToKey(6, false)),
            KeyCode::Char('&') => Some(Command::LibraryJumpToKey(7, false)),
            KeyCode::Char('*') => Some(Command::LibraryJumpToKey(8, false)),
            KeyCode::Char('(') => Some(Command::LibraryJumpToKey(9, false)),
            KeyCode::Char(')') => Some(Command::LibraryJumpToKey(10, false)),
            KeyCode::Char('_') => Some(Command::LibraryJumpToKey(11, false)),
            KeyCode::Char('+') => Some(Command::LibraryJumpToKey(12, false)),

            // BPM jumps (common DJ tempos)
            KeyCode::Char('q') => Some(Command::LibraryJumpToBpm(120)), // House
            KeyCode::Char('w') => Some(Command::LibraryJumpToBpm(128)), // Electro
            KeyCode::Char('e') => Some(Command::LibraryJumpToBpm(130)), // Techno
            KeyCode::Char('r') => Some(Command::LibraryJumpToBpm(140)), // Trance
            KeyCode::Char('t') => Some(Command::LibraryJumpToBpm(150)), // Hard
            KeyCode::Char('y') => Some(Command::LibraryJumpToBpm(170)), // DnB

            _ => None,
        }
    }
}

impl Default for InputHandler {
    fn default() -> Self {
        Self::new()
    }
}
