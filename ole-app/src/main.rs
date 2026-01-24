//! OLE - Open Live Engine
//!
//! Terminal-based DJ application with vintage CRT aesthetic.

use std::io::{self, stdout};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::Receiver;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    Terminal,
};

use ole_audio::{AudioCommand, AudioEngine, AudioEvent, EngineState};
use ole_input::{Command, DeckId, Direction, EffectType, InputHandler};
use ole_library::{AnalysisCache, Config, LibraryScanner, ScanConfig, ScanProgress, TrackLoader};
use ole_tui::{
    App, CrossfaderWidget, DeckWidget, FocusedPane, HelpWidget, LibraryWidget, MasterVuMeterWidget,
    PhaseWidget, ScopeWidget, SpectrumWidget, StatusBarWidget, Theme,
};

/// Frame rate for UI updates
const FPS: u64 = 30;

fn main() -> anyhow::Result<()> {
    // Initialize terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // Create audio channels
    let (cmd_tx, cmd_rx, evt_tx, evt_rx) = AudioEngine::create_channels();

    // Shutdown flag
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_audio = shutdown.clone();

    // Spawn audio thread
    let audio_handle = thread::spawn(move || {
        run_audio_thread(cmd_rx, evt_tx, shutdown_audio);
    });

    // Create engine handle for main thread
    let engine = AudioEngine::new(cmd_tx, evt_rx);

    // Run main event loop
    let result = run_app(&mut terminal, engine, shutdown.clone());

    // Cleanup
    shutdown.store(true, Ordering::SeqCst);
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    // Wait for audio thread
    let _ = audio_handle.join();

    result
}

fn run_audio_thread(
    cmd_rx: Receiver<AudioCommand>,
    evt_tx: crossbeam_channel::Sender<AudioEvent>,
    shutdown: Arc<AtomicBool>,
) {
    // Get audio host and device
    let host = cpal::default_host();
    let device = match host.default_output_device() {
        Some(d) => d,
        None => {
            let _ = evt_tx.send(AudioEvent::Error("No audio output device found".into()));
            return;
        }
    };

    let config = match device.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            let _ = evt_tx.send(AudioEvent::Error(format!(
                "Failed to get audio config: {}",
                e
            )));
            return;
        }
    };

    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;

    // Create engine state
    let engine_state = Arc::new(std::sync::Mutex::new(EngineState::new(sample_rate)));
    let engine_for_callback = engine_state.clone();

    // Pre-allocate mono conversion buffer (avoid allocation in audio callback)
    // Max buffer size for typical audio: 8192 samples stereo = 16384 floats
    let mut mono_conversion_buffer = vec![0.0f32; 16384];

    // State update interval
    let mut last_state_update = Instant::now();
    let state_update_interval = Duration::from_millis(33); // ~30fps

    // Build audio stream
    let stream = device.build_output_stream(
        &config.into(),
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            // Use try_lock to avoid blocking the real-time audio thread
            // On contention (rare), output silence rather than blocking
            if let Ok(mut state) = engine_for_callback.try_lock() {
                if channels == 2 {
                    state.process(data);
                } else {
                    // Handle mono output using pre-allocated buffer
                    let stereo_len = data.len() * 2;
                    let stereo = &mut mono_conversion_buffer[..stereo_len];
                    state.process(stereo);
                    for (i, sample) in data.iter_mut().enumerate() {
                        *sample = (stereo[i * 2] + stereo[i * 2 + 1]) * 0.5;
                    }
                }
            } else {
                // Lock contention - output silence to avoid blocking
                data.fill(0.0);
            }
        },
        |err| {
            eprintln!("Audio stream error: {}", err);
        },
        None,
    );

    let stream = match stream {
        Ok(s) => s,
        Err(e) => {
            let _ = evt_tx.send(AudioEvent::Error(format!(
                "Failed to create audio stream: {}",
                e
            )));
            return;
        }
    };

    if let Err(e) = stream.play() {
        let _ = evt_tx.send(AudioEvent::Error(format!("Failed to start audio: {}", e)));
        return;
    }

    // Command processing loop
    while !shutdown.load(Ordering::Relaxed) {
        // Process commands
        match cmd_rx.recv_timeout(Duration::from_millis(10)) {
            Ok(AudioCommand::Shutdown) => break,
            Ok(cmd) => {
                if let Ok(mut state) = engine_state.lock() {
                    state.handle_command(cmd);
                }
            }
            Err(_) => {}
        }

        // Send state updates periodically
        if last_state_update.elapsed() >= state_update_interval {
            if let Ok(state) = engine_state.lock() {
                let _ = evt_tx.try_send(state.get_state());
            }
            last_state_update = Instant::now();
        }
    }
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    engine: AudioEngine,
    shutdown: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let mut app = App::new();
    let mut input_handler = InputHandler::new();
    let track_loader = TrackLoader::new();

    // Load user config (last scan folder, etc.)
    let mut config = Config::load();

    // Initialize library cache and scanner
    let cache_path = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ole")
        .join("library.db");
    let cache = AnalysisCache::open(&cache_path).ok();
    let scanner = cache.map(LibraryScanner::new);

    // Load cached tracks on startup if we have a last scan folder
    if config.last_scan_folder.is_some() {
        if let Some(ref scanner) = scanner {
            if let Ok(tracks) = scanner.get_all_tracks() {
                if !tracks.is_empty() {
                    app.state.library.set_tracks(tracks);
                }
            }
        }
    }

    // Track scan progress receiver and current scan folder for saving to config
    let mut scan_progress_rx: Option<crossbeam_channel::Receiver<ScanProgress>> = None;
    let mut current_scan_folder: Option<PathBuf> = None;

    let frame_duration = Duration::from_millis(1000 / FPS);
    let mut last_frame = Instant::now();

    // Show startup banner with track count if we loaded cached tracks
    let track_count = app.state.library.tracks.len();
    if track_count > 0 {
        app.state.set_message(format!(
            "OLE - Loaded {} tracks | Press ? for help, / for library",
            track_count
        ));
    } else {
        app.state.set_message(
            "OLE - Open Live Engine | Press ? for help, / for library, :scan <dir> to scan tracks",
        );
    }

    loop {
        // Check for shutdown
        if shutdown.load(Ordering::Relaxed) || app.should_quit {
            engine.send(AudioCommand::Shutdown);
            break;
        }

        // Process audio events
        while let Ok(event) = engine.event_rx.try_recv() {
            app.state.handle_audio_event(event);
        }

        // Process scan progress updates
        let mut scan_complete = false;
        if let Some(ref rx) = scan_progress_rx {
            while let Ok(progress) = rx.try_recv() {
                match progress {
                    ScanProgress::Started { total } => {
                        app.state.library.is_scanning = true;
                        app.state.library.scan_progress = (0, total);
                        app.state
                            .set_message(format!("Scanning {} files...", total));
                    }
                    ScanProgress::Analyzing { current, total, .. } => {
                        app.state.library.scan_progress = (current, total);
                    }
                    ScanProgress::Cached { current, total, .. } => {
                        app.state.library.scan_progress = (current, total);
                    }
                    ScanProgress::Complete {
                        analyzed,
                        cached,
                        failed,
                    } => {
                        app.state.library.is_scanning = false;
                        // Load all tracks from scanner
                        if let Some(ref scanner) = scanner {
                            if let Ok(tracks) = scanner.get_all_tracks() {
                                app.state.library.set_tracks(tracks);
                            }
                        }
                        // Save the scanned folder to config for next startup
                        if let Some(ref folder) = current_scan_folder {
                            config.last_scan_folder = Some(folder.clone());
                            let _ = config.save(); // Best effort, don't fail on config save error
                        }
                        current_scan_folder = None;
                        app.state.set_success(format!(
                            "Scan complete: {} analyzed, {} cached, {} failed",
                            analyzed, cached, failed
                        ));
                        scan_complete = true;
                    }
                    ScanProgress::Error { .. } => {
                        // Log but continue
                    }
                }
            }
        }
        if scan_complete {
            scan_progress_rx = None;
        }

        // Increment frame counter for animations
        app.state.frame_count = app.state.frame_count.wrapping_add(1);

        // Update beat pulse animation (for beat phase dots)
        app.state.update_beat_pulse();

        // Update sync quality for steady border glow when decks are phase-locked
        app.state.update_sync_quality();

        // Update CRT visual effects (phosphor afterglow, peak hold, etc.)
        app.state.update_crt_effects();

        // Render
        terminal.draw(|frame| {
            render_ui(frame, &mut app);
        })?;

        // Handle input
        let timeout = frame_duration.saturating_sub(last_frame.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                // Handle quit shortcut
                if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    app.quit();
                    continue;
                }

                // Process through input handler
                if let Some(cmd) = input_handler.handle_key(key) {
                    // Handle library commands that need special access
                    match &cmd {
                        Command::LibraryScan(path) => {
                            if let Some(ref scanner) = scanner {
                                let scan_config = ScanConfig {
                                    directory: path.clone(),
                                    ..Default::default()
                                };
                                let (rx, _handle) = scanner.scan_async(scan_config);
                                scan_progress_rx = Some(rx);
                                current_scan_folder = Some(path.clone());
                                app.state.library.is_scanning = true;
                                app.state
                                    .set_message(format!("Starting scan of {}...", path.display()));
                            } else {
                                app.state.set_error("Library cache not available");
                            }
                        }
                        Command::LibraryLoadToDeck(deck) => {
                            if let Some(track) = app.state.library.selected_track() {
                                let path = track.path.clone();
                                let key = track.key.clone();
                                load_track_with_key(
                                    &mut app,
                                    &engine,
                                    &track_loader,
                                    *deck,
                                    &path,
                                    key.clone(),
                                );
                                // Update current playing key for harmonic highlighting
                                app.state.library.current_playing_key = key;
                            }
                        }
                        _ => {
                            handle_command(&mut app, &engine, &track_loader, cmd);
                        }
                    }
                }

                // Update mode in app state
                app.state.set_mode(input_handler.mode());
                app.state.command_buffer = input_handler.command_buffer().to_string();

                // Sync focused deck with input handler for effects targeting
                let focused_deck = match app.state.focused {
                    FocusedPane::DeckA => DeckId::A,
                    FocusedPane::DeckB => DeckId::B,
                    _ => DeckId::A, // Default to A if crossfader/effects focused
                };
                input_handler.set_focused_deck(focused_deck);
            }
        }

        // Maintain frame rate
        let elapsed = last_frame.elapsed();
        if elapsed < frame_duration {
            thread::sleep(frame_duration - elapsed);
        }
        last_frame = Instant::now();
    }

    Ok(())
}

