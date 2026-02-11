use std::path::PathBuf;
use std::sync::Arc;

use crossbeam_channel::Sender;
use eframe::egui;

use ole_audio::{AudioCommand, AudioEvent};
use ole_input::{Command, DeckId, Direction, EffectType};
use ole_library::{AnalysisCache, Config, LibraryScanner, ScanConfig, ScanProgress, TrackLoader};

use crate::input::handle_keyboard;
use crate::state::{FocusedPane, GuiState};
use crate::theme::CyberTheme;
use crate::widgets;

pub struct OleApp {
    state: GuiState,
    cmd_tx: Sender<AudioCommand>,
    event_rx: crossbeam_channel::Receiver<AudioEvent>,
    track_loader: TrackLoader,
    scanner: Option<LibraryScanner>,
    config: Config,
    scan_progress_rx: Option<crossbeam_channel::Receiver<ScanProgress>>,
    current_scan_folder: Option<PathBuf>,
    theme_applied: bool,
}

impl OleApp {
    pub fn new(
        cmd_tx: Sender<AudioCommand>,
        event_rx: crossbeam_channel::Receiver<AudioEvent>,
    ) -> Self {
        let track_loader = TrackLoader::new();
        let config = Config::load();

        let cache_path = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ole")
            .join("library.db");
        let cache = AnalysisCache::open(&cache_path).ok();
        let scanner = cache.map(LibraryScanner::new);

        let mut state = GuiState::default();

        // Load cached tracks
        if config.last_scan_folder.is_some() {
            if let Some(ref scanner) = scanner {
                if let Ok(tracks) = scanner.get_all_tracks() {
                    if !tracks.is_empty() {
                        state.library.set_tracks(tracks);
                    }
                }
            }
        }

        let track_count = state.library.tracks.len();
        if track_count > 0 {
            state.set_message(format!(
                "OLE - Loaded {} tracks | Press ? for help, / for library",
                track_count
            ));
        } else {
            state.set_message(
                "OLE - Open Live Engine | Press ? for help, / for library, :scan <dir> to scan tracks",
            );
        }

        Self {
            state,
            cmd_tx,
            event_rx,
            track_loader,
            scanner,
            config,
            scan_progress_rx: None,
            current_scan_folder: None,
            theme_applied: false,
        }
    }

