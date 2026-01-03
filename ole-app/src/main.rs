//! OLE - Open Live Engine
//!
//! Terminal-based DJ application with vintage CRT aesthetic.

use std::io::{self, stdout};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

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
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::Receiver;

use ole_audio::{AudioEngine, AudioCommand, AudioEvent, EngineState};
use ole_input::{InputHandler, Command, DeckId, Direction, EffectType};
use ole_library::{AnalysisCache, LibraryScanner, ScanConfig, ScanProgress, TrackLoader};
use ole_tui::{
    App, FocusedPane, LibraryWidget, Theme,
    DeckWidget, SpectrumWidget, CrossfaderWidget, StatusBarWidget, HelpWidget,
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
            let _ = evt_tx.send(AudioEvent::Error(format!("Failed to get audio config: {}", e)));
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
            let _ = evt_tx.send(AudioEvent::Error(format!("Failed to create audio stream: {}", e)));
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

    // Initialize library cache and scanner
    let cache_path = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ole")
        .join("library.db");
    let cache = AnalysisCache::open(&cache_path).ok();
    let scanner = cache.map(LibraryScanner::new);

    // Track scan progress receiver
    let mut scan_progress_rx: Option<crossbeam_channel::Receiver<ScanProgress>> = None;

    let frame_duration = Duration::from_millis(1000 / FPS);
    let mut last_frame = Instant::now();

    // Show startup banner
    app.state.set_message("OLE - Open Live Engine | Press ? for help, / for library, :scan <dir> to scan tracks");

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
                        app.state.set_message(format!("Scanning {} files...", total));
                    }
                    ScanProgress::Analyzing { current, total, .. } => {
                        app.state.library.scan_progress = (current, total);
                    }
                    ScanProgress::Cached { current, total, .. } => {
                        app.state.library.scan_progress = (current, total);
                    }
                    ScanProgress::Complete { analyzed, cached, failed } => {
                        app.state.library.is_scanning = false;
                        // Load all tracks from scanner
                        if let Some(ref scanner) = scanner {
                            if let Ok(tracks) = scanner.get_all_tracks() {
                                app.state.library.set_tracks(tracks);
                            }
                        }
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
                                let config = ScanConfig {
                                    directory: path.clone(),
                                    ..Default::default()
                                };
                                let (rx, _handle) = scanner.scan_async(config);
                                scan_progress_rx = Some(rx);
                                app.state.library.is_scanning = true;
                                app.state.set_message(format!("Starting scan of {}...", path.display()));
                            } else {
                                app.state.set_error("Library cache not available");
                            }
                        }
                        Command::LibraryLoadToDeck(deck) => {
                            if let Some(track) = app.state.library.selected_track() {
                                let path = track.path.clone();
                                let key = track.key.clone();
                                load_track(&mut app, &engine, &track_loader, *deck, &path);
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
        Command::ToggleEffect(DeckId::A, EffectType::Filter) => engine.send(AudioCommand::ToggleFilterA),
        Command::ToggleEffect(DeckId::B, EffectType::Filter) => engine.send(AudioCommand::ToggleFilterB),
        Command::ToggleEffect(DeckId::A, EffectType::Delay) => engine.send(AudioCommand::ToggleDelayA),
        Command::ToggleEffect(DeckId::B, EffectType::Delay) => engine.send(AudioCommand::ToggleDelayB),
        Command::ToggleEffect(DeckId::A, EffectType::Reverb) => engine.send(AudioCommand::ToggleReverbA),
        Command::ToggleEffect(DeckId::B, EffectType::Reverb) => engine.send(AudioCommand::ToggleReverbB),
        Command::AdjustFilterCutoff(DeckId::A, delta) => engine.send(AudioCommand::AdjustFilterCutoffA(delta)),
        Command::AdjustFilterCutoff(DeckId::B, delta) => engine.send(AudioCommand::AdjustFilterCutoffB(delta)),

        // Effects - preset levels (with feedback)
        Command::SetDelayLevel(deck, level) => {
            let deck_char = match deck { DeckId::A => 'A', DeckId::B => 'B' };
            match deck {
                DeckId::A => engine.send(AudioCommand::SetDelayLevelA(level)),
                DeckId::B => engine.send(AudioCommand::SetDelayLevelB(level)),
            }
            if level == 0 {
                app.state.set_message(format!("▶ Deck {} DELAY OFF", deck_char));
            } else {
                app.state.set_message(format!("▶ Deck {} DELAY:{}", deck_char, level));
            }
        }
        Command::SetFilterPreset(deck, filter_type, level) => {
            let deck_char = match deck { DeckId::A => 'A', DeckId::B => 'B' };
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
                app.state.set_message(format!("▶ Deck {} FILTER OFF", deck_char));
            } else {
                app.state.set_message(format!("▶ Deck {} FILTER:{}:{}", deck_char, ft, level));
            }
        }
        Command::SetReverbLevel(deck, level) => {
            let deck_char = match deck { DeckId::A => 'A', DeckId::B => 'B' };
            match deck {
                DeckId::A => engine.send(AudioCommand::SetReverbLevelA(level)),
                DeckId::B => engine.send(AudioCommand::SetReverbLevelB(level)),
            }
            if level == 0 {
                app.state.set_message(format!("▶ Deck {} REVERB OFF", deck_char));
            } else {
                app.state.set_message(format!("▶ Deck {} REVERB:{}", deck_char, level));
            }
        }

        // Load tracks
        Command::LoadTrack(deck, path) => {
            load_track(app, engine, loader, deck, &path);
        }

        // UI commands
        Command::ToggleHelp => app.state.toggle_help(),
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
        Command::LibraryClearFilter => app.state.library.set_filter(None),
        Command::LibraryToggle => app.state.toggle_library(),

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
        Command::SetVinylNoise(DeckId::A, amount) => engine.send(AudioCommand::SetVinylNoiseA(amount)),
        Command::SetVinylNoise(DeckId::B, amount) => engine.send(AudioCommand::SetVinylNoiseB(amount)),
        Command::SetVinylWarmth(DeckId::A, amount) => engine.send(AudioCommand::SetVinylWarmthA(amount)),
        Command::SetVinylWarmth(DeckId::B, amount) => engine.send(AudioCommand::SetVinylWarmthB(amount)),

        // Time stretching commands
        Command::ToggleTimeStretch(DeckId::A) => engine.send(AudioCommand::ToggleTimeStretchA),
        Command::ToggleTimeStretch(DeckId::B) => engine.send(AudioCommand::ToggleTimeStretchB),
        Command::SetTimeStretchRatio(DeckId::A, ratio) => engine.send(AudioCommand::SetTimeStretchRatioA(ratio)),
        Command::SetTimeStretchRatio(DeckId::B, ratio) => engine.send(AudioCommand::SetTimeStretchRatioB(ratio)),

        // Delay modulation commands
        Command::SetDelayModulation(DeckId::A, mode) => engine.send(AudioCommand::SetDelayModulationA(mode)),
        Command::SetDelayModulation(DeckId::B, mode) => engine.send(AudioCommand::SetDelayModulationB(mode)),
        Command::CycleDelayModulation(_) => {} // TODO: implement cycling

        // Mode changes (handled by input handler, but we can use them for state)
        Command::EnterCommandMode | Command::EnterEffectsMode |
        Command::EnterNormalMode | Command::EnterBrowserMode |
        Command::Cancel | Command::ExecuteCommand(_) => {}
    }
}