fn handle_command(app: &mut App, engine: &AudioEngine, loader: &TrackLoader, cmd: Command) {
    match cmd {
        // Transport
        Command::Play(DeckId::A) => engine.send(AudioCommand::PlayA),
        Command::Play(DeckId::B) => engine.send(AudioCommand::PlayB),
        Command::Pause(DeckId::A) => engine.send(AudioCommand::PauseA),
        Command::Pause(DeckId::B) => engine.send(AudioCommand::PauseB),
        Command::Stop(DeckId::A) => engine.send(AudioCommand::StopA),
        Command::Stop(DeckId::B) => engine.send(AudioCommand::StopB),
        Command::Toggle(DeckId::A) => engine.send(AudioCommand::ToggleA),
        Command::Toggle(DeckId::B) => engine.send(AudioCommand::ToggleB),

        // Seeking
        Command::Seek(DeckId::A, pos) => engine.send(AudioCommand::SeekA(pos)),
        Command::Seek(DeckId::B, pos) => engine.send(AudioCommand::SeekB(pos)),
        Command::Nudge(DeckId::A, delta) => engine.send(AudioCommand::NudgeA(delta)),
        Command::Nudge(DeckId::B, delta) => engine.send(AudioCommand::NudgeB(delta)),
        Command::BeatNudge(DeckId::A, beats) => engine.send(AudioCommand::BeatNudgeA(beats)),
        Command::BeatNudge(DeckId::B, beats) => engine.send(AudioCommand::BeatNudgeB(beats)),

        // Beatjump
        Command::Beatjump(DeckId::A, beats) => engine.send(AudioCommand::BeatjumpA(beats)),
        Command::Beatjump(DeckId::B, beats) => engine.send(AudioCommand::BeatjumpB(beats)),

        // Cue points
        Command::SetCue(DeckId::A, num) => {
            engine.send(AudioCommand::SetCueA(num));
            app.state.set_success(format!("▶ Deck A CUE {} set", num));
        }
        Command::SetCue(DeckId::B, num) => {
            engine.send(AudioCommand::SetCueB(num));
            app.state.set_success(format!("▶ Deck B CUE {} set", num));
        }
        Command::JumpCue(DeckId::A, num) => engine.send(AudioCommand::JumpCueA(num)),
        Command::JumpCue(DeckId::B, num) => engine.send(AudioCommand::JumpCueB(num)),

        // Tempo
        Command::SetTempo(DeckId::A, tempo) => engine.send(AudioCommand::SetTempoA(tempo)),
        Command::SetTempo(DeckId::B, tempo) => engine.send(AudioCommand::SetTempoB(tempo)),
        Command::AdjustTempo(DeckId::A, delta) => engine.send(AudioCommand::AdjustTempoA(delta)),
        Command::AdjustTempo(DeckId::B, delta) => engine.send(AudioCommand::AdjustTempoB(delta)),

        // Gain
        Command::SetGain(DeckId::A, gain) => engine.send(AudioCommand::SetGainA(gain)),
        Command::SetGain(DeckId::B, gain) => engine.send(AudioCommand::SetGainB(gain)),
        Command::AdjustGain(DeckId::A, delta) => engine.send(AudioCommand::AdjustGainA(delta)),
        Command::AdjustGain(DeckId::B, delta) => engine.send(AudioCommand::AdjustGainB(delta)),

        // Sync
        Command::Sync(DeckId::A) => engine.send(AudioCommand::SyncAToB),
        Command::Sync(DeckId::B) => engine.send(AudioCommand::SyncBToA),

        // Crossfader
        Command::SetCrossfader(pos) => engine.send(AudioCommand::SetCrossfader(pos)),
        Command::MoveCrossfader(Direction::Left) => engine.send(AudioCommand::MoveCrossfader(-0.1)),
        Command::MoveCrossfader(Direction::Right) => engine.send(AudioCommand::MoveCrossfader(0.1)),
        Command::MoveCrossfader(_) => {} // Up/Down not used for crossfader
        Command::CenterCrossfader => engine.send(AudioCommand::CenterCrossfader),

        // Effects - toggle
        Command::ToggleEffect(DeckId::A, EffectType::Filter) => {
            engine.send(AudioCommand::ToggleFilterA)
        }
        Command::ToggleEffect(DeckId::B, EffectType::Filter) => {
            engine.send(AudioCommand::ToggleFilterB)
        }
        Command::ToggleEffect(DeckId::A, EffectType::Delay) => {
            engine.send(AudioCommand::ToggleDelayA)
        }
        Command::ToggleEffect(DeckId::B, EffectType::Delay) => {
            engine.send(AudioCommand::ToggleDelayB)
        }
        Command::ToggleEffect(DeckId::A, EffectType::Reverb) => {
            engine.send(AudioCommand::ToggleReverbA)
        }
        Command::ToggleEffect(DeckId::B, EffectType::Reverb) => {
            engine.send(AudioCommand::ToggleReverbB)
        }
        Command::ToggleEffect(DeckId::A, EffectType::TapeStop) => {
            engine.send(AudioCommand::ToggleTapeStopA)
        }
        Command::ToggleEffect(DeckId::B, EffectType::TapeStop) => {
            engine.send(AudioCommand::ToggleTapeStopB)
        }
        Command::ToggleEffect(DeckId::A, EffectType::Flanger) => {
            engine.send(AudioCommand::ToggleFlangerA)
        }
        Command::ToggleEffect(DeckId::B, EffectType::Flanger) => {
            engine.send(AudioCommand::ToggleFlangerB)
        }
        Command::ToggleEffect(DeckId::A, EffectType::Bitcrusher) => {
            engine.send(AudioCommand::ToggleBitcrusherA)
        }
        Command::ToggleEffect(DeckId::B, EffectType::Bitcrusher) => {
            engine.send(AudioCommand::ToggleBitcrusherB)
        }
        Command::AdjustFilterCutoff(DeckId::A, delta) => {
            engine.send(AudioCommand::AdjustFilterCutoffA(delta))
        }
        Command::AdjustFilterCutoff(DeckId::B, delta) => {
            engine.send(AudioCommand::AdjustFilterCutoffB(delta))
        }

        // Effects - preset levels (with feedback)
        Command::SetDelayLevel(deck, level) => {
            let deck_char = match deck {
                DeckId::A => 'A',
                DeckId::B => 'B',
            };
            match deck {
                DeckId::A => engine.send(AudioCommand::SetDelayLevelA(level)),
                DeckId::B => engine.send(AudioCommand::SetDelayLevelB(level)),
            }
            if level == 0 {
                app.state
                    .set_message(format!("▶ Deck {} DELAY OFF", deck_char));
            } else {
                app.state
                    .set_message(format!("▶ Deck {} DELAY:{}", deck_char, level));
            }
        }
        Command::SetFilterPreset(deck, filter_type, level) => {
            let deck_char = match deck {
                DeckId::A => 'A',
                DeckId::B => 'B',
            };
            let ft = match filter_type {
                ole_audio::FilterType::LowPass => "LOW",
                ole_audio::FilterType::BandPass => "BAND",
                ole_audio::FilterType::HighPass => "HIGH",
            };
            match deck {
                DeckId::A => engine.send(AudioCommand::SetFilterPresetA(filter_type, level)),
                DeckId::B => engine.send(AudioCommand::SetFilterPresetB(filter_type, level)),
            }
            if level == 0 {
                app.state
                    .set_message(format!("▶ Deck {} FILTER OFF", deck_char));
            } else {
                app.state
                    .set_message(format!("▶ Deck {} FILTER:{}:{}", deck_char, ft, level));
            }
        }
        Command::SetReverbLevel(deck, level) => {
            let deck_char = match deck {
                DeckId::A => 'A',
                DeckId::B => 'B',
            };
            match deck {
                DeckId::A => engine.send(AudioCommand::SetReverbLevelA(level)),
                DeckId::B => engine.send(AudioCommand::SetReverbLevelB(level)),
            }
            if level == 0 {
                app.state
                    .set_message(format!("▶ Deck {} REVERB OFF", deck_char));
            } else {
                app.state
                    .set_message(format!("▶ Deck {} REVERB:{}", deck_char, level));
            }
        }

        // Load tracks
        Command::LoadTrack(deck, path) => {
            load_track_with_key(app, engine, loader, deck, &path, None);
        }

        // UI commands
        Command::ToggleHelp => app.state.toggle_help(),
        Command::ToggleScope => app.state.toggle_scope(),
        Command::CycleScopeMode => app.state.cycle_scope_mode(),
        Command::ZoomIn(deck) => match deck {
            DeckId::A => app.state.zoom_a = app.state.zoom_a.zoom_in(),
            DeckId::B => app.state.zoom_b = app.state.zoom_b.zoom_in(),
        },
        Command::ZoomOut(deck) => match deck {
            DeckId::A => app.state.zoom_a = app.state.zoom_a.zoom_out(),
            DeckId::B => app.state.zoom_b = app.state.zoom_b.zoom_out(),
        },
        Command::SetTheme(name) => app.state.set_theme(&name),
        Command::CycleFocus => app.state.cycle_focus(),
        Command::Focus(deck) => {
            app.state.focus(match deck {
                DeckId::A => FocusedPane::DeckA,
                DeckId::B => FocusedPane::DeckB,
            });
        }
        Command::Quit => app.quit(),

        // Library commands
        Command::LibrarySelectNext => app.state.library.select_next(),
        Command::LibrarySelectPrev => app.state.library.select_prev(),
        Command::LibrarySelectFirst => app.state.library.select_first(),
        Command::LibrarySelectLast => app.state.library.select_last(),
        Command::LibraryFilterByKey(key) => app.state.library.set_filter(Some(key)),
        Command::LibraryFilterByBpmRange(min, max) => {
            // For now, just jump to min BPM - could add range filter later
            if app.state.library.jump_to_bpm(min) {
                app.state.set_message(format!("◈ BPM {}-{}", min, max));
            }
        }
        Command::LibraryFilterCompatible => {
            app.state.library.filter_compatible();
            app.state.set_message("♫ Showing compatible keys");
        }
        Command::LibraryClearFilter => {
            app.state.library.set_filter(None);
            app.state.set_message("◎ Filter cleared");
        }
        Command::LibraryToggle => app.state.toggle_library(),
        Command::LibraryJumpToKey(pos, is_minor) => {
            let key_str = format!("{}{}", pos, if is_minor { 'A' } else { 'B' });
            if app.state.library.jump_to_key(pos, is_minor) {
                app.state.set_message(format!("→ Key {}", key_str));
            } else {
                app.state.set_warning(format!("No tracks in {}", key_str));
            }
        }
        Command::LibraryJumpToBpm(bpm) => {
            if app.state.library.jump_to_bpm(bpm) {
                app.state.set_message(format!("→ ~{} BPM", bpm));
            } else {
                app.state.set_warning(format!("No tracks near {} BPM", bpm));
            }
        }

        // LibraryScan and LibraryLoadToDeck are handled specially in main loop
        Command::LibraryScan(_) | Command::LibraryLoadToDeck(_) => {}

        // Filter mode commands
        Command::SetFilterMode(DeckId::A, mode) => engine.send(AudioCommand::SetFilterModeA(mode)),
        Command::SetFilterMode(DeckId::B, mode) => engine.send(AudioCommand::SetFilterModeB(mode)),
        Command::CycleFilterMode(DeckId::A) => {
            // Cycle through Biquad -> Ladder -> SVF -> Biquad
            let next = match app.state.filter_a_mode {
                ole_audio::FilterMode::Biquad => ole_audio::FilterMode::Ladder,
                ole_audio::FilterMode::Ladder => ole_audio::FilterMode::SVF,
                ole_audio::FilterMode::SVF => ole_audio::FilterMode::Biquad,
            };
            engine.send(AudioCommand::SetFilterModeA(next));
        }
        Command::CycleFilterMode(DeckId::B) => {
            let next = match app.state.filter_b_mode {
                ole_audio::FilterMode::Biquad => ole_audio::FilterMode::Ladder,
                ole_audio::FilterMode::Ladder => ole_audio::FilterMode::SVF,
                ole_audio::FilterMode::SVF => ole_audio::FilterMode::Biquad,
            };
            engine.send(AudioCommand::SetFilterModeB(next));
        }

        // Vinyl emulation commands
        Command::ToggleVinyl(DeckId::A) => engine.send(AudioCommand::ToggleVinylA),
        Command::ToggleVinyl(DeckId::B) => engine.send(AudioCommand::ToggleVinylB),
        Command::SetVinylPreset(DeckId::A, preset) => {
            let p = match preset {
                ole_input::VinylPresetId::Subtle => ole_audio::VinylPreset::Clean,
                ole_input::VinylPresetId::Warm => ole_audio::VinylPreset::Warm,
                ole_input::VinylPresetId::Classic => ole_audio::VinylPreset::Vintage,
                ole_input::VinylPresetId::Aged => ole_audio::VinylPreset::Worn,
                ole_input::VinylPresetId::LoFi => ole_audio::VinylPreset::Extreme,
            };
            engine.send(AudioCommand::SetVinylPresetA(p));
        }
        Command::SetVinylPreset(DeckId::B, preset) => {
            let p = match preset {
                ole_input::VinylPresetId::Subtle => ole_audio::VinylPreset::Clean,
                ole_input::VinylPresetId::Warm => ole_audio::VinylPreset::Warm,
                ole_input::VinylPresetId::Classic => ole_audio::VinylPreset::Vintage,
                ole_input::VinylPresetId::Aged => ole_audio::VinylPreset::Worn,
                ole_input::VinylPresetId::LoFi => ole_audio::VinylPreset::Extreme,
            };
            engine.send(AudioCommand::SetVinylPresetB(p));
        }
        Command::CycleVinylPreset(_) => {} // TODO: implement cycling
        Command::SetVinylWow(DeckId::A, amount) => engine.send(AudioCommand::SetVinylWowA(amount)),
        Command::SetVinylWow(DeckId::B, amount) => engine.send(AudioCommand::SetVinylWowB(amount)),
        Command::SetVinylNoise(DeckId::A, amount) => {
            engine.send(AudioCommand::SetVinylNoiseA(amount))
        }
        Command::SetVinylNoise(DeckId::B, amount) => {
            engine.send(AudioCommand::SetVinylNoiseB(amount))
        }
        Command::SetVinylWarmth(DeckId::A, amount) => {
            engine.send(AudioCommand::SetVinylWarmthA(amount))
        }
        Command::SetVinylWarmth(DeckId::B, amount) => {
            engine.send(AudioCommand::SetVinylWarmthB(amount))
        }

        // Time stretching commands
        Command::ToggleTimeStretch(DeckId::A) => engine.send(AudioCommand::ToggleTimeStretchA),
        Command::ToggleTimeStretch(DeckId::B) => engine.send(AudioCommand::ToggleTimeStretchB),
        Command::SetTimeStretchRatio(DeckId::A, ratio) => {
            engine.send(AudioCommand::SetTimeStretchRatioA(ratio))
        }
        Command::SetTimeStretchRatio(DeckId::B, ratio) => {
            engine.send(AudioCommand::SetTimeStretchRatioB(ratio))
        }

        // Delay modulation commands
        Command::SetDelayModulation(DeckId::A, mode) => {
            engine.send(AudioCommand::SetDelayModulationA(mode))
        }
        Command::SetDelayModulation(DeckId::B, mode) => {
            engine.send(AudioCommand::SetDelayModulationB(mode))
        }
        Command::CycleDelayModulation(_) => {} // TODO: implement cycling

        // Mode changes (handled by input handler, but we can use them for state)
        Command::EnterCommandMode
        | Command::EnterEffectsMode
        | Command::EnterNormalMode
        | Command::EnterBrowserMode
        | Command::Cancel
        | Command::ExecuteCommand(_) => {}

        // CRT screen effects
        Command::ToggleCrt => {
            app.state.crt_effects.toggle_crt();
            let status = if app.state.crt_effects.crt_enabled {
                "ON"
            } else {
                "OFF"
            };
            app.state.set_message(format!("▶ CRT effects {}", status));
        }
        Command::ToggleGlow => {
            app.state.crt_effects.toggle_glow();
            let status = if app.state.crt_effects.glow_enabled {
                "ON"
            } else {
                "OFF"
            };
            app.state.set_message(format!("▶ Phosphor glow {}", status));
        }
        Command::ToggleNoise => {
            app.state.crt_effects.toggle_noise();
            let status = if app.state.crt_effects.noise_enabled {
                "ON"
            } else {
                "OFF"
            };
            app.state.set_message(format!("▶ Static noise {}", status));
        }
        Command::ToggleChromatic => {
            app.state.crt_effects.toggle_chromatic();
            let status = if app.state.crt_effects.chromatic_enabled {
                "ON"
            } else {
                "OFF"
            };
            app.state
                .set_message(format!("▶ Chromatic aberration {}", status));
        }
        Command::CycleCrtIntensity => {
            app.state.crt_effects.cycle_intensity();
            let name = app.state.crt_effects.intensity.name();
            app.state.set_message(format!("▶ CRT intensity: {}", name));
        }

        // Mastering chain commands
        Command::ToggleMastering => {
            engine.send(AudioCommand::ToggleMastering);
            // Show feedback based on current state (will be toggled by engine)
            let status = if app.state.mastering_enabled {
                "OFF"
            } else {
                "ON"
            };
            app.state.set_message(format!("▶ Mastering {}", status));
        }
        Command::SetMasteringPreset(preset) => {
            engine.send(AudioCommand::SetMasteringPreset(preset));
            app.state
                .set_message(format!("▶ Mastering: {}", preset.display_name()));
        }
        Command::CycleMasteringPreset => {
            engine.send(AudioCommand::CycleMasteringPreset);
            // Show next preset (will be cycled by engine)
            let next = app.state.mastering_preset.next();
            app.state
                .set_message(format!("▶ Mastering: {}", next.display_name()));
        }

        // Tape Stop commands
        Command::ToggleTapeStop(deck) => match deck {
            DeckId::A => engine.send(AudioCommand::ToggleTapeStopA),
            DeckId::B => engine.send(AudioCommand::ToggleTapeStopB),
        },
        Command::TriggerTapeStop(deck) => {
            match deck {
                DeckId::A => engine.send(AudioCommand::TriggerTapeStopA),
                DeckId::B => engine.send(AudioCommand::TriggerTapeStopB),
            }
            app.state.set_message("▼ Tape Stop");
        }
        Command::TriggerTapeStart(deck) => {
            match deck {
                DeckId::A => engine.send(AudioCommand::TriggerTapeStartA),
                DeckId::B => engine.send(AudioCommand::TriggerTapeStartB),
            }
            app.state.set_message("▲ Tape Start");
        }

        // Flanger commands
        Command::ToggleFlanger(deck) => {
            match deck {
                DeckId::A => engine.send(AudioCommand::ToggleFlangerA),
                DeckId::B => engine.send(AudioCommand::ToggleFlangerB),
            }
            app.state.set_message("◊ Flanger toggled");
        }

        // Bitcrusher commands
        Command::ToggleBitcrusher(deck) => {
            match deck {
                DeckId::A => engine.send(AudioCommand::ToggleBitcrusherA),
                DeckId::B => engine.send(AudioCommand::ToggleBitcrusherB),
            }
            app.state.set_message("░ Bitcrusher toggled");
        }

        // Help scrolling
        Command::HelpScrollUp => app.state.help_scroll_up(),
        Command::HelpScrollDown => app.state.help_scroll_down(),
    }
}