    fn send_audio(&self, cmd: AudioCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    fn drain_audio_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            self.state.handle_audio_event(event);
        }
    }

    fn process_scan_progress(&mut self) {
        let mut scan_complete = false;
        if let Some(ref rx) = self.scan_progress_rx {
            while let Ok(progress) = rx.try_recv() {
                match progress {
                    ScanProgress::Started { total } => {
                        self.state.library.is_scanning = true;
                        self.state.library.scan_progress = (0, total);
                        self.state.set_message(format!("Scanning {} files...", total));
                    }
                    ScanProgress::Analyzing { current, total, .. } => {
                        self.state.library.scan_progress = (current, total);
                    }
                    ScanProgress::Cached { current, total, .. } => {
                        self.state.library.scan_progress = (current, total);
                    }
                    ScanProgress::Complete { analyzed, cached, failed } => {
                        self.state.library.is_scanning = false;
                        if let Some(ref scanner) = self.scanner {
                            if let Ok(tracks) = scanner.get_all_tracks() {
                                self.state.library.set_tracks(tracks);
                            }
                        }
                        if let Some(ref folder) = self.current_scan_folder {
                            self.config.last_scan_folder = Some(folder.clone());
                            let _ = self.config.save();
                        }
                        self.current_scan_folder = None;
                        self.state.set_success(format!(
                            "Scan complete: {} analyzed, {} cached, {} failed",
                            analyzed, cached, failed
                        ));
                        scan_complete = true;
                    }
                    ScanProgress::Error { .. } => {}
                }
            }
        }
        if scan_complete {
            self.scan_progress_rx = None;
        }
    }

    pub fn handle_command(&mut self, cmd: Command) {
        match cmd {
            // Transport
            Command::Play(DeckId::A) => self.send_audio(AudioCommand::PlayA),
            Command::Play(DeckId::B) => self.send_audio(AudioCommand::PlayB),
            Command::Pause(DeckId::A) => self.send_audio(AudioCommand::PauseA),
            Command::Pause(DeckId::B) => self.send_audio(AudioCommand::PauseB),
            Command::Stop(DeckId::A) => self.send_audio(AudioCommand::StopA),
            Command::Stop(DeckId::B) => self.send_audio(AudioCommand::StopB),
            Command::Toggle(DeckId::A) => self.send_audio(AudioCommand::ToggleA),
            Command::Toggle(DeckId::B) => self.send_audio(AudioCommand::ToggleB),

            // Seeking
            Command::Seek(DeckId::A, pos) => self.send_audio(AudioCommand::SeekA(pos)),
            Command::Seek(DeckId::B, pos) => self.send_audio(AudioCommand::SeekB(pos)),
            Command::Nudge(DeckId::A, d) => self.send_audio(AudioCommand::NudgeA(d)),
            Command::Nudge(DeckId::B, d) => self.send_audio(AudioCommand::NudgeB(d)),
            Command::BeatNudge(DeckId::A, b) => self.send_audio(AudioCommand::BeatNudgeA(b)),
            Command::BeatNudge(DeckId::B, b) => self.send_audio(AudioCommand::BeatNudgeB(b)),
            Command::Beatjump(DeckId::A, b) => self.send_audio(AudioCommand::BeatjumpA(b)),
            Command::Beatjump(DeckId::B, b) => self.send_audio(AudioCommand::BeatjumpB(b)),

            // Cue points
            Command::SetCue(DeckId::A, n) => {
                self.send_audio(AudioCommand::SetCueA(n));
                self.state.set_success(format!("Deck A CUE {} set", n));
            }
            Command::SetCue(DeckId::B, n) => {
                self.send_audio(AudioCommand::SetCueB(n));
                self.state.set_success(format!("Deck B CUE {} set", n));
            }
            Command::JumpCue(DeckId::A, n) => self.send_audio(AudioCommand::JumpCueA(n)),
            Command::JumpCue(DeckId::B, n) => self.send_audio(AudioCommand::JumpCueB(n)),

            // Tempo
            Command::SetTempo(DeckId::A, t) => self.send_audio(AudioCommand::SetTempoA(t)),
            Command::SetTempo(DeckId::B, t) => self.send_audio(AudioCommand::SetTempoB(t)),
            Command::AdjustTempo(DeckId::A, d) => self.send_audio(AudioCommand::AdjustTempoA(d)),
            Command::AdjustTempo(DeckId::B, d) => self.send_audio(AudioCommand::AdjustTempoB(d)),

            // Gain
            Command::SetGain(DeckId::A, g) => self.send_audio(AudioCommand::SetGainA(g)),
            Command::SetGain(DeckId::B, g) => self.send_audio(AudioCommand::SetGainB(g)),
            Command::AdjustGain(DeckId::A, d) => self.send_audio(AudioCommand::AdjustGainA(d)),
            Command::AdjustGain(DeckId::B, d) => self.send_audio(AudioCommand::AdjustGainB(d)),

            // Sync
            Command::Sync(DeckId::A) => self.send_audio(AudioCommand::SyncAToB),
            Command::Sync(DeckId::B) => self.send_audio(AudioCommand::SyncBToA),

            // Crossfader
            Command::SetCrossfader(pos) => self.send_audio(AudioCommand::SetCrossfader(pos)),
            Command::MoveCrossfader(Direction::Left) => {
                self.send_audio(AudioCommand::MoveCrossfader(-0.1))
            }
            Command::MoveCrossfader(Direction::Right) => {
                self.send_audio(AudioCommand::MoveCrossfader(0.1))
            }
            Command::MoveCrossfader(_) => {}
            Command::CenterCrossfader => self.send_audio(AudioCommand::CenterCrossfader),

            // Effects - toggle
            Command::ToggleEffect(DeckId::A, EffectType::Filter) => {
                self.send_audio(AudioCommand::ToggleFilterA)
            }
            Command::ToggleEffect(DeckId::B, EffectType::Filter) => {
                self.send_audio(AudioCommand::ToggleFilterB)
            }
            Command::ToggleEffect(DeckId::A, EffectType::Delay) => {
                self.send_audio(AudioCommand::ToggleDelayA)
            }
            Command::ToggleEffect(DeckId::B, EffectType::Delay) => {
                self.send_audio(AudioCommand::ToggleDelayB)
            }
            Command::ToggleEffect(DeckId::A, EffectType::Reverb) => {
                self.send_audio(AudioCommand::ToggleReverbA)
            }
            Command::ToggleEffect(DeckId::B, EffectType::Reverb) => {
                self.send_audio(AudioCommand::ToggleReverbB)
            }
            Command::ToggleEffect(DeckId::A, EffectType::TapeStop) => {
                self.send_audio(AudioCommand::ToggleTapeStopA)
            }
            Command::ToggleEffect(DeckId::B, EffectType::TapeStop) => {
                self.send_audio(AudioCommand::ToggleTapeStopB)
            }
            Command::ToggleEffect(DeckId::A, EffectType::Flanger) => {
                self.send_audio(AudioCommand::ToggleFlangerA)
            }
            Command::ToggleEffect(DeckId::B, EffectType::Flanger) => {
                self.send_audio(AudioCommand::ToggleFlangerB)
            }
            Command::ToggleEffect(DeckId::A, EffectType::Bitcrusher) => {
                self.send_audio(AudioCommand::ToggleBitcrusherA)
            }
            Command::ToggleEffect(DeckId::B, EffectType::Bitcrusher) => {
                self.send_audio(AudioCommand::ToggleBitcrusherB)
            }
            Command::AdjustFilterCutoff(DeckId::A, d) => {
                self.send_audio(AudioCommand::AdjustFilterCutoffA(d))
            }
            Command::AdjustFilterCutoff(DeckId::B, d) => {
                self.send_audio(AudioCommand::AdjustFilterCutoffB(d))
            }

            // Effects - preset levels
            Command::SetDelayLevel(deck, level) => {
                let ch = match deck { DeckId::A => 'A', DeckId::B => 'B' };
                match deck {
                    DeckId::A => self.send_audio(AudioCommand::SetDelayLevelA(level)),
                    DeckId::B => self.send_audio(AudioCommand::SetDelayLevelB(level)),
                }
                if level == 0 {
                    self.state.set_message(format!("Deck {} DELAY OFF", ch));
                } else {
                    self.state.set_message(format!("Deck {} DELAY:{}", ch, level));
                }
            }
            Command::SetFilterPreset(deck, filter_type, level) => {
                let ch = match deck { DeckId::A => 'A', DeckId::B => 'B' };
                let ft = match filter_type {
                    ole_audio::FilterType::LowPass => "LOW",
                    ole_audio::FilterType::BandPass => "BAND",
                    ole_audio::FilterType::HighPass => "HIGH",
                };
                match deck {
                    DeckId::A => self.send_audio(AudioCommand::SetFilterPresetA(filter_type, level)),
                    DeckId::B => self.send_audio(AudioCommand::SetFilterPresetB(filter_type, level)),
                }
                if level == 0 {
                    self.state.set_message(format!("Deck {} FILTER OFF", ch));
                } else {
                    self.state.set_message(format!("Deck {} FILTER:{}:{}", ch, ft, level));
                }
            }
            Command::SetReverbLevel(deck, level) => {
                let ch = match deck { DeckId::A => 'A', DeckId::B => 'B' };
                match deck {
                    DeckId::A => self.send_audio(AudioCommand::SetReverbLevelA(level)),
                    DeckId::B => self.send_audio(AudioCommand::SetReverbLevelB(level)),
                }
                if level == 0 {
                    self.state.set_message(format!("Deck {} REVERB OFF", ch));
                } else {
                    self.state.set_message(format!("Deck {} REVERB:{}", ch, level));
                }
            }

            // Load tracks
            Command::LoadTrack(deck, path) => {
                self.load_track(deck, &path, None);
            }

            // UI commands
            Command::ToggleHelp => self.state.toggle_help(),
            Command::ToggleScope => self.state.toggle_scope(),
            Command::CycleScopeMode => self.state.cycle_scope_mode(),
            Command::ZoomIn(deck) => match deck {
                DeckId::A => self.state.zoom_a = self.state.zoom_a.zoom_in(),
                DeckId::B => self.state.zoom_b = self.state.zoom_b.zoom_in(),
            },
            Command::ZoomOut(deck) => match deck {
                DeckId::A => self.state.zoom_a = self.state.zoom_a.zoom_out(),
                DeckId::B => self.state.zoom_b = self.state.zoom_b.zoom_out(),
            },
            Command::SetTheme(_) => {} // Single theme in GUI
            Command::CycleFocus => self.state.cycle_focus(),
            Command::Focus(deck) => {
                self.state.focused = match deck {
                    DeckId::A => FocusedPane::DeckA,
                    DeckId::B => FocusedPane::DeckB,
                };
            }
            Command::Quit => self.state.should_quit = true,

            // Library commands
            Command::LibrarySelectNext => self.state.library.select_next(),
            Command::LibrarySelectPrev => self.state.library.select_prev(),
            Command::LibrarySelectFirst => self.state.library.select_first(),
            Command::LibrarySelectLast => self.state.library.select_last(),
            Command::LibraryFilterByKey(key) => self.state.library.set_filter(Some(key)),
            Command::LibraryFilterByBpmRange(min, _max) => {
                if self.state.library.jump_to_bpm(min) {
                    self.state.set_message(format!("BPM {}", min));
                }
            }
            Command::LibraryFilterCompatible => {
                self.state.library.filter_compatible();
                self.state.set_message("Showing compatible keys");
            }
            Command::LibraryClearFilter => {
                self.state.library.set_filter(None);
                self.state.set_message("Filter cleared");
            }
            Command::LibraryToggle => self.state.toggle_library(),
            Command::LibraryJumpToKey(pos, is_minor) => {
                let key_str = format!("{}{}", pos, if is_minor { 'A' } else { 'B' });
                if self.state.library.jump_to_key(pos, is_minor) {
                    self.state.set_message(format!("Key {}", key_str));
                } else {
                    self.state.set_warning(format!("No tracks in {}", key_str));
                }
            }
            Command::LibraryJumpToBpm(bpm) => {
                if self.state.library.jump_to_bpm(bpm) {
                    self.state.set_message(format!("~{} BPM", bpm));
                } else {
                    self.state.set_warning(format!("No tracks near {} BPM", bpm));
                }
            }

            // Library scan/load handled specially
            Command::LibraryScan(path) => {
                if let Some(ref scanner) = self.scanner {
                    let scan_config = ScanConfig {
                        directory: path.clone(),
                        ..Default::default()
                    };
                    let (rx, _handle) = scanner.scan_async(scan_config);
                    self.scan_progress_rx = Some(rx);
                    self.current_scan_folder = Some(path.clone());
                    self.state.library.is_scanning = true;
                    self.state.set_message(format!("Starting scan of {}...", path.display()));
                } else {
                    self.state.set_error("Library cache not available");
                }
            }
            Command::LibraryRescan => {
                if let Some(ref folder) = self.config.last_scan_folder {
                    if let Some(ref scanner) = self.scanner {
                        let (rx, _handle) = scanner.rescan_turbo(folder.clone());
                        self.scan_progress_rx = Some(rx);
                        self.current_scan_folder = Some(folder.clone());
                        self.state.library.is_scanning = true;
                        let cpus = std::thread::available_parallelism()
                            .map(|p| p.get())
                            .unwrap_or(8);
                        self.state.set_message(format!(
                            "TURBO RESCAN: {} threads | {}",
                            cpus * 2, folder.display()
                        ));
                    } else {
                        self.state.set_error("Library cache not available");
                    }
                } else {
                    self.state.set_error("No previous scan folder - use :scan <path> first");
                }
            }
            Command::LibraryLoadToDeck(deck) => {
                if let Some(track) = self.state.library.selected_track() {
                    let path = track.path.clone();
                    let key = track.key.clone();
                    self.state.library.current_playing_key = key.clone();
                    self.load_track(deck, &path, key);
                }
            }

            // Filter mode
            Command::SetFilterMode(DeckId::A, mode) => {
                self.send_audio(AudioCommand::SetFilterModeA(mode))
            }
            Command::SetFilterMode(DeckId::B, mode) => {
                self.send_audio(AudioCommand::SetFilterModeB(mode))
            }
            Command::CycleFilterMode(DeckId::A) => {
                let next = match self.state.filter_a_mode {
                    ole_audio::FilterMode::Biquad => ole_audio::FilterMode::Ladder,
                    ole_audio::FilterMode::Ladder => ole_audio::FilterMode::SVF,
                    ole_audio::FilterMode::SVF => ole_audio::FilterMode::Biquad,
                };
                self.state.filter_a_mode = next;
                self.send_audio(AudioCommand::SetFilterModeA(next));
            }
            Command::CycleFilterMode(DeckId::B) => {
                let next = match self.state.filter_b_mode {
                    ole_audio::FilterMode::Biquad => ole_audio::FilterMode::Ladder,
                    ole_audio::FilterMode::Ladder => ole_audio::FilterMode::SVF,
                    ole_audio::FilterMode::SVF => ole_audio::FilterMode::Biquad,
                };
                self.state.filter_b_mode = next;
                self.send_audio(AudioCommand::SetFilterModeB(next));
            }

            // Vinyl
            Command::ToggleVinyl(DeckId::A) => self.send_audio(AudioCommand::ToggleVinylA),
            Command::ToggleVinyl(DeckId::B) => self.send_audio(AudioCommand::ToggleVinylB),
            Command::SetVinylPreset(DeckId::A, preset) => {
                let p = vinyl_preset_to_audio(preset);
                self.send_audio(AudioCommand::SetVinylPresetA(p));
            }
            Command::SetVinylPreset(DeckId::B, preset) => {
                let p = vinyl_preset_to_audio(preset);
                self.send_audio(AudioCommand::SetVinylPresetB(p));
            }
            Command::CycleVinylPreset(deck) => {
                let next = match (deck, self.state.vinyl_a_preset, self.state.vinyl_b_preset) {
                    (DeckId::A, p, _) | (DeckId::B, _, p) => {
                        match p {
                            ole_audio::VinylPreset::Clean => ole_audio::VinylPreset::Warm,
                            ole_audio::VinylPreset::Warm => ole_audio::VinylPreset::Vintage,
                            ole_audio::VinylPreset::Vintage => ole_audio::VinylPreset::Worn,
                            ole_audio::VinylPreset::Worn => ole_audio::VinylPreset::Extreme,
                            ole_audio::VinylPreset::Extreme => ole_audio::VinylPreset::Clean,
                        }
                    }
                };
                match deck {
                    DeckId::A => {
                        self.state.vinyl_a_preset = next;
                        self.send_audio(AudioCommand::SetVinylPresetA(next));
                    }
                    DeckId::B => {
                        self.state.vinyl_b_preset = next;
                        self.send_audio(AudioCommand::SetVinylPresetB(next));
                    }
                }
            }
            Command::SetVinylWow(DeckId::A, a) => self.send_audio(AudioCommand::SetVinylWowA(a)),
            Command::SetVinylWow(DeckId::B, a) => self.send_audio(AudioCommand::SetVinylWowB(a)),
            Command::SetVinylNoise(DeckId::A, a) => self.send_audio(AudioCommand::SetVinylNoiseA(a)),
            Command::SetVinylNoise(DeckId::B, a) => self.send_audio(AudioCommand::SetVinylNoiseB(a)),
            Command::SetVinylWarmth(DeckId::A, a) => self.send_audio(AudioCommand::SetVinylWarmthA(a)),
            Command::SetVinylWarmth(DeckId::B, a) => self.send_audio(AudioCommand::SetVinylWarmthB(a)),

            // Time stretch
            Command::ToggleTimeStretch(DeckId::A) => {
                self.send_audio(AudioCommand::ToggleTimeStretchA)
            }
            Command::ToggleTimeStretch(DeckId::B) => {
                self.send_audio(AudioCommand::ToggleTimeStretchB)
            }
            Command::SetTimeStretchRatio(DeckId::A, r) => {
                self.send_audio(AudioCommand::SetTimeStretchRatioA(r))
            }
            Command::SetTimeStretchRatio(DeckId::B, r) => {
                self.send_audio(AudioCommand::SetTimeStretchRatioB(r))
            }

            // Delay modulation
            Command::SetDelayModulation(DeckId::A, m) => {
                self.send_audio(AudioCommand::SetDelayModulationA(m))
            }
            Command::SetDelayModulation(DeckId::B, m) => {
                self.send_audio(AudioCommand::SetDelayModulationB(m))
            }
            Command::CycleDelayModulation(deck) => {
                let current = match deck {
                    DeckId::A => self.state.delay_a_modulation,
                    DeckId::B => self.state.delay_b_modulation,
                };
                let next = match current {
                    ole_audio::DelayModulation::Off => ole_audio::DelayModulation::Subtle,
                    ole_audio::DelayModulation::Subtle => ole_audio::DelayModulation::Classic,
                    ole_audio::DelayModulation::Classic => ole_audio::DelayModulation::Heavy,
                    ole_audio::DelayModulation::Heavy => ole_audio::DelayModulation::Off,
                };
                match deck {
                    DeckId::A => {
                        self.state.delay_a_modulation = next;
                        self.send_audio(AudioCommand::SetDelayModulationA(next));
                    }
                    DeckId::B => {
                        self.state.delay_b_modulation = next;
                        self.send_audio(AudioCommand::SetDelayModulationB(next));
                    }
                }
            }

            // Mode changes (handled by input handler)
            Command::EnterCommandMode
            | Command::EnterEffectsMode
            | Command::EnterNormalMode
            | Command::EnterBrowserMode
            | Command::Cancel
            | Command::ExecuteCommand(_) => {}

            // CRT effects (adapted for GUI)
            Command::ToggleCrt => {
                self.state.scanlines_enabled = !self.state.scanlines_enabled;
                let status = if self.state.scanlines_enabled { "ON" } else { "OFF" };
                self.state.set_message(format!("CRT effects {}", status));
            }
            Command::ToggleGlow => {
                self.state.glow_enabled = !self.state.glow_enabled;
            }
            Command::ToggleNoise => {
                self.state.noise_enabled = !self.state.noise_enabled;
            }
            Command::ToggleChromatic => {
                self.state.chromatic_enabled = !self.state.chromatic_enabled;
            }
            Command::CycleCrtIntensity => {
                self.state.crt_intensity = (self.state.crt_intensity + 1) % 4;
                let name = match self.state.crt_intensity {
                    0 => "Off",
                    1 => "Subtle",
                    2 => "Medium",
                    _ => "Heavy",
                };
                self.state.set_message(format!("CRT: {}", name));
            }

            // Mastering
            Command::ToggleMastering => {
                self.send_audio(AudioCommand::ToggleMastering);
                self.state.mastering_enabled = !self.state.mastering_enabled;
                let status = if self.state.mastering_enabled { "ON" } else { "OFF" };
                self.state.set_message(format!("Mastering {}", status));
            }
            Command::SetMasteringPreset(preset) => {
                self.send_audio(AudioCommand::SetMasteringPreset(preset));
                self.state.set_message(format!("Mastering: {}", preset.display_name()));
            }
            Command::CycleMasteringPreset => {
                self.send_audio(AudioCommand::CycleMasteringPreset);
                self.state.mastering_preset = self.state.mastering_preset.next();
                self.state.set_message(format!("Mastering: {}", self.state.mastering_preset.display_name()));
            }

            // Tape Stop
            Command::ToggleTapeStop(deck) => match deck {
                DeckId::A => self.send_audio(AudioCommand::ToggleTapeStopA),
                DeckId::B => self.send_audio(AudioCommand::ToggleTapeStopB),
            },
            Command::TriggerTapeStop(deck) => {
                match deck {
                    DeckId::A => self.send_audio(AudioCommand::TriggerTapeStopA),
                    DeckId::B => self.send_audio(AudioCommand::TriggerTapeStopB),
                }
                self.state.set_message("Tape Stop");
            }
            Command::TriggerTapeStart(deck) => {
                match deck {
                    DeckId::A => self.send_audio(AudioCommand::TriggerTapeStartA),
                    DeckId::B => self.send_audio(AudioCommand::TriggerTapeStartB),
                }
                self.state.set_message("Tape Start");
            }

            // Flanger
            Command::ToggleFlanger(deck) => {
                match deck {
                    DeckId::A => self.send_audio(AudioCommand::ToggleFlangerA),
                    DeckId::B => self.send_audio(AudioCommand::ToggleFlangerB),
                }
                self.state.set_message("Flanger toggled");
            }

            // Bitcrusher
            Command::ToggleBitcrusher(deck) => {
                match deck {
                    DeckId::A => self.send_audio(AudioCommand::ToggleBitcrusherA),
                    DeckId::B => self.send_audio(AudioCommand::ToggleBitcrusherB),
                }
                self.state.set_message("Bitcrusher toggled");
            }

            // Help scrolling
            Command::HelpScrollUp => self.state.help_scroll = (self.state.help_scroll - 30.0).max(0.0),
            Command::HelpScrollDown => self.state.help_scroll = (self.state.help_scroll + 30.0).min(2000.0),
        }
    }

    fn load_track(&mut self, deck: DeckId, path: &std::path::Path, key: Option<String>) {
        self.state.set_message(format!("Loading {}...", path.display()));
        match self.track_loader.load(path) {
            Ok(track) => {
                let name = if track.metadata.title != "Unknown" {
                    Some(track.metadata.title.clone())
                } else {
                    path.file_name().map(|s| s.to_string_lossy().to_string())
                };
                let samples = Arc::new(track.samples);
                let waveform = Arc::new(track.waveform_overview);
                let enhanced_waveform = Arc::new(track.enhanced_waveform);
                match deck {
                    DeckId::A => self.send_audio(AudioCommand::LoadDeckA(
                        samples, track.sample_rate, name, waveform, enhanced_waveform, key,
                    )),
                    DeckId::B => self.send_audio(AudioCommand::LoadDeckB(
                        samples, track.sample_rate, name, waveform, enhanced_waveform, key,
                    )),
                }
                self.state.set_message(format!(
                    "Loaded to deck {}: {}",
                    match deck { DeckId::A => 'A', DeckId::B => 'B' },
                    path.file_name().unwrap_or_default().to_string_lossy(),
                ));
            }
            Err(e) => {
                self.state.set_error(format!("Failed to load: {}", e));
            }
        }
    }
}

