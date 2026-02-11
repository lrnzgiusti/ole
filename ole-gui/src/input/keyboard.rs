use egui::{Context, Key};

use ole_input::{Command, DeckId, Direction};
use crate::state::{FocusedPane, GuiState};

pub fn handle_keyboard(ctx: &Context, state: &mut GuiState) -> Vec<Command> {
    let mut commands = Vec::new();

    ctx.input(|input| {
        // Quit: Ctrl+Q
        if input.modifiers.command && input.key_pressed(Key::Q) {
            commands.push(Command::Quit);
            return;
        }

        // Mode-specific handling
        match state.mode {
            ole_input::Mode::Normal => {
                handle_normal_mode(input, state, &mut commands);
            }
            ole_input::Mode::Command => {
                handle_command_mode(input, state, &mut commands);
            }
            ole_input::Mode::Effects => {
                handle_effects_mode(input, state, &mut commands);
            }
            ole_input::Mode::Help => {
                handle_help_mode(input, state, &mut commands);
            }
            ole_input::Mode::Browser => {
                handle_browser_mode(input, state, &mut commands);
            }
        }
    });

    commands
}

fn focused_deck(state: &GuiState) -> DeckId {
    match state.focused {
        FocusedPane::DeckA => DeckId::A,
        FocusedPane::DeckB => DeckId::B,
        _ => DeckId::A,
    }
}