fn load_track_with_key(
    app: &mut App,
    engine: &AudioEngine,
    loader: &TrackLoader,
    deck: DeckId,
    path: &std::path::Path,
    key: Option<String>,
) {
    app.state
        .set_message(format!("Loading {}...", path.display()));

    match loader.load(path) {
        Ok(track) => {
            let name = if track.metadata.title != "Unknown" {
                Some(track.metadata.title.clone())
            } else {
                path.file_name().map(|s| s.to_string_lossy().to_string())
            };

            // Wrap in Arc to avoid copying large sample data through channel
            let samples = Arc::new(track.samples);
            let waveform = Arc::new(track.waveform_overview);
            let enhanced_waveform = Arc::new(track.enhanced_waveform);
            match deck {
                DeckId::A => engine.send(AudioCommand::LoadDeckA(
                    samples,
                    track.sample_rate,
                    name,
                    waveform,
                    enhanced_waveform,
                    key,
                )),
                DeckId::B => engine.send(AudioCommand::LoadDeckB(
                    samples,
                    track.sample_rate,
                    name,
                    waveform,
                    enhanced_waveform,
                    key,
                )),
            }

            app.state.set_message(format!(
                "Loaded to deck {}: {}",
                match deck {
                    DeckId::A => 'A',
                    DeckId::B => 'B',
                },
                path.file_name().unwrap_or_default().to_string_lossy(),
            ));
        }
        Err(e) => {
            app.state.set_message(format!("Failed to load: {}", e));
        }
    }
}