impl eframe::App for OleApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply theme once
        if !self.theme_applied {
            CyberTheme::apply(ctx);
            self.theme_applied = true;
        }

        // Drain audio events
        self.drain_audio_events();

        // Process scan progress
        self.process_scan_progress();

        // Update animations
        self.state.update_animations();

        // Handle keyboard input
        let commands = handle_keyboard(ctx, &mut self.state);
        for cmd in commands {
            self.handle_command(cmd);
        }

        // Check quit
        if self.state.should_quit {
            self.send_audio(AudioCommand::Shutdown);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Render UI (returns any widget-generated commands like seek)
        let widget_cmds = render_ui(ctx, &mut self.state, &self.cmd_tx);
        for cmd in widget_cmds {
            self.handle_command(cmd);
        }

        // Request continuous repaint for animations
        ctx.request_repaint();
    }
}

fn render_ui(ctx: &egui::Context, state: &mut GuiState, cmd_tx: &Sender<AudioCommand>) -> Vec<Command> {
    let mut commands = Vec::new();

    // Top panel - status bar
    egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
        widgets::StatusBar::show(ui, state);
    });

    // Bottom panel - status/mode
    egui::TopBottomPanel::bottom("bottom_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            let mode_text = format!("[{:?}]", state.mode);
            ui.label(egui::RichText::new(mode_text).color(crate::theme::PRIMARY));
            if !state.command_buffer.is_empty() {
                ui.label(
                    egui::RichText::new(format!(":{}", state.command_buffer))
                        .color(crate::theme::TEXT),
                );
            }
            if let Some(ref msg) = state.message {
                let color = match state.message_type {
                    MessageType::Info => crate::theme::TEXT,
                    MessageType::Success => crate::theme::PRIMARY,
                    MessageType::Warning => crate::theme::WARNING,
                    MessageType::Error => crate::theme::DANGER,
                };
                ui.label(egui::RichText::new(msg).color(color));
            }
        });
    });

    // Main content
    egui::CentralPanel::default().show(ctx, |ui| {
        // Decks row
        ui.columns(2, |cols| {
            if let Some(cmd) = widgets::DeckPanel::show(&mut cols[0], state, true) {
                commands.push(cmd);
            }
            if let Some(cmd) = widgets::DeckPanel::show(&mut cols[1], state, false) {
                commands.push(cmd);
            }
        });

        // Energy bridge between decks
        widgets::EnergyBridge::show(ui, state);

        ui.separator();

        // Effects + Mixer row
        ui.columns(3, |cols| {
            widgets::FxRack::show(&mut cols[0], state, cmd_tx, true);
            widgets::MixerPanel::show(&mut cols[1], state, cmd_tx);
            widgets::FxRack::show(&mut cols[2], state, cmd_tx, false);
        });

        ui.separator();

        // Visualization: Spectrum bars or Scope modes (TimeDomain/Lissajous/StereoField/Waterfall)
        if state.show_scope {
            widgets::ScopeWidget::show(ui, state);
        } else {
            widgets::SpectrumWidget::show(ui, state);
        }

        ui.separator();

        // Phase
        widgets::PhaseWidget::show(ui, state);

        // Library (if shown)
        if state.show_library {
            ui.separator();
            widgets::LibraryPanel::show(ui, state);
        }
    });

    // VFX overlays
    if state.scanlines_enabled {
        crate::vfx::draw_scanlines(ctx);
    }

    commands
}

fn vinyl_preset_to_audio(preset: ole_input::VinylPresetId) -> ole_audio::VinylPreset {
    match preset {
        ole_input::VinylPresetId::Subtle => ole_audio::VinylPreset::Clean,
        ole_input::VinylPresetId::Warm => ole_audio::VinylPreset::Warm,
        ole_input::VinylPresetId::Classic => ole_audio::VinylPreset::Vintage,
        ole_input::VinylPresetId::Aged => ole_audio::VinylPreset::Worn,
        ole_input::VinylPresetId::LoFi => ole_audio::VinylPreset::Extreme,
    }
}

use crate::state::MessageType;