fn handle_normal_mode(
    input: &egui::InputState,
    state: &mut GuiState,
    cmds: &mut Vec<Command>,
) {
    // Mode switching: ':' enters command mode (Shift+Semicolon or via Text event)
    if input.key_pressed(Key::Semicolon) && input.modifiers.shift {
        state.set_mode(ole_input::Mode::Command);
        cmds.push(Command::EnterCommandMode);
        return;
    }
    // Also check for ':' as text event (handles different keyboard layouts)
    for event in &input.events {
        if let egui::Event::Text(text) = event {
            if text == ":" {
                state.set_mode(ole_input::Mode::Command);
                cmds.push(Command::EnterCommandMode);
                return;
            }
        }
    }
    if input.key_pressed(Key::E) && !input.modifiers.shift {
        state.set_mode(ole_input::Mode::Effects);
        cmds.push(Command::EnterEffectsMode);
        return;
    }
    if input.key_pressed(Key::Questionmark) {
        state.set_mode(ole_input::Mode::Help);
        cmds.push(Command::ToggleHelp);
        return;
    }
    if input.key_pressed(Key::Slash) || (input.key_pressed(Key::O) && !input.modifiers.shift) {
        state.set_mode(ole_input::Mode::Browser);
        cmds.push(Command::EnterBrowserMode);
        return;
    }

    // Tab - cycle focus
    if input.key_pressed(Key::Tab) {
        cmds.push(Command::CycleFocus);
    }

    // Crossfader: h/l or left/right
    if input.key_pressed(Key::H) && !input.modifiers.shift {
        cmds.push(Command::MoveCrossfader(Direction::Left));
    }
    if input.key_pressed(Key::L) && !input.modifiers.shift {
        cmds.push(Command::MoveCrossfader(Direction::Right));
    }
    if input.key_pressed(Key::ArrowLeft) {
        cmds.push(Command::MoveCrossfader(Direction::Left));
    }
    if input.key_pressed(Key::ArrowRight) {
        cmds.push(Command::MoveCrossfader(Direction::Right));
    }
    if input.key_pressed(Key::Backslash) {
        cmds.push(Command::CenterCrossfader);
    }

    // Deck A transport
    if input.key_pressed(Key::A) && !input.modifiers.shift {
        cmds.push(Command::Toggle(DeckId::A));
    }
    if input.key_pressed(Key::S) && !input.modifiers.shift {
        cmds.push(Command::Pause(DeckId::A));
    }
    if input.key_pressed(Key::Z) && !input.modifiers.shift {
        cmds.push(Command::Stop(DeckId::A));
    }

    // Deck B transport (shifted)
    if input.key_pressed(Key::A) && input.modifiers.shift {
        cmds.push(Command::Toggle(DeckId::B));
    }
    if input.key_pressed(Key::S) && input.modifiers.shift {
        cmds.push(Command::Pause(DeckId::B));
    }
    if input.key_pressed(Key::Z) && input.modifiers.shift {
        cmds.push(Command::Stop(DeckId::B));
    }

    // Nudge
    if input.key_pressed(Key::X) && !input.modifiers.shift {
        cmds.push(Command::Nudge(DeckId::A, -0.02));
    }
    if input.key_pressed(Key::C) && !input.modifiers.shift {
        cmds.push(Command::Nudge(DeckId::A, 0.02));
    }
    if input.key_pressed(Key::X) && input.modifiers.shift {
        cmds.push(Command::Nudge(DeckId::B, -0.02));
    }
    if input.key_pressed(Key::C) && input.modifiers.shift {
        cmds.push(Command::Nudge(DeckId::B, 0.02));
    }

    // Beatjump on focused deck
    let fd = focused_deck(state);
    if input.key_pressed(Key::J) && !input.modifiers.shift {
        cmds.push(Command::Beatjump(fd, -1));
    }
    if input.key_pressed(Key::K) && !input.modifiers.shift {
        cmds.push(Command::Beatjump(fd, 1));
    }
    if input.key_pressed(Key::ArrowDown) {
        cmds.push(Command::Beatjump(fd, -4));
    }
    if input.key_pressed(Key::ArrowUp) {
        cmds.push(Command::Beatjump(fd, 4));
    }
    if input.key_pressed(Key::J) && input.modifiers.shift {
        cmds.push(Command::Beatjump(fd, -8));
    }
    if input.key_pressed(Key::K) && input.modifiers.shift {
        cmds.push(Command::Beatjump(fd, 8));
    }

    // Beat nudge
    if input.key_pressed(Key::D) && !input.modifiers.shift {
        cmds.push(Command::BeatNudge(fd, 0.0625));
    }
    if input.key_pressed(Key::D) && input.modifiers.shift {
        cmds.push(Command::BeatNudge(fd, -0.0625));
    }

    // Tempo A: [ ] { }
    if input.key_pressed(Key::OpenBracket) && !input.modifiers.shift {
        cmds.push(Command::AdjustTempo(DeckId::A, -0.01));
    }
    if input.key_pressed(Key::CloseBracket) && !input.modifiers.shift {
        cmds.push(Command::AdjustTempo(DeckId::A, 0.01));
    }
    if input.key_pressed(Key::OpenBracket) && input.modifiers.shift {
        cmds.push(Command::AdjustTempo(DeckId::A, -0.1));
    }
    if input.key_pressed(Key::CloseBracket) && input.modifiers.shift {
        cmds.push(Command::AdjustTempo(DeckId::A, 0.1));
    }

    // Tempo B: , . < >
    if input.key_pressed(Key::Comma) && !input.modifiers.shift {
        cmds.push(Command::AdjustTempo(DeckId::B, -0.01));
    }
    if input.key_pressed(Key::Period) && !input.modifiers.shift {
        cmds.push(Command::AdjustTempo(DeckId::B, 0.01));
    }
    if input.key_pressed(Key::Comma) && input.modifiers.shift {
        cmds.push(Command::AdjustTempo(DeckId::B, -0.1));
    }
    if input.key_pressed(Key::Period) && input.modifiers.shift {
        cmds.push(Command::AdjustTempo(DeckId::B, 0.1));
    }

    // Sync
    if input.key_pressed(Key::B) && !input.modifiers.shift {
        cmds.push(Command::Sync(DeckId::B));
    }
    if input.key_pressed(Key::B) && input.modifiers.shift {
        cmds.push(Command::Sync(DeckId::A));
    }

    // Gain A: - =
    if input.key_pressed(Key::Minus) && !input.modifiers.shift {
        cmds.push(Command::AdjustGain(DeckId::A, -0.05));
    }
    if input.key_pressed(Key::Equals) && !input.modifiers.shift {
        cmds.push(Command::AdjustGain(DeckId::A, 0.05));
    }
    // Gain B: _ +
    if input.key_pressed(Key::Minus) && input.modifiers.shift {
        cmds.push(Command::AdjustGain(DeckId::B, -0.05));
    }
    if input.key_pressed(Key::Equals) && input.modifiers.shift {
        cmds.push(Command::AdjustGain(DeckId::B, 0.05));
    }

    // Scope toggle
    if input.key_pressed(Key::V) && !input.modifiers.shift {
        cmds.push(Command::ToggleScope);
    }
    if input.key_pressed(Key::V) && input.modifiers.shift {
        cmds.push(Command::CycleScopeMode);
    }

    // Waveform zoom
    if input.key_pressed(Key::W) && !input.modifiers.shift {
        cmds.push(Command::ZoomIn(fd));
    }
    if input.key_pressed(Key::W) && input.modifiers.shift {
        cmds.push(Command::ZoomOut(fd));
    }

    // Mastering
    if input.key_pressed(Key::P) && !input.modifiers.shift {
        cmds.push(Command::CycleMasteringPreset);
    }
    if input.key_pressed(Key::P) && input.modifiers.shift {
        cmds.push(Command::ToggleMastering);
    }

    // Cue points 1-8
    for (i, key) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4,
                     Key::Num5, Key::Num6, Key::Num7, Key::Num8].iter().enumerate() {
        if input.key_pressed(*key) && !input.modifiers.shift {
            cmds.push(Command::JumpCue(fd, (i + 1) as u8));
        }
        if input.key_pressed(*key) && input.modifiers.shift {
            cmds.push(Command::SetCue(fd, (i + 1) as u8));
        }
    }
}