fn render_ui(frame: &mut ratatui::Frame, app: &mut App) {
    let area = frame.area();
    let theme = &app.state.theme;

    // Clear with background
    let block = ratatui::widgets::Block::default().style(theme.normal());
    frame.render_widget(block, area);

    // Main layout - conditionally include library
    let chunks = if app.state.show_library {
        Layout::vertical([
            Constraint::Length(1), // Title
            Constraint::Min(8),    // Main content (decks)
            Constraint::Length(8), // Library
            Constraint::Length(6), // Spectrum
            Constraint::Length(3), // Phase + Crossfader
            Constraint::Length(1), // Status bar
        ])
        .split(area)
    } else {
        Layout::vertical([
            Constraint::Length(1), // Title
            Constraint::Min(10),   // Main content
            Constraint::Length(6), // Spectrum
            Constraint::Length(3), // Phase + Crossfader
            Constraint::Length(1), // Status bar
        ])
        .split(area)
    };

    // Title bar
    render_title(frame, chunks[0], theme);

    // Decks (side by side)
    let deck_chunks = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    // Deck A (with sync quality glow and CRT peak hold effects)
    let deck_a = DeckWidget::new(&app.state.deck_a, theme, "DECK A")
        .focused(app.state.focused == FocusedPane::DeckA)
        .frame_count(app.state.frame_count)
        .sync_quality(app.state.sync_quality)
        .crt_peak_hold(app.state.crt_effects.vu_peak_a)
        .zoom(app.state.zoom_a)
        .filter(
            app.state.filter_a_enabled,
            app.state.filter_a_type,
            app.state.filter_a_level,
        )
        .delay(app.state.delay_a_enabled, app.state.delay_a_level)
        .reverb(app.state.reverb_a_enabled, app.state.reverb_a_level);
    frame.render_widget(deck_a, deck_chunks[0]);

    // Deck B (with sync quality glow and CRT peak hold effects)
    let deck_b = DeckWidget::new(&app.state.deck_b, theme, "DECK B")
        .focused(app.state.focused == FocusedPane::DeckB)
        .frame_count(app.state.frame_count)
        .sync_quality(app.state.sync_quality)
        .crt_peak_hold(app.state.crt_effects.vu_peak_b)
        .zoom(app.state.zoom_b)
        .filter(
            app.state.filter_b_enabled,
            app.state.filter_b_type,
            app.state.filter_b_level,
        )
        .delay(app.state.delay_b_enabled, app.state.delay_b_level)
        .reverb(app.state.reverb_b_enabled, app.state.reverb_b_level);
    frame.render_widget(deck_b, deck_chunks[1]);

    // Determine chunk indices based on whether library is shown
    let (library_idx, spectrum_idx, crossfader_idx, status_idx) = if app.state.show_library {
        (Some(2), 3, 4, 5)
    } else {
        (None, 2, 3, 4)
    };

    // Library widget (if shown)
    if let Some(idx) = library_idx {
        let is_browser_mode = app.state.mode == ole_input::Mode::Browser;
        let library = LibraryWidget::new(&mut app.state.library, theme)
            .focused(is_browser_mode || app.state.focused == FocusedPane::Library);
        frame.render_widget(library, chunks[idx]);
    }

    // Spectrum analyzer or Oscilloscope (toggle with 'v')
    if app.state.show_scope {
        let scope = ScopeWidget::new(
            app.state.deck_a.scope_samples.as_slice(),
            app.state.deck_b.scope_samples.as_slice(),
            theme,
        )
        .mode(app.state.scope_mode);
        frame.render_widget(scope, chunks[spectrum_idx]);
    } else {
        let spectrum = SpectrumWidget::new(
            &app.state.deck_a.spectrum,
            &app.state.deck_b.spectrum,
            theme,
        )
        .sync_quality(app.state.sync_quality)
        .afterglow(
            &app.state.crt_effects.spectrum_history,
            app.state.crt_effects.spectrum_history_idx,
        );
        frame.render_widget(spectrum, chunks[spectrum_idx]);
    }

    // Phase + Camelot + Crossfader area (split horizontally)
    let mixer_chunks = Layout::horizontal([
        Constraint::Percentage(30), // Phase meter
        Constraint::Percentage(40), // Camelot wheel
        Constraint::Percentage(30), // Crossfader
    ])
    .split(chunks[crossfader_idx]);

    // Phase meter - shows beat alignment between decks
    let has_grid_a = app
        .state
        .deck_a
        .beat_grid_info
        .as_ref()
        .is_some_and(|g| g.has_grid);
    let has_grid_b = app
        .state
        .deck_b
        .beat_grid_info
        .as_ref()
        .is_some_and(|g| g.has_grid);
    let bpm_a = app.state.deck_a.bpm.unwrap_or(0.0) * app.state.deck_a.tempo;
    let bpm_b = app.state.deck_b.bpm.unwrap_or(0.0) * app.state.deck_b.tempo;
    let phase = PhaseWidget::new(theme)
        .phases(app.state.deck_a.beat_phase, app.state.deck_b.beat_phase)
        .bpms(bpm_a, bpm_b)
        .has_grids(has_grid_a, has_grid_b);
    frame.render_widget(phase, mixer_chunks[0]);

    // Master VU meter - shows deck levels with peak hold and LUFS
    let vu_meter = MasterVuMeterWidget::new(theme)
        .levels(app.state.deck_a.peak_level, app.state.deck_b.peak_level)
        .peak_holds(
            app.state.crt_effects.vu_peak_a,
            app.state.crt_effects.vu_peak_b,
        )
        .clipping(app.state.deck_a.is_clipping, app.state.deck_b.is_clipping)
        .lufs(app.state.mastering_lufs.momentary)
        .gain_reduction(app.state.mastering_gain_reduction);
    frame.render_widget(vu_meter, mixer_chunks[1]);

    // Crossfader with BPM difference display
    let crossfader = CrossfaderWidget::new(app.state.crossfader, theme)
        .bpms(app.state.deck_a.bpm, app.state.deck_b.bpm);
    frame.render_widget(crossfader, mixer_chunks[2]);

    // Build effect chain strings
    let effects_a = build_effect_string(
        app.state.filter_a_enabled,
        app.state.filter_a_level,
        app.state.delay_a_enabled,
        app.state.delay_a_level,
        app.state.reverb_a_enabled,
        app.state.reverb_a_level,
    );
    let effects_b = build_effect_string(
        app.state.filter_b_enabled,
        app.state.filter_b_level,
        app.state.delay_b_enabled,
        app.state.delay_b_level,
        app.state.reverb_b_enabled,
        app.state.reverb_b_level,
    );

    // Status bar
    let status = StatusBarWidget::new(app.state.mode, &app.state.command_buffer, theme)
        .message(app.state.message.as_deref(), app.state.message_type)
        .effects(effects_a, effects_b);
    frame.render_widget(status, chunks[status_idx]);

    // Help overlay (scrollable)
    if app.state.show_help {
        let help_area = centered_rect(72, 40, area);
        let help = HelpWidget::new(theme).scroll(app.state.help_scroll);
        frame.render_widget(help, help_area);
    }

    // CRT post-processing effects
    let buf = frame.buffer_mut();

    // Scanlines effect (subtle horizontal line darkening)
    if app.state.crt_effects.scanlines_enabled {
        apply_scanlines(buf, area, app.state.crt_effects.scanline_offset, theme);
    }

    // Screen flicker effect (triggered on track load)
    if app.state.crt_effects.flicker_frames_remaining > 0 {
        apply_flicker(buf, area, app.state.crt_effects.flicker_intensity);
    }

    // New CRT screen effects (only if master switch is on)
    if app.state.crt_effects.crt_enabled {
        // Phosphor glow - bloom around bright elements
        if app.state.crt_effects.glow_enabled && app.state.crt_effects.glow_intensity > 0.0 {
            apply_phosphor_glow(buf, area, app.state.crt_effects.glow_intensity, theme);
        }

        // Chromatic aberration - color fringing at edges
        if app.state.crt_effects.chromatic_enabled && app.state.crt_effects.chromatic_offset > 0 {
            apply_chromatic_aberration(buf, area, app.state.crt_effects.chromatic_offset);
        }

        // Static noise - random noise overlay (last, so it's on top)
        if app.state.crt_effects.noise_enabled && app.state.crt_effects.noise_intensity > 0.0 {
            apply_static_noise(
                buf,
                area,
                app.state.crt_effects.noise_intensity,
                app.state.frame_count,
            );
        }
    }
}