fn load_track(app: &mut App, engine: &AudioEngine, loader: &TrackLoader, deck: DeckId, path: &std::path::Path) {
    app.state.set_message(format!("Loading {}...", path.display()));

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
            match deck {
                DeckId::A => engine.send(AudioCommand::LoadDeckA(samples, track.sample_rate, name, waveform)),
                DeckId::B => engine.send(AudioCommand::LoadDeckB(samples, track.sample_rate, name, waveform)),
            }

            app.state.set_message(format!(
                "Loaded to deck {}: {}",
                match deck { DeckId::A => 'A', DeckId::B => 'B' },
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
    let block = ratatui::widgets::Block::default()
        .style(theme.normal());
    frame.render_widget(block, area);

    // Main layout - conditionally include library
    let chunks = if app.state.show_library {
        Layout::vertical([
            Constraint::Length(1),  // Title
            Constraint::Min(8),     // Main content (decks)
            Constraint::Length(8),  // Library
            Constraint::Length(6),  // Spectrum
            Constraint::Length(3),  // Crossfader
            Constraint::Length(1),  // Status bar
        ])
        .split(area)
    } else {
        Layout::vertical([
            Constraint::Length(1),  // Title
            Constraint::Min(10),    // Main content
            Constraint::Length(6),  // Spectrum
            Constraint::Length(3),  // Crossfader
            Constraint::Length(1),  // Status bar
        ])
        .split(area)
    };

    // Title bar
    render_title(frame, chunks[0], theme);

    // Decks (side by side)
    let deck_chunks = Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ])
    .split(chunks[1]);

    // Deck A
    let deck_a = DeckWidget::new(&app.state.deck_a, theme, "DECK A")
        .focused(app.state.focused == FocusedPane::DeckA)
        .frame_count(app.state.frame_count)
        .filter(app.state.filter_a_enabled, app.state.filter_a_type, app.state.filter_a_level)
        .delay(app.state.delay_a_enabled, app.state.delay_a_level)
        .reverb(app.state.reverb_a_enabled, app.state.reverb_a_level);
    frame.render_widget(deck_a, deck_chunks[0]);

    // Deck B
    let deck_b = DeckWidget::new(&app.state.deck_b, theme, "DECK B")
        .focused(app.state.focused == FocusedPane::DeckB)
        .frame_count(app.state.frame_count)
        .filter(app.state.filter_b_enabled, app.state.filter_b_type, app.state.filter_b_level)
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

    // Spectrum analyzer
    let spectrum = SpectrumWidget::new(
        &app.state.deck_a.spectrum,
        &app.state.deck_b.spectrum,
        theme,
    );
    frame.render_widget(spectrum, chunks[spectrum_idx]);

    // Crossfader with BPM difference display
    let crossfader = CrossfaderWidget::new(app.state.crossfader, theme)
        .bpms(app.state.deck_a.bpm, app.state.deck_b.bpm);
    frame.render_widget(crossfader, chunks[crossfader_idx]);

    // Build effect chain strings
    let effects_a = build_effect_string(
        app.state.filter_a_enabled, app.state.filter_a_level,
        app.state.delay_a_enabled, app.state.delay_a_level,
        app.state.reverb_a_enabled, app.state.reverb_a_level,
    );
    let effects_b = build_effect_string(
        app.state.filter_b_enabled, app.state.filter_b_level,
        app.state.delay_b_enabled, app.state.delay_b_level,
        app.state.reverb_b_enabled, app.state.reverb_b_level,
    );

    // Status bar
    let status = StatusBarWidget::new(
        app.state.mode,
        &app.state.command_buffer,
        theme,
    )
    .message(app.state.message.as_deref(), app.state.message_type)
    .effects(effects_a, effects_b);
    frame.render_widget(status, chunks[status_idx]);

    // Help overlay
    if app.state.show_help {
        let help_area = centered_rect(65, 37, area);
        let help = HelpWidget::new(theme);
        frame.render_widget(help, help_area);
    }
}

fn render_title(frame: &mut ratatui::Frame, area: Rect, theme: &Theme) {
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;

    let title_text = " OLE - Open Live Engine ";
    let padding = (area.width as usize).saturating_sub(title_text.len()) / 2;
    let padded = format!("{:═<pad$}{}{:═<rest$}",
        "", title_text, "",
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
fn build_effect_string(
    filter_enabled: bool, filter_level: u8,
    delay_enabled: bool, delay_level: u8,
    reverb_enabled: bool, reverb_level: u8,
) -> String {
    let mut parts = Vec::new();
    if filter_enabled && filter_level > 0 {
        parts.push(format!("F{}", filter_level));
    }
    if delay_enabled && delay_level > 0 {
        parts.push(format!("D{}", delay_level));
    }
    if reverb_enabled && reverb_level > 0 {
        parts.push(format!("R{}", reverb_level));
    }
    parts.join(" ")
}