fn handle_command_mode(
    input: &egui::InputState,
    state: &mut GuiState,
    cmds: &mut Vec<Command>,
) {
    if input.key_pressed(Key::Escape) {
        state.set_mode(ole_input::Mode::Normal);
        state.command_buffer.clear();
        cmds.push(Command::Cancel);
        return;
    }

    if input.key_pressed(Key::Enter) {
        let buffer = state.command_buffer.clone();
        state.set_mode(ole_input::Mode::Normal);

        // Parse command
        let parts: Vec<&str> = buffer.split_whitespace().collect();
        match parts.first().copied() {
            Some("q") | Some("quit") => cmds.push(Command::Quit),
            Some("help") => cmds.push(Command::ToggleHelp),
            Some("sync") => {
                // Sync: if a deck arg provided, sync that deck; otherwise sync focused
                if parts.len() > 1 {
                    match parts[1] {
                        "a" | "A" => cmds.push(Command::Sync(DeckId::A)),
                        "b" | "B" => cmds.push(Command::Sync(DeckId::B)),
                        _ => cmds.push(Command::Sync(focused_deck(state))),
                    }
                } else {
                    cmds.push(Command::Sync(focused_deck(state)));
                }
            }
            Some("lib") | Some("library") => cmds.push(Command::LibraryToggle),
            Some("rescan") => cmds.push(Command::LibraryRescan),
            Some("scan") => {
                if parts.len() > 1 {
                    let path = parts[1..].join(" ");
                    cmds.push(Command::LibraryScan(std::path::PathBuf::from(path)));
                } else {
                    state.set_error("Usage: :scan <directory>");
                }
            }
            Some("load") => {
                if parts.len() > 2 {
                    let deck = match parts[1] {
                        "a" | "A" => Some(DeckId::A),
                        "b" | "B" => Some(DeckId::B),
                        _ => None,
                    };
                    if let Some(deck) = deck {
                        let path = parts[2..].join(" ");
                        cmds.push(Command::LoadTrack(deck, std::path::PathBuf::from(path)));
                    }
                }
            }
            _ => {
                if !buffer.is_empty() {
                    state.set_error(format!("Unknown command: {}", buffer));
                }
            }
        }
        state.command_buffer.clear();
        return;
    }

    if input.key_pressed(Key::Backspace) {
        state.command_buffer.pop();
        return;
    }

    // Collect text input
    for event in &input.events {
        if let egui::Event::Text(text) = event {
            state.command_buffer.push_str(text);
        }
    }
}