fn render_title(frame: &mut ratatui::Frame, area: Rect, theme: &Theme) {
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;

    let title_text = " OLE - Open Live Engine ";
    let padding = (area.width as usize).saturating_sub(title_text.len()) / 2;
    let padded = format!(
        "{:═<pad$}{}{:═<rest$}",
        "",
        title_text,
        "",
        pad = padding,
        rest = area.width as usize - padding - title_text.len()
    );

    let line = Line::from(Span::styled(padded, theme.title()));
    frame.render_widget(Paragraph::new(line), area);
}

/// Create a centered rectangle
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

/// Build effect chain string for display (e.g. "F5 D3 R2")
///
/// Optimized: uses a fixed-size buffer, no Vec allocation.
#[inline]
fn build_effect_string(
    filter_enabled: bool,
    filter_level: u8,
    delay_enabled: bool,
    delay_level: u8,
    reverb_enabled: bool,
    reverb_level: u8,
) -> String {
    // Max output: "F10 D5 R5" = 10 chars, use small string optimization
    let mut result = String::with_capacity(12);

    if filter_enabled && filter_level > 0 {
        use std::fmt::Write;
        let _ = write!(result, "F{}", filter_level);
    }
    if delay_enabled && delay_level > 0 {
        if !result.is_empty() {
            result.push(' ');
        }
        use std::fmt::Write;
        let _ = write!(result, "D{}", delay_level);
    }
    if reverb_enabled && reverb_level > 0 {
        if !result.is_empty() {
            result.push(' ');
        }
        use std::fmt::Write;
        let _ = write!(result, "R{}", reverb_level);
    }
    result
}

