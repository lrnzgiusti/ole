use egui::{Context, Key};

use ole_input::{Command, DeckId, Direction, EffectType};
use crate::state::{fx_cursor_effect_type, FocusedPane, FX_SLOT_COUNT, GuiState};

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

    // Tempo A: [ ] = ±0.1%, { } = ±1%, Alt+[ Alt+] = ±10%
    if input.key_pressed(Key::OpenBracket) && !input.modifiers.shift && !input.modifiers.alt {
        cmds.push(Command::AdjustTempo(DeckId::A, -0.001));
    }
    if input.key_pressed(Key::CloseBracket) && !input.modifiers.shift && !input.modifiers.alt {
        cmds.push(Command::AdjustTempo(DeckId::A, 0.001));
    }
    if input.key_pressed(Key::OpenBracket) && input.modifiers.shift && !input.modifiers.alt {
        cmds.push(Command::AdjustTempo(DeckId::A, -0.01));
    }
    if input.key_pressed(Key::CloseBracket) && input.modifiers.shift && !input.modifiers.alt {
        cmds.push(Command::AdjustTempo(DeckId::A, 0.01));
    }
    if input.key_pressed(Key::OpenBracket) && input.modifiers.alt && !input.modifiers.shift {
        cmds.push(Command::AdjustTempo(DeckId::A, -0.1));
    }
    if input.key_pressed(Key::CloseBracket) && input.modifiers.alt && !input.modifiers.shift {
        cmds.push(Command::AdjustTempo(DeckId::A, 0.1));
    }

    // Tempo B: , . = ±0.1%, < > = ±1%, Alt+, Alt+. = ±10%
    if input.key_pressed(Key::Comma) && !input.modifiers.shift && !input.modifiers.alt {
        cmds.push(Command::AdjustTempo(DeckId::B, -0.001));
    }
    if input.key_pressed(Key::Period) && !input.modifiers.shift && !input.modifiers.alt {
        cmds.push(Command::AdjustTempo(DeckId::B, 0.001));
    }
    if input.key_pressed(Key::Comma) && input.modifiers.shift && !input.modifiers.alt {
        cmds.push(Command::AdjustTempo(DeckId::B, -0.01));
    }
    if input.key_pressed(Key::Period) && input.modifiers.shift && !input.modifiers.alt {
        cmds.push(Command::AdjustTempo(DeckId::B, 0.01));
    }
    if input.key_pressed(Key::Comma) && input.modifiers.alt && !input.modifiers.shift {
        cmds.push(Command::AdjustTempo(DeckId::B, -0.1));
    }
    if input.key_pressed(Key::Period) && input.modifiers.alt && !input.modifiers.shift {
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

    // Copilot: Ctrl+P
    if input.key_pressed(Key::P) && input.modifiers.ctrl && !input.modifiers.shift {
        cmds.push(Command::ToggleCopilot);
    }

    // Mastering
    if input.key_pressed(Key::P) && !input.modifiers.shift && !input.modifiers.ctrl {
        cmds.push(Command::CycleMasteringPreset);
    }
    if input.key_pressed(Key::P) && input.modifiers.shift && !input.modifiers.ctrl {
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

    // Looping (focused deck)
    // F = auto-loop 4 beats, Shift+F = toggle loop on/off
    if input.key_pressed(Key::F) && !input.modifiers.shift {
        cmds.push(Command::AutoLoop(fd, 4.0));
    }
    if input.key_pressed(Key::F) && input.modifiers.shift {
        cmds.push(Command::ToggleLoop(fd));
    }
    // G = loop halve, Shift+G = loop double
    if input.key_pressed(Key::G) && !input.modifiers.shift {
        cmds.push(Command::LoopHalve(fd));
    }
    if input.key_pressed(Key::G) && input.modifiers.shift {
        cmds.push(Command::LoopDouble(fd));
    }
    // I = set loop in, U = set loop out
    if input.key_pressed(Key::I) && !input.modifiers.shift {
        cmds.push(Command::SetLoopIn(fd));
    }
    if input.key_pressed(Key::U) && !input.modifiers.shift {
        cmds.push(Command::SetLoopOut(fd));
    }
    // R = clear loop
    if input.key_pressed(Key::R) && !input.modifiers.shift {
        cmds.push(Command::ClearLoop(fd));
    }

    // Quantize: Q = toggle quantize, Shift+Q = cycle resolution
    if input.key_pressed(Key::Q) && !input.modifiers.shift {
        cmds.push(Command::ToggleQuantize(fd));
    }
    if input.key_pressed(Key::Q) && input.modifiers.shift {
        cmds.push(Command::CycleQuantizeResolution(fd));
    }

    // Key Lock: T = toggle key lock
    if input.key_pressed(Key::T) && !input.modifiers.shift {
        cmds.push(Command::ToggleKeyLock(fd));
    }

    // Slip Mode: Y = toggle slip
    if input.key_pressed(Key::Y) && !input.modifiers.shift {
        cmds.push(Command::ToggleSlip(fd));
    }

    // Recording: F9 toggle
    if input.key_pressed(Key::F9) {
        cmds.push(Command::ToggleRecording);
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
            Some("rec") | Some("record") => cmds.push(Command::ToggleRecording),
            Some("save") => {
                if parts.len() > 1 {
                    let path = parts[1..].join(" ");
                    cmds.push(Command::SaveRecording(std::path::PathBuf::from(path)));
                } else {
                    // Default to ~/ole_recording.wav
                    let path = dirs::home_dir()
                        .unwrap_or_else(|| std::path::PathBuf::from("."))
                        .join("ole_recording.wav");
                    cmds.push(Command::SaveRecording(path));
                }
            }
            Some("sample") => {
                // :sample <slot> <path> - load sample to slot
                if parts.len() > 2 {
                    if let Ok(slot) = parts[1].parse::<u8>() {
                        if (1..=8).contains(&slot) {
                            let path = parts[2..].join(" ");
                            cmds.push(Command::LoadSamplerSlot(slot - 1, std::path::PathBuf::from(path)));
                        } else {
                            state.set_error("Slot must be 1-8");
                        }
                    } else {
                        state.set_error("Usage: :sample <1-8> <path>");
                    }
                } else {
                    state.set_error("Usage: :sample <1-8> <path>");
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

    // Cursor navigation
    if input.key_pressed(Key::J) || input.key_pressed(Key::ArrowDown) {
        state.fx_cursor = (state.fx_cursor + 1) % FX_SLOT_COUNT;
        state.selected_effect = fx_cursor_effect_type(state.fx_cursor);
        return;
    }
    if input.key_pressed(Key::K) || input.key_pressed(Key::ArrowUp) {
        state.fx_cursor = state.fx_cursor.checked_sub(1).unwrap_or(FX_SLOT_COUNT - 1);
        state.selected_effect = fx_cursor_effect_type(state.fx_cursor);
        return;
    }

    // Enter toggles effect at cursor
    if input.key_pressed(Key::Enter) {
        match state.fx_cursor {
            1 => {} // EQ — no toggle (kills handled by Z/A/Q)
            12 => cmds.push(Command::ToggleVinyl(fd)),
            _ => {
                if let Some(et) = fx_cursor_effect_type(state.fx_cursor) {
                    cmds.push(Command::ToggleEffect(fd, et));
                }
            }
        }
        return;
    }

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
    // Mode cycling with 'm' — context-aware based on cursor
    if input.key_pressed(Key::M) && !input.modifiers.shift {
        match state.fx_cursor {
            0 => cmds.push(Command::CycleFilterMode(fd)),
            2 => cmds.push(Command::CycleDelayMode(fd)),
            _ => cmds.push(Command::CycleFilterMode(fd)), // default to filter for backward compat
        }
    }

    // New effects
    if input.key_pressed(Key::P) && !input.modifiers.shift {
        cmds.push(Command::TogglePhaser(fd));
    }
    if input.key_pressed(Key::X) && !input.modifiers.shift {
        cmds.push(Command::ToggleGate(fd));
    }
    if input.key_pressed(Key::R) && !input.modifiers.shift {
        cmds.push(Command::TriggerBeatRepeat(fd));
    }
    if input.key_pressed(Key::N) && !input.modifiers.shift {
        cmds.push(Command::ToggleRingMod(fd));
    }
    if input.key_pressed(Key::S) && !input.modifiers.shift {
        cmds.push(Command::ToggleShimmer(fd));
    }
    if input.key_pressed(Key::W) && !input.modifiers.shift {
        cmds.push(Command::ToggleWashOut(fd));
    }
    if input.key_pressed(Key::D) && !input.modifiers.shift {
        cmds.push(Command::CycleDelayMode(fd));
    }
    if input.key_pressed(Key::D) && input.modifiers.shift {
        cmds.push(Command::ToggleEffect(fd, EffectType::Delay));
    }
    if input.key_pressed(Key::R) && input.modifiers.shift {
        cmds.push(Command::ToggleEffect(fd, EffectType::Reverb));
    }

    // Filter toggle
    if input.key_pressed(Key::M) && input.modifiers.shift {
        cmds.push(Command::ToggleEffect(fd, EffectType::Filter));
    }

    // Dry/wet mix adjustment for cursor effect: ←/→
    if let Some(effect_type) = fx_cursor_effect_type(state.fx_cursor) {
        if input.key_pressed(Key::ArrowRight) {
            cmds.push(Command::AdjustEffectMix(fd, effect_type, 0.1));
        }
        if input.key_pressed(Key::ArrowLeft) {
            cmds.push(Command::AdjustEffectMix(fd, effect_type, -0.1));
        }
    }

    // Loop rolls in effects mode: 1-7 for different beat sizes
    if input.key_pressed(Key::Num1) && !input.modifiers.shift && !input.modifiers.ctrl {
        cmds.push(Command::AutoLoop(fd, 0.25));
    }
    if input.key_pressed(Key::Num2) && !input.modifiers.shift && !input.modifiers.ctrl {
        cmds.push(Command::AutoLoop(fd, 0.5));
    }
    if input.key_pressed(Key::Num3) && !input.modifiers.shift && !input.modifiers.ctrl {
        cmds.push(Command::AutoLoop(fd, 1.0));
    }
    if input.key_pressed(Key::Num4) && !input.modifiers.shift && !input.modifiers.ctrl {
        cmds.push(Command::AutoLoop(fd, 2.0));
    }
    if input.key_pressed(Key::Num5) && !input.modifiers.shift && !input.modifiers.ctrl {
        cmds.push(Command::AutoLoop(fd, 4.0));
    }
    if input.key_pressed(Key::Num6) && !input.modifiers.shift && !input.modifiers.ctrl {
        cmds.push(Command::AutoLoop(fd, 8.0));
    }
    if input.key_pressed(Key::Num7) && !input.modifiers.shift && !input.modifiers.ctrl {
        cmds.push(Command::AutoLoop(fd, 16.0));
    }

    // EQ kills in effects mode: Z/A/Q for low/mid/high kill
    if input.key_pressed(Key::Z) && !input.modifiers.shift {
        cmds.push(Command::KillEqLow(fd));
    }
    if input.key_pressed(Key::A) && !input.modifiers.shift {
        cmds.push(Command::KillEqMid(fd));
    }
    if input.key_pressed(Key::Q) && !input.modifiers.shift {
        cmds.push(Command::KillEqHigh(fd));
    }

    // Effect macros: F1-F4 for preset combos
    if input.key_pressed(Key::F1) {
        cmds.push(Command::TriggerMacro(fd, 1)); // Build Up
    }
    if input.key_pressed(Key::F2) {
        cmds.push(Command::TriggerMacro(fd, 2)); // Drop
    }
    if input.key_pressed(Key::F3) {
        cmds.push(Command::TriggerMacro(fd, 3)); // Dub Echo
    }
    if input.key_pressed(Key::F4) {
        cmds.push(Command::TriggerMacro(fd, 4)); // Glitch
    }

    // Sampler triggers: Shift+1-8 to trigger, Ctrl+1-8 to stop
    for (i, key) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4,
                     Key::Num5, Key::Num6, Key::Num7, Key::Num8].iter().enumerate() {
        if input.key_pressed(*key) && input.modifiers.shift && !input.modifiers.ctrl {
            cmds.push(Command::TriggerSampler(i as u8));
        }
        if input.key_pressed(*key) && input.modifiers.ctrl && !input.modifiers.shift {
            cmds.push(Command::StopSampler(i as u8));
        }
    }

    // Recording: F9 toggle
    if input.key_pressed(Key::F9) {
        cmds.push(Command::ToggleRecording);
    }
}

fn handle_help_mode(
    input: &egui::InputState,
    state: &mut GuiState,
    cmds: &mut Vec<Command>,
) {
    if input.key_pressed(Key::Escape) || input.key_pressed(Key::Q) || input.key_pressed(Key::Questionmark) {
        state.set_mode(ole_input::Mode::Normal);
        state.show_help = false;
        cmds.push(Command::Cancel);
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
    // If in search sub-mode, all typing goes to search query
    if state.library_search_active {
        if input.key_pressed(Key::Escape) {
            state.library_search_active = false;
            cmds.push(Command::LibrarySearchClear);
            return;
        }
        if input.key_pressed(Key::Enter) {
            state.library_search_active = false;
            return;
        }
        if input.key_pressed(Key::Backspace) {
            cmds.push(Command::LibrarySearchBackspace);
            return;
        }
        // Arrow keys still navigate
        if input.key_pressed(Key::ArrowDown) {
            cmds.push(Command::LibrarySelectNext);
            return;
        }
        if input.key_pressed(Key::ArrowUp) {
            cmds.push(Command::LibrarySelectPrev);
            return;
        }
        // Collect text input for search
        for event in &input.events {
            if let egui::Event::Text(text) = event {
                for c in text.chars() {
                    cmds.push(Command::LibrarySearchAppend(c));
                }
            }
        }
        return;
    }

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

    // Page navigation
    if input.key_pressed(Key::D) && input.modifiers.ctrl {
        cmds.push(Command::LibraryPageDown);
    }
    if input.key_pressed(Key::U) && input.modifiers.ctrl {
        cmds.push(Command::LibraryPageUp);
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

    // Sort
    if input.key_pressed(Key::S) && !input.modifiers.shift {
        cmds.push(Command::LibraryCycleSort);
    }
    if input.key_pressed(Key::S) && input.modifiers.shift {
        cmds.push(Command::LibraryReverseSort);
    }

    // Energy direction (copilot)
    if input.key_pressed(Key::E) && !input.modifiers.shift {
        cmds.push(Command::CycleEnergyDirection);
    }

    // Search: '/' enters search sub-mode
    if input.key_pressed(Key::Slash) {
        state.library_search_active = true;
        cmds.push(Command::LibrarySearchClear);
    }

    // History toggle
    if input.key_pressed(Key::H) && !input.modifiers.shift {
        cmds.push(Command::LibraryShowHistory);
    }
}