fn handle_effects_mode(
    input: &egui::InputState,
    state: &mut GuiState,
    cmds: &mut Vec<Command>,
) {
    if input.key_pressed(Key::Escape) {
        state.set_mode(ole_input::Mode::Normal);
        cmds.push(Command::Cancel);
        return;
    }

    let fd = focused_deck(state);

    // Quick toggles
    if input.key_pressed(Key::T) && !input.modifiers.shift {
        cmds.push(Command::TriggerTapeStop(fd));
    }
    if input.key_pressed(Key::T) && input.modifiers.shift {
        cmds.push(Command::TriggerTapeStart(fd));
    }
    if input.key_pressed(Key::G) && !input.modifiers.shift {
        cmds.push(Command::ToggleFlanger(fd));
    }
    if input.key_pressed(Key::C) && !input.modifiers.shift {
        cmds.push(Command::ToggleBitcrusher(fd));
    }
    if input.key_pressed(Key::V) && !input.modifiers.shift {
        cmds.push(Command::ToggleVinyl(fd));
    }
    if input.key_pressed(Key::M) && !input.modifiers.shift {
        cmds.push(Command::CycleFilterMode(fd));
    }

    // Delay levels d0-d5
    // Delay levels d0-d5, Reverb levels r0-r5, Filter presets
    // These are complex multi-key sequences in the TUI.
    // Will be expanded in Phase 7 with proper modal state machine.
}

fn handle_help_mode(
    input: &egui::InputState,
    state: &mut GuiState,
    cmds: &mut Vec<Command>,
) {
    if input.key_pressed(Key::Escape) || input.key_pressed(Key::Q) {
        state.set_mode(ole_input::Mode::Normal);
        state.show_help = false;
        return;
    }
    if input.key_pressed(Key::J) || input.key_pressed(Key::ArrowDown) {
        cmds.push(Command::HelpScrollDown);
    }
    if input.key_pressed(Key::K) || input.key_pressed(Key::ArrowUp) {
        cmds.push(Command::HelpScrollUp);
    }
}

fn handle_browser_mode(
    input: &egui::InputState,
    state: &mut GuiState,
    cmds: &mut Vec<Command>,
) {
    if input.key_pressed(Key::Escape) {
        state.set_mode(ole_input::Mode::Normal);
        cmds.push(Command::Cancel);
        return;
    }

    // Navigation
    if input.key_pressed(Key::J) || input.key_pressed(Key::ArrowDown) {
        cmds.push(Command::LibrarySelectNext);
    }
    if input.key_pressed(Key::K) || input.key_pressed(Key::ArrowUp) {
        cmds.push(Command::LibrarySelectPrev);
    }
    if input.key_pressed(Key::G) && !input.modifiers.shift {
        cmds.push(Command::LibrarySelectFirst);
    }
    if input.key_pressed(Key::G) && input.modifiers.shift {
        cmds.push(Command::LibrarySelectLast);
    }

    // Load to deck
    if input.key_pressed(Key::A) && !input.modifiers.shift {
        cmds.push(Command::LibraryLoadToDeck(DeckId::A));
    }
    if input.key_pressed(Key::B) && !input.modifiers.shift {
        cmds.push(Command::LibraryLoadToDeck(DeckId::B));
    }
    if input.key_pressed(Key::Enter) {
        let fd = focused_deck(state);
        cmds.push(Command::LibraryLoadToDeck(fd));
    }

    // Filter
    if input.key_pressed(Key::F) && !input.modifiers.shift {
        cmds.push(Command::LibraryFilterCompatible);
    }
    if input.key_pressed(Key::C) && !input.modifiers.shift {
        cmds.push(Command::LibraryClearFilter);
    }
    if input.key_pressed(Key::L) && !input.modifiers.shift {
        cmds.push(Command::LibraryToggle);
    }
}