/// Apply CRT scanlines effect - dims every Nth row for raster appearance
///
/// Optimized: uses integer math for dimming, pre-computes dim factor.
#[inline]
fn apply_scanlines(buf: &mut ratatui::buffer::Buffer, area: Rect, offset: u8, theme: &Theme) {
    use ratatui::style::Color;

    let scanline_spacing = theme.scanline_spacing;
    let intensity = theme.scanline_intensity;

    // Early exit if scanlines disabled
    if scanline_spacing == 0 || intensity <= 0.0 || area.width == 0 || area.height == 0 {
        return;
    }

    // Pre-compute dim factor as integer: (1.0 - intensity) * 256, capped at 80% dimming
    // dim_factor ranges from 51 (0.2 * 256) to 256 (1.0 * 256)
    let dim_factor = ((1.0 - intensity.clamp(0.0, 0.8)) * 256.0) as u16;

    let offset_adjusted = offset / 4; // Slow roll effect

    for y in area.y..area.y + area.height {
        let row_with_offset = (y as u8).wrapping_add(offset_adjusted);
        if !row_with_offset.is_multiple_of(scanline_spacing) {
            continue; // Skip non-scanline rows early
        }

        for x in area.x..area.x + area.width {
            let cell = &mut buf[(x, y)];
            if let Some(Color::Rgb(r, g, b)) = cell.style().fg {
                // Integer dim: (color * dim_factor) >> 8
                let new_r = ((r as u16 * dim_factor) >> 8) as u8;
                let new_g = ((g as u16 * dim_factor) >> 8) as u8;
                let new_b = ((b as u16 * dim_factor) >> 8) as u8;
                cell.set_style(cell.style().fg(Color::Rgb(new_r, new_g, new_b)));
            } else {
                cell.set_style(theme.dim());
            }
        }
    }
}

/// Apply screen flicker effect - random distortion on track load
///
/// Optimized: uses integer math for brightness boost, pre-computes factors.
#[inline]
fn apply_flicker(buf: &mut ratatui::buffer::Buffer, area: Rect, intensity: f32) {
    use ratatui::style::Color;

    if intensity < 0.1 || area.width == 0 || area.height == 0 {
        return;
    }

    // Pre-compute brightness boost as integer: (1.0 + intensity * 0.3) * 256
    // This gives us a multiplier in the range 256-332 (for intensity 0.1-1.0)
    let boost_factor = (256.0 + intensity * 76.8) as u16; // 76.8 = 0.3 * 256

    // Pre-compute glitch threshold
    let do_glitch = intensity > 0.5;
    let glitch_offset = (intensity * 100.0) as u16;

    for y in area.y..area.y + area.height {
        // Check if this row should have glitch effect
        let row_glitch = do_glitch && ((y.wrapping_mul(7).wrapping_add(glitch_offset)) % 5 == 0);

        for x in area.x..area.x + area.width {
            let cell = &mut buf[(x, y)];

            if row_glitch {
                // Glitch character selection based on position
                let glitch_char = match (x.wrapping_add(y)) & 3 {
                    0 => '░',
                    1 => '▒',
                    _ => cell.symbol().chars().next().unwrap_or(' '),
                };
                cell.set_char(glitch_char);
            }

            // Brighten colors using integer math
            if let Some(Color::Rgb(r, g, b)) = cell.style().fg {
                // (color * boost_factor) >> 8, clamped to 255
                let new_r = (((r as u16) * boost_factor) >> 8).min(255) as u8;
                let new_g = (((g as u16) * boost_factor) >> 8).min(255) as u8;
                let new_b = (((b as u16) * boost_factor) >> 8).min(255) as u8;
                cell.set_style(cell.style().fg(Color::Rgb(new_r, new_g, new_b)));
            }
        }
    }
}

/// Apply phosphor glow effect - bright elements bleed light to neighbors
///
/// Optimized single-pass algorithm: processes bottom-to-top, right-to-left
/// to avoid cascading glow. Uses integer math for blend calculations.
/// Zero heap allocations.
#[inline]
fn apply_phosphor_glow(
    buf: &mut ratatui::buffer::Buffer,
    area: Rect,
    intensity: f32,
    theme: &Theme,
) {
    use ratatui::style::Color;

    if intensity <= 0.0 || area.width == 0 || area.height == 0 {
        return;
    }

    let threshold = theme.glow_threshold;
    // Pre-compute blend factor as fixed-point (0-256 scale) for integer math
    // glow_factor = intensity * 0.25, scaled to 0-64 range for bit shifting
    let blend = ((intensity * 64.0) as u16).min(64);

    let x_end = area.x + area.width;
    let y_end = area.y + area.height;

    // Process in reverse order (bottom-right to top-left) to prevent glow cascading
    for y in (area.y..y_end).rev() {
        for x in (area.x..x_end).rev() {
            let cell = &buf[(x, y)];
            if let Some(Color::Rgb(r, g, b)) = cell.style().fg {
                // Check if this is a bright cell (any channel exceeds threshold)
                if r > threshold || g > threshold || b > threshold {
                    // Apply glow to neighbors using integer blend
                    // blend_color = neighbor + ((bright - neighbor) * blend) >> 6
                    apply_glow_to_neighbor(buf, x.wrapping_sub(1), y, r, g, b, blend, area);
                    apply_glow_to_neighbor(buf, x + 1, y, r, g, b, blend, area);
                    apply_glow_to_neighbor(buf, x, y.wrapping_sub(1), r, g, b, blend, area);
                    apply_glow_to_neighbor(buf, x, y + 1, r, g, b, blend, area);
                }
            }
        }
    }
}

/// Apply glow blend to a single neighbor cell (inlined for performance)
#[inline(always)]
#[allow(clippy::too_many_arguments)] // Intentional: inline helper avoids struct overhead
fn apply_glow_to_neighbor(
    buf: &mut ratatui::buffer::Buffer,
    nx: u16,
    ny: u16,
    bright_r: u8,
    bright_g: u8,
    bright_b: u8,
    blend: u16,
    area: Rect,
) {
    use ratatui::style::Color;

    // Bounds check
    if nx < area.x || nx >= area.x + area.width || ny < area.y || ny >= area.y + area.height {
        return;
    }

    let cell = &mut buf[(nx, ny)];
    if let Some(Color::Rgb(nr, ng, nb)) = cell.style().fg {
        // Integer blend: new = old + ((bright - old) * blend) >> 6
        let new_r =
            (nr as u16 + (((bright_r as i16 - nr as i16) as u16 * blend) >> 6)).min(255) as u8;
        let new_g =
            (ng as u16 + (((bright_g as i16 - ng as i16) as u16 * blend) >> 6)).min(255) as u8;
        let new_b =
            (nb as u16 + (((bright_b as i16 - nb as i16) as u16 * blend) >> 6)).min(255) as u8;
        cell.set_style(cell.style().fg(Color::Rgb(new_r, new_g, new_b)));
    }
}

/// Apply chromatic aberration effect - color channel offset based on position
///
/// Optimized: uses pure integer math, no floating point operations.
/// Pre-computes shift values per column for cache efficiency.
#[inline]
fn apply_chromatic_aberration(buf: &mut ratatui::buffer::Buffer, area: Rect, offset: u8) {
    use ratatui::style::Color;

    if offset == 0 || area.width == 0 {
        return;
    }

    let center_x = area.x + area.width / 2;
    let half_width = (area.width / 2).max(1) as i32;
    // Pre-multiply offset for integer math: offset * 8, then we'll divide by half_width
    let offset_scaled = (offset as i32) << 3;

    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            let cell = &mut buf[(x, y)];

            if let Some(Color::Rgb(r, g, b)) = cell.style().fg {
                // Integer distance: (x - center) ranges from -half_width to +half_width
                // shift = (x - center) * offset * 8 / half_width
                let dx = x as i32 - center_x as i32;
                let shift = (dx * offset_scaled) / half_width;

                // Clamp using saturating arithmetic
                let new_r = (r as i32 + shift).clamp(0, 255) as u8;
                let new_b = (b as i32 - shift).clamp(0, 255) as u8;

                cell.set_style(cell.style().fg(Color::Rgb(new_r, g, new_b)));
            }
        }
    }
}

/// Apply static noise effect - random character noise overlay
///
/// Optimized: uses fast xorshift hash, integer math for color dimming,
/// and early-exit checks to minimize work.
#[inline]
fn apply_static_noise(
    buf: &mut ratatui::buffer::Buffer,
    area: Rect,
    intensity: f32,
    frame_count: u64,
) {
    use ratatui::style::Color;

    if intensity <= 0.0 || area.width == 0 || area.height == 0 {
        return;
    }

    // Noise glyphs (various densities) - static array
    const NOISE_GLYPHS: [char; 5] = ['░', '▒', '▓', '·', '∙'];

    // Pre-compute threshold as u32 for faster comparison (0-65535 range)
    let threshold = ((intensity * 65535.0) as u32).min(65535);

    // Dim factor as integer: 70% = multiply by 179 then >> 8
    const DIM_FACTOR: u16 = 179; // 0.7 * 256

    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            // Fast xorshift-based hash for pseudo-random
            let mut hash = (x as u32)
                .wrapping_mul(0x9E3779B9)
                .wrapping_add((y as u32).wrapping_mul(0x85EBCA6B))
                .wrapping_add((frame_count as u32).wrapping_mul(0xC2B2AE35));
            hash ^= hash >> 16;
            hash = hash.wrapping_mul(0x85EBCA6B);
            hash ^= hash >> 13;

            // Use lower 16 bits for threshold comparison
            if (hash & 0xFFFF) < threshold {
                let cell = &mut buf[(x, y)];

                // Check for space early to avoid work
                let current_char = cell.symbol().chars().next().unwrap_or(' ');
                if current_char == ' ' {
                    continue;
                }

                // Pick noise glyph using upper bits of hash
                let glyph_idx = ((hash >> 16) % NOISE_GLYPHS.len() as u32) as usize;
                cell.set_char(NOISE_GLYPHS[glyph_idx]);

                // Integer dim: (color * 179) >> 8 ≈ color * 0.7
                if let Some(Color::Rgb(r, g, b)) = cell.style().fg {
                    let new_r = ((r as u16 * DIM_FACTOR) >> 8) as u8;
                    let new_g = ((g as u16 * DIM_FACTOR) >> 8) as u8;
                    let new_b = ((b as u16 * DIM_FACTOR) >> 8) as u8;
                    cell.set_style(cell.style().fg(Color::Rgb(new_r, new_g, new_b)));
                }
            }
        }
    }
}
