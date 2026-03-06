use std::path::PathBuf;
use std::sync::Arc;

use crossbeam_channel::Sender;
use eframe::egui;

use ole_analysis::phrase::{compute_energy_curve, detect_phrases};
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
        // Trigger FX flash on effect toggles
        if matches!(cmd, Command::ToggleEffect(_, _)) {
            self.state.fx_flash = 0.5;
        }
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
            Command::ToggleEffect(deck, effect_type) => {
                let cmd = match (deck, effect_type) {
                    (DeckId::A, EffectType::Filter) => AudioCommand::ToggleFilterA,
                    (DeckId::B, EffectType::Filter) => AudioCommand::ToggleFilterB,
                    (DeckId::A, EffectType::Delay) => AudioCommand::ToggleDelayA,
                    (DeckId::B, EffectType::Delay) => AudioCommand::ToggleDelayB,
                    (DeckId::A, EffectType::Reverb) => AudioCommand::ToggleReverbA,
                    (DeckId::B, EffectType::Reverb) => AudioCommand::ToggleReverbB,
                    (DeckId::A, EffectType::TapeStop) => AudioCommand::ToggleTapeStopA,
                    (DeckId::B, EffectType::TapeStop) => AudioCommand::ToggleTapeStopB,
                    (DeckId::A, EffectType::Flanger) => AudioCommand::ToggleFlangerA,
                    (DeckId::B, EffectType::Flanger) => AudioCommand::ToggleFlangerB,
                    (DeckId::A, EffectType::Bitcrusher) => AudioCommand::ToggleBitcrusherA,
                    (DeckId::B, EffectType::Bitcrusher) => AudioCommand::ToggleBitcrusherB,
                    (DeckId::A, EffectType::Phaser) => AudioCommand::TogglePhaserA,
                    (DeckId::B, EffectType::Phaser) => AudioCommand::TogglePhaserB,
                    (DeckId::A, EffectType::Gate) => AudioCommand::ToggleGateA,
                    (DeckId::B, EffectType::Gate) => AudioCommand::ToggleGateB,
                    (DeckId::A, EffectType::BeatRepeat) => AudioCommand::ToggleBeatRepeatA,
                    (DeckId::B, EffectType::BeatRepeat) => AudioCommand::ToggleBeatRepeatB,
                    (DeckId::A, EffectType::RingMod) => AudioCommand::ToggleRingModA,
                    (DeckId::B, EffectType::RingMod) => AudioCommand::ToggleRingModB,
                    (DeckId::A, EffectType::Shimmer) => AudioCommand::ToggleShimmerA,
                    (DeckId::B, EffectType::Shimmer) => AudioCommand::ToggleShimmerB,
                    (DeckId::A, EffectType::WashOut) => AudioCommand::ToggleWashOutA,
                    (DeckId::B, EffectType::WashOut) => AudioCommand::ToggleWashOutB,
                };
                self.send_audio(cmd);
                self.state.selected_effect = Some(effect_type);
            }
            Command::AdjustEffectMix(deck, effect_type, delta) => {
                let is_a = deck == DeckId::A;
                let current = self.state.get_effect_mix(is_a, effect_type);
                let new_mix = (current + delta).clamp(0.0, 1.0);
                let cmd = match deck {
                    DeckId::A => AudioCommand::SetEffectMixA(effect_type, new_mix),
                    DeckId::B => AudioCommand::SetEffectMixB(effect_type, new_mix),
                };
                self.send_audio(cmd);
                self.state.selected_effect = Some(effect_type);
                self.state.set_message(format!("{:?} mix: {:.0}%", effect_type, new_mix * 100.0));
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
            Command::LibraryFilterByBpmRange(min, max) => {
                // Jump to nearest track within BPM range
                let target = (min + max) / 2;
                if self.state.library.jump_to_bpm(target) {
                    self.state.set_message(format!("BPM {}-{}", min, max));
                }
            }
            Command::LibraryFilterCompatible => {
                self.state.library.filter_compatible();
                self.state.set_message("Showing compatible keys");
            }
            Command::LibraryClearFilter => {
                self.state.library.clear_filter();
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
                if let Some(track) = self.state.library.selected_track().cloned() {
                    let path = track.path.clone();
                    let key = track.key.clone();
                    self.state.library.add_to_history(&track);
                    self.state.library.current_playing_key = key.clone();
                    self.load_track(deck, &path, key);
                }
            }

            // Library enhancements (Phase 4)
            Command::LibrarySearch(q) => {
                self.state.library.search_query = q;
                self.state.library.selected_index = 0;
                self.state.library.needs_scroll = true;
            }
            Command::LibrarySearchAppend(c) => {
                self.state.library.search_query.push(c);
                self.state.library.selected_index = 0;
                self.state.library.needs_scroll = true;
            }
            Command::LibrarySearchBackspace => {
                self.state.library.search_query.pop();
                self.state.library.selected_index = 0;
                self.state.library.needs_scroll = true;
            }
            Command::LibrarySearchClear => {
                self.state.library.search_query.clear();
                self.state.library.selected_index = 0;
                self.state.library.needs_scroll = true;
            }
            Command::LibraryCycleSort => {
                self.state.library.cycle_sort();
                self.state.set_message(format!("Sort: {}", self.state.library.sort_column.label()));
            }
            Command::LibraryReverseSort => {
                self.state.library.reverse_sort();
                let dir = if self.state.library.sort_ascending { "ASC" } else { "DESC" };
                self.state.set_message(format!("Sort: {} {}", self.state.library.sort_column.label(), dir));
            }
            Command::LibraryShowHistory => {
                self.state.library.show_history = !self.state.library.show_history;
                let msg = if self.state.library.show_history { "Track history" } else { "Library" };
                self.state.set_message(msg);
            }
            Command::LibraryPageDown => self.state.library.page_down(),
            Command::LibraryPageUp => self.state.library.page_up(),

            // Sampler
            Command::TriggerSampler(slot) => {
                self.send_audio(AudioCommand::TriggerSampler(slot));
            }
            Command::StopSampler(slot) => {
                self.send_audio(AudioCommand::StopSampler(slot));
            }
            Command::LoadSamplerSlot(slot, path) => {
                match self.track_loader.load(&path) {
                    Ok(track) => {
                        let name = if track.metadata.title.is_empty() {
                            path.file_stem().map(|s| s.to_string_lossy().to_string())
                        } else {
                            Some(track.metadata.title.clone())
                        };
                        self.send_audio(AudioCommand::LoadSamplerSlot(
                            slot,
                            Arc::new(track.samples),
                            track.sample_rate,
                            name,
                        ));
                        self.state.set_success(format!("Sampler {} loaded", slot + 1));
                    }
                    Err(e) => {
                        self.state.set_error(format!("Failed to load sample: {}", e));
                    }
                }
            }
            Command::ClearSamplerSlot(slot) => {
                self.send_audio(AudioCommand::ClearSamplerSlot(slot));
                self.state.set_message(format!("Sampler {} cleared", slot + 1));
            }
            Command::ToggleSamplerLoop(slot) => {
                let current = self.state.sampler_slots.get(slot as usize)
                    .map(|(_, _, loop_enabled, _)| *loop_enabled).unwrap_or(false);
                self.send_audio(AudioCommand::SetSamplerLoop(slot, !current));
            }

            // Effect Macros (predefined multi-effect combos)
            Command::TriggerMacro(deck, macro_id) => {
                match macro_id {
                    1 => {
                        // "Build Up" - Force-enable Filter sweep + Delay + Reverb
                        let (filter_on, set_filter, delay_on, set_delay, reverb_on, set_reverb) = match deck {
                            DeckId::A => (
                                !self.state.filter_a_enabled, AudioCommand::SetFilterPresetA(ole_audio::FilterType::HighPass, 5),
                                !self.state.delay_a_enabled, AudioCommand::SetDelayLevelA(3),
                                !self.state.reverb_a_enabled, AudioCommand::SetReverbLevelA(3),
                            ),
                            DeckId::B => (
                                !self.state.filter_b_enabled, AudioCommand::SetFilterPresetB(ole_audio::FilterType::HighPass, 5),
                                !self.state.delay_b_enabled, AudioCommand::SetDelayLevelB(3),
                                !self.state.reverb_b_enabled, AudioCommand::SetReverbLevelB(3),
                            ),
                        };
                        // Only toggle if not already enabled
                        if filter_on {
                            self.send_audio(match deck { DeckId::A => AudioCommand::ToggleFilterA, DeckId::B => AudioCommand::ToggleFilterB });
                        }
                        self.send_audio(set_filter);
                        if delay_on {
                            self.send_audio(match deck { DeckId::A => AudioCommand::ToggleDelayA, DeckId::B => AudioCommand::ToggleDelayB });
                        }
                        self.send_audio(set_delay);
                        if reverb_on {
                            self.send_audio(match deck { DeckId::A => AudioCommand::ToggleReverbA, DeckId::B => AudioCommand::ToggleReverbB });
                        }
                        self.send_audio(set_reverb);
                        self.state.set_message(format!("Macro: BUILD UP (Deck {:?})", deck));
                    }
                    2 => {
                        // "Drop" - Force-disable all effects (filter off, delay off, reverb off)
                        let (filter_on, delay_on, reverb_on) = match deck {
                            DeckId::A => (self.state.filter_a_enabled, self.state.delay_a_enabled, self.state.reverb_a_enabled),
                            DeckId::B => (self.state.filter_b_enabled, self.state.delay_b_enabled, self.state.reverb_b_enabled),
                        };
                        if filter_on {
                            self.send_audio(match deck { DeckId::A => AudioCommand::ToggleFilterA, DeckId::B => AudioCommand::ToggleFilterB });
                        }
                        if delay_on {
                            self.send_audio(match deck { DeckId::A => AudioCommand::ToggleDelayA, DeckId::B => AudioCommand::ToggleDelayB });
                        }
                        if reverb_on {
                            self.send_audio(match deck { DeckId::A => AudioCommand::ToggleReverbA, DeckId::B => AudioCommand::ToggleReverbB });
                        }
                        self.state.set_message(format!("Macro: DROP (Deck {:?})", deck));
                    }
                    3 => {
                        // "Dub Echo" - Force-enable Delay + Shimmer
                        let (delay_on, shimmer_on) = match deck {
                            DeckId::A => (!self.state.delay_a_enabled, !self.state.shimmer_a_enabled),
                            DeckId::B => (!self.state.delay_b_enabled, !self.state.shimmer_b_enabled),
                        };
                        if delay_on {
                            self.send_audio(match deck { DeckId::A => AudioCommand::ToggleDelayA, DeckId::B => AudioCommand::ToggleDelayB });
                        }
                        self.send_audio(match deck { DeckId::A => AudioCommand::SetDelayLevelA(4), DeckId::B => AudioCommand::SetDelayLevelB(4) });
                        if shimmer_on {
                            self.send_audio(match deck { DeckId::A => AudioCommand::ToggleShimmerA, DeckId::B => AudioCommand::ToggleShimmerB });
                        }
                        self.state.set_message(format!("Macro: DUB ECHO (Deck {:?})", deck));
                    }
                    4 => {
                        // "Glitch" - Force-enable Gate + Bitcrusher + BeatRepeat
                        let (gate_on, crush_on) = match deck {
                            DeckId::A => (!self.state.gate_a_enabled, !self.state.bitcrusher_a_enabled),
                            DeckId::B => (!self.state.gate_b_enabled, !self.state.bitcrusher_b_enabled),
                        };
                        if gate_on {
                            self.send_audio(match deck { DeckId::A => AudioCommand::ToggleGateA, DeckId::B => AudioCommand::ToggleGateB });
                        }
                        if crush_on {
                            self.send_audio(match deck { DeckId::A => AudioCommand::ToggleBitcrusherA, DeckId::B => AudioCommand::ToggleBitcrusherB });
                        }
                        // BeatRepeat uses trigger (not toggle), so always send
                        self.send_audio(match deck { DeckId::A => AudioCommand::TriggerBeatRepeatA, DeckId::B => AudioCommand::TriggerBeatRepeatB });
                        self.state.set_message(format!("Macro: GLITCH (Deck {:?})", deck));
                    }
                    _ => {}
                }
                self.state.fx_flash = 0.8; // Strong flash for macros
            }

            // Recording
            Command::ToggleRecording => {
                if self.state.is_recording {
                    self.send_audio(AudioCommand::StopRecording);
                    self.state.set_success("Recording stopped");
                } else {
                    self.send_audio(AudioCommand::StartRecording);
                    self.state.set_message("Recording started");
                }
            }
            Command::SaveRecording(path) => {
                // Stop recording if currently recording
                if self.state.is_recording {
                    self.send_audio(AudioCommand::StopRecording);
                }
                // Save happens in the future after recording buffer is available
                // For now, notify the user
                self.state.set_message(format!("Saving to {}...", path.display()));
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
            Command::EnterEffectsMode => {
                self.state.selected_effect = crate::state::fx_cursor_effect_type(self.state.fx_cursor);
            }
            Command::EnterCommandMode
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
                self.state.selected_effect = Some(EffectType::Flanger);
                self.state.set_message("Flanger toggled");
            }

            // Bitcrusher
            Command::ToggleBitcrusher(deck) => {
                match deck {
                    DeckId::A => self.send_audio(AudioCommand::ToggleBitcrusherA),
                    DeckId::B => self.send_audio(AudioCommand::ToggleBitcrusherB),
                }
                self.state.selected_effect = Some(EffectType::Bitcrusher);
                self.state.set_message("Bitcrusher toggled");
            }

            // Phaser
            Command::TogglePhaser(deck) => {
                match deck {
                    DeckId::A => self.send_audio(AudioCommand::TogglePhaserA),
                    DeckId::B => self.send_audio(AudioCommand::TogglePhaserB),
                }
                self.state.selected_effect = Some(EffectType::Phaser);
                self.state.set_message("Phaser toggled");
            }

            // Gate
            Command::ToggleGate(deck) => {
                match deck {
                    DeckId::A => self.send_audio(AudioCommand::ToggleGateA),
                    DeckId::B => self.send_audio(AudioCommand::ToggleGateB),
                }
                self.state.selected_effect = Some(EffectType::Gate);
                self.state.set_message("Gate toggled");
            }

            // Beat Repeat
            Command::ToggleBeatRepeat(deck) => {
                match deck {
                    DeckId::A => self.send_audio(AudioCommand::ToggleBeatRepeatA),
                    DeckId::B => self.send_audio(AudioCommand::ToggleBeatRepeatB),
                }
                self.state.selected_effect = Some(EffectType::BeatRepeat);
                self.state.set_message("Beat Repeat toggled");
            }
            Command::TriggerBeatRepeat(deck) => {
                match deck {
                    DeckId::A => self.send_audio(AudioCommand::TriggerBeatRepeatA),
                    DeckId::B => self.send_audio(AudioCommand::TriggerBeatRepeatB),
                }
                self.state.selected_effect = Some(EffectType::BeatRepeat);
                self.state.set_message("Beat Repeat");
            }

            // Ring Modulator
            Command::ToggleRingMod(deck) => {
                match deck {
                    DeckId::A => self.send_audio(AudioCommand::ToggleRingModA),
                    DeckId::B => self.send_audio(AudioCommand::ToggleRingModB),
                }
                self.state.selected_effect = Some(EffectType::RingMod);
                self.state.set_message("Ring Mod toggled");
            }

            // Shimmer Reverb
            Command::ToggleShimmer(deck) => {
                match deck {
                    DeckId::A => self.send_audio(AudioCommand::ToggleShimmerA),
                    DeckId::B => self.send_audio(AudioCommand::ToggleShimmerB),
                }
                self.state.selected_effect = Some(EffectType::Shimmer);
                self.state.set_message("Shimmer toggled");
            }

            // Wash Out
            Command::ToggleWashOut(deck) => {
                match deck {
                    DeckId::A => self.send_audio(AudioCommand::ToggleWashOutA),
                    DeckId::B => self.send_audio(AudioCommand::ToggleWashOutB),
                }
                self.state.selected_effect = Some(EffectType::WashOut);
                self.state.set_message("Wash Out toggled");
            }
            Command::SetWashAmount(deck, amount) => {
                match deck {
                    DeckId::A => self.send_audio(AudioCommand::SetWashAmountA(amount)),
                    DeckId::B => self.send_audio(AudioCommand::SetWashAmountB(amount)),
                }
            }

            // Delay mode
            Command::CycleDelayMode(deck) => {
                match deck {
                    DeckId::A => {
                        self.send_audio(AudioCommand::CycleDelayModeA);
                        let next = self.state.delay_a_mode.next();
                        self.state.delay_a_mode = next;
                        self.state.set_message(format!("Delay: {}", next.display_name()));
                    }
                    DeckId::B => {
                        self.send_audio(AudioCommand::CycleDelayModeB);
                        let next = self.state.delay_b_mode.next();
                        self.state.delay_b_mode = next;
                        self.state.set_message(format!("Delay: {}", next.display_name()));
                    }
                }
            }

            // Looping
            Command::SetLoopIn(DeckId::A) => {
                self.send_audio(AudioCommand::SetLoopInA);
                self.state.set_message("Loop IN set");
            }
            Command::SetLoopIn(DeckId::B) => {
                self.send_audio(AudioCommand::SetLoopInB);
                self.state.set_message("Loop IN set");
            }
            Command::SetLoopOut(DeckId::A) => {
                self.send_audio(AudioCommand::SetLoopOutA);
                self.state.set_message("Loop OUT set - loop active");
            }
            Command::SetLoopOut(DeckId::B) => {
                self.send_audio(AudioCommand::SetLoopOutB);
                self.state.set_message("Loop OUT set - loop active");
            }
            Command::ToggleLoop(DeckId::A) => self.send_audio(AudioCommand::ToggleLoopA),
            Command::ToggleLoop(DeckId::B) => self.send_audio(AudioCommand::ToggleLoopB),
            Command::ClearLoop(DeckId::A) => {
                self.send_audio(AudioCommand::ClearLoopA);
                self.state.set_message("Loop cleared");
            }
            Command::ClearLoop(DeckId::B) => {
                self.send_audio(AudioCommand::ClearLoopB);
                self.state.set_message("Loop cleared");
            }
            Command::AutoLoop(DeckId::A, beats) => {
                self.send_audio(AudioCommand::AutoLoopA(beats));
                self.state.set_message(format!("Auto-loop {} beats", beats));
            }
            Command::AutoLoop(DeckId::B, beats) => {
                self.send_audio(AudioCommand::AutoLoopB(beats));
                self.state.set_message(format!("Auto-loop {} beats", beats));
            }
            Command::LoopHalve(DeckId::A) => self.send_audio(AudioCommand::LoopHalveA),
            Command::LoopHalve(DeckId::B) => self.send_audio(AudioCommand::LoopHalveB),
            Command::LoopDouble(DeckId::A) => self.send_audio(AudioCommand::LoopDoubleA),
            Command::LoopDouble(DeckId::B) => self.send_audio(AudioCommand::LoopDoubleB),
            Command::LoopRollStart(DeckId::A, beats) => {
                self.send_audio(AudioCommand::LoopRollStartA(beats));
                self.state.set_message(format!("Loop roll {} beats", beats));
            }
            Command::LoopRollStart(DeckId::B, beats) => {
                self.send_audio(AudioCommand::LoopRollStartB(beats));
                self.state.set_message(format!("Loop roll {} beats", beats));
            }
            Command::LoopRollEnd(DeckId::A) => self.send_audio(AudioCommand::LoopRollEndA),
            Command::LoopRollEnd(DeckId::B) => self.send_audio(AudioCommand::LoopRollEndB),

            // 3-Band EQ
            Command::AdjustEqLow(DeckId::A, d) => self.send_audio(AudioCommand::AdjustEqLowA(d)),
            Command::AdjustEqLow(DeckId::B, d) => self.send_audio(AudioCommand::AdjustEqLowB(d)),
            Command::AdjustEqMid(DeckId::A, d) => self.send_audio(AudioCommand::AdjustEqMidA(d)),
            Command::AdjustEqMid(DeckId::B, d) => self.send_audio(AudioCommand::AdjustEqMidB(d)),
            Command::AdjustEqHigh(DeckId::A, d) => self.send_audio(AudioCommand::AdjustEqHighA(d)),
            Command::AdjustEqHigh(DeckId::B, d) => self.send_audio(AudioCommand::AdjustEqHighB(d)),
            Command::KillEqLow(DeckId::A) => self.send_audio(AudioCommand::KillEqLowA),
            Command::KillEqLow(DeckId::B) => self.send_audio(AudioCommand::KillEqLowB),
            Command::KillEqMid(DeckId::A) => self.send_audio(AudioCommand::KillEqMidA),
            Command::KillEqMid(DeckId::B) => self.send_audio(AudioCommand::KillEqMidB),
            Command::KillEqHigh(DeckId::A) => self.send_audio(AudioCommand::KillEqHighA),
            Command::KillEqHigh(DeckId::B) => self.send_audio(AudioCommand::KillEqHighB),

            // Quantize
            Command::ToggleQuantize(DeckId::A) => {
                self.send_audio(AudioCommand::ToggleQuantizeA);
                self.state.set_message("Quantize toggled (Deck A)");
            }
            Command::ToggleQuantize(DeckId::B) => {
                self.send_audio(AudioCommand::ToggleQuantizeB);
                self.state.set_message("Quantize toggled (Deck B)");
            }
            Command::CycleQuantizeResolution(DeckId::A) => {
                self.send_audio(AudioCommand::CycleQuantizeResolutionA);
            }
            Command::CycleQuantizeResolution(DeckId::B) => {
                self.send_audio(AudioCommand::CycleQuantizeResolutionB);
            }

            // Key Lock
            Command::ToggleKeyLock(DeckId::A) => {
                self.send_audio(AudioCommand::ToggleKeyLockA);
                self.state.set_message("Key Lock toggled (Deck A)");
            }
            Command::ToggleKeyLock(DeckId::B) => {
                self.send_audio(AudioCommand::ToggleKeyLockB);
                self.state.set_message("Key Lock toggled (Deck B)");
            }

            // Slip Mode
            Command::ToggleSlip(DeckId::A) => {
                self.send_audio(AudioCommand::ToggleSlipA);
                self.state.set_message("Slip mode toggled (Deck A)");
            }
            Command::ToggleSlip(DeckId::B) => {
                self.send_audio(AudioCommand::ToggleSlipB);
                self.state.set_message("Slip mode toggled (Deck B)");
            }

            // Help scrolling
            Command::HelpScrollUp => {
                self.state.help_scroll = (self.state.help_scroll - 30.0).max(0.0);
                self.state.help_scroll_dirty = true;
            }
            Command::HelpScrollDown => {
                self.state.help_scroll = (self.state.help_scroll + 30.0).min(2000.0);
                self.state.help_scroll_dirty = true;
            }

            // DJ Copilot
            Command::ToggleCopilot => {
                self.state.library.copilot_enabled = !self.state.library.copilot_enabled;
                if self.state.library.copilot_enabled {
                    self.state.library.sort_column = crate::state::SortColumn::Score;
                    self.state.library.sort_ascending = false;
                    self.state.library.compute_copilot_scores();
                    self.state.set_success("Copilot ON");
                } else {
                    self.state.library.copilot_scores.clear();
                    self.state.library.sort_column = crate::state::SortColumn::Key;
                    self.state.library.sort_ascending = true;
                    self.state.set_message("Copilot OFF");
                }
            }
            Command::CycleEnergyDirection => {
                self.state.library.energy_direction = self.state.library.energy_direction.next();
                self.state.library.compute_copilot_scores();
                self.state.set_message(format!(
                    "Energy: {}",
                    self.state.library.energy_direction.label()
                ));
            }
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

                // Compute phrase intelligence from enhanced waveform
                let energy_curve = compute_energy_curve(&enhanced_waveform, 10);
                let duration = track.metadata.duration_secs;
                let step_secs = if !energy_curve.is_empty() {
                    duration / energy_curve.len() as f64
                } else {
                    1.0
                };

                // Try to get BPM from library cache for phrase detection
                let cached_bpm = self.state.library.tracks.iter()
                    .find(|t| t.path == path)
                    .and_then(|t| t.bpm);
                let bpm_for_phrases = cached_bpm.unwrap_or(128.0);
                let phrase_markers = detect_phrases(
                    bpm_for_phrases,
                    0.0, // first beat offset determined by audio engine
                    duration,
                    &energy_curve,
                    step_secs,
                );

                let energy_curve = Arc::new(energy_curve);
                let phrase_markers = Arc::new(phrase_markers);

                // Update per-deck copilot tracking
                let energy_level = Some(ole_analysis::compute_energy_level(
                    &samples,
                ));
                match deck {
                    DeckId::A => {
                        self.state.library.current_key_a = key.clone();
                        self.state.library.current_bpm_a = cached_bpm;
                        self.state.library.current_energy_a = energy_level;
                        // Also update current_playing_key for harmonic filter
                        self.state.library.current_playing_key = key.clone();
                    }
                    DeckId::B => {
                        self.state.library.current_key_b = key.clone();
                        self.state.library.current_bpm_b = cached_bpm;
                        self.state.library.current_energy_b = energy_level;
                    }
                }

                // Recompute copilot scores if enabled
                if self.state.library.copilot_enabled {
                    self.state.library.compute_copilot_scores();
                }

                match deck {
                    DeckId::A => self.send_audio(AudioCommand::LoadDeckA(
                        samples, track.sample_rate, name, waveform, enhanced_waveform, key,
                        energy_curve, phrase_markers,
                    )),
                    DeckId::B => self.send_audio(AudioCommand::LoadDeckB(
                        samples, track.sample_rate, name, waveform, enhanced_waveform, key,
                        energy_curve, phrase_markers,
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

    // Help overlay
    if state.show_help {
        egui::Area::new(egui::Id::new("help_overlay"))
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(egui::Color32::from_rgba_unmultiplied(0x0a, 0x0a, 0x0a, 0xE8))
                    .stroke(egui::Stroke::new(1.0, crate::theme::PRIMARY))
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.set_max_width(520.0);
                        ui.set_max_height(500.0);

                        ui.label(
                            egui::RichText::new("OLE - KEYBOARD REFERENCE")
                                .color(crate::theme::PRIMARY)
                                .strong()
                                .monospace()
                                .size(14.0),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("Press ? or Esc to close")
                                .color(crate::theme::TEXT_DIM)
                                .monospace()
                                .size(10.0),
                        );
                        ui.add_space(6.0);

                        let mut scroll_area = egui::ScrollArea::vertical()
                            .max_height(460.0);
                        if state.help_scroll_dirty {
                            scroll_area = scroll_area.scroll_offset(egui::Vec2::new(0.0, state.help_scroll));
                            state.help_scroll_dirty = false;
                        }
                        let scroll_output = scroll_area.show(ui, |ui| {
                                help_section(ui, "MODES", &[
                                    ("?", "Toggle help"),
                                    (":", "Command mode"),
                                    ("e", "Effects mode"),
                                    ("/ or o", "Browser mode"),
                                    ("Esc", "Back to Normal"),
                                    ("Ctrl+Q", "Quit"),
                                ]);
                                help_section(ui, "TRANSPORT", &[
                                    ("a / A", "Toggle play Deck A / B"),
                                    ("s / S", "Pause Deck A / B"),
                                    ("z / Z", "Stop Deck A / B"),
                                    ("Tab", "Cycle focus (A\u{2194}B)"),
                                ]);
                                help_section(ui, "MIXING", &[
                                    ("h / l", "Crossfader left / right"),
                                    ("\u{2190} / \u{2192}", "Crossfader left / right"),
                                    ("\\", "Center crossfader"),
                                    ("- / =", "Deck A gain down / up"),
                                    ("_ / +", "Deck B gain down / up"),
                                ]);
                                help_section(ui, "TEMPO & SYNC", &[
                                    ("[ / ]", "Deck A tempo \u{00b1}0.1%"),
                                    ("{ / }", "Deck A tempo \u{00b1}1%"),
                                    ("Alt+[ / Alt+]", "Deck A tempo \u{00b1}10%"),
                                    (", / .", "Deck B tempo \u{00b1}0.1%"),
                                    ("< / >", "Deck B tempo \u{00b1}1%"),
                                    ("Alt+, / Alt+.", "Deck B tempo \u{00b1}10%"),
                                    ("b / B", "Sync B\u{2192}A / A\u{2192}B"),
                                ]);
                                help_section(ui, "NAVIGATION", &[
                                    ("j / k", "Beatjump -1 / +1 (focused)"),
                                    ("J / K", "Beatjump -8 / +8"),
                                    ("\u{2193} / \u{2191}", "Beatjump -4 / +4"),
                                    ("x / c", "Nudge Deck A back / fwd"),
                                    ("X / C", "Nudge Deck B back / fwd"),
                                    ("d / D", "Beat nudge fwd / back"),
                                ]);
                                help_section(ui, "LOOPING (focused deck)", &[
                                    ("f", "Auto-loop 4 beats"),
                                    ("F", "Toggle loop on/off"),
                                    ("g / G", "Loop halve / double"),
                                    ("i / u", "Set loop in / out"),
                                    ("r", "Clear loop"),
                                ]);
                                help_section(ui, "CUE POINTS", &[
                                    ("1-8", "Jump to cue 1-8 (focused)"),
                                    ("Shift+1-8", "Set cue 1-8 (focused)"),
                                ]);
                                help_section(ui, "FEATURES", &[
                                    ("q / Q", "Quantize toggle / cycle resolution"),
                                    ("t", "Toggle key lock"),
                                    ("y", "Toggle slip mode"),
                                    ("Ctrl+P", "Toggle DJ Copilot"),
                                    ("F9", "Toggle recording"),
                                ]);
                                help_section(ui, "VISUALIZATION", &[
                                    ("v", "Toggle scope / spectrum"),
                                    ("V", "Cycle: Scope\u{2192}Lissajous\u{2192}Stereo\u{2192}Waterfall"),
                                    ("w / W", "Waveform zoom in / out"),
                                    ("p / P", "Cycle mastering preset / toggle"),
                                ]);
                                help_section(ui, "EFFECTS MODE (e)", &[
                                    ("Esc", "Back to Normal"),
                                ]);
                                help_hint(ui, "Toggle an FX to turn it on/off. Toggling");
                                help_hint(ui, "also selects it for mix adjustment.");
                                help_section(ui, "  Toggle on/off", &[
                                    ("g", "Flanger"),
                                    ("c", "Bitcrusher"),
                                    ("p", "Phaser"),
                                    ("x", "Gate"),
                                    ("n", "Ring mod"),
                                    ("s", "Shimmer reverb"),
                                    ("w", "Wash out"),
                                    ("v", "Vinyl"),
                                    ("Shift+D", "Delay"),
                                    ("Shift+R", "Reverb"),
                                    ("Shift+M", "Filter"),
                                    ("t / T", "Tape stop / start"),
                                    ("r", "Trigger beat repeat"),
                                ]);
                                help_hint(ui, "After toggling, use arrows to set dry/wet.");
                                help_hint(ui, "Hold arrow to sweep quickly.");
                                help_section(ui, "  Dry/wet mix", &[
                                    ("\u{2190} / \u{2192}", "\u{00b1}10% mix on selected FX"),
                                ]);
                                help_hint(ui, "The FX rack shows > next to selected FX");
                                help_hint(ui, "and a bar \u{2588}\u{2588}\u{2588}\u{2591}\u{2591} for mix level.");
                                help_section(ui, "  Filter & delay modes", &[
                                    ("m", "Cycle filter mode (LP/HP/BP)"),
                                    ("d", "Cycle delay mode"),
                                ]);
                                help_section(ui, "  EQ kills", &[
                                    ("z / a / q", "Kill low / mid / high"),
                                ]);
                                help_section(ui, "  Performance", &[
                                    ("1-7", "Loop roll (0.25\u{2192}16 beats)"),
                                    ("F1-F4", "Macros: Build/Drop/Dub Echo/Glitch"),
                                    ("Shift+1-8", "Trigger sampler 1-8"),
                                    ("Ctrl+1-8", "Stop sampler 1-8"),
                                    ("F9", "Toggle recording"),
                                ]);
                                help_section(ui, "BROWSER MODE (/ or o)", &[
                                    ("Esc", "Back to Normal"),
                                    ("j / k", "Select next / prev track"),
                                    ("\u{2193} / \u{2191}", "Select next / prev track"),
                                    ("g / G", "Select first / last"),
                                    ("Ctrl+D / Ctrl+U", "Page down / up"),
                                    ("a / b", "Load to Deck A / B"),
                                    ("Enter", "Load to focused deck"),
                                    ("f", "Filter compatible keys"),
                                    ("c", "Clear filter"),
                                    ("l", "Toggle library panel"),
                                    ("s / S", "Cycle sort / reverse"),
                                    ("/", "Search (type to filter, Esc to cancel)"),
                                    ("h", "Toggle history view"),
                                    ("e", "Cycle energy direction (copilot)"),
                                ]);
                                help_section(ui, "HELP MODE (?)", &[
                                    ("j / k", "Scroll down / up"),
                                    ("\u{2193} / \u{2191}", "Scroll down / up"),
                                    ("q / Esc / ?", "Close help"),
                                ]);
                                help_section(ui, "DJ COPILOT", &[
                                    ("Ctrl+P", "Toggle copilot on/off"),
                                ]);
                                help_hint(ui, "Copilot scores every track by how well");
                                help_hint(ui, "it mixes with what's currently playing.");
                                help_hint(ui, "");
                                help_hint(ui, "When on, the library title shows:");
                                help_hint(ui, "  LIBRARY [COPILOT \u{25b2} 42/128]");
                                help_hint(ui, "  \u{25b2} = Build  \u{25bc} = Drop  = = Maintain");
                                help_hint(ui, "");
                                help_hint(ui, "Tracks get a score bar: \u{2588}\u{2588}\u{2588}\u{2588}\u{2591} = 80%");
                                help_hint(ui, "  Bright  = great match (>60%)");
                                help_hint(ui, "  Normal  = decent match (30-60%)");
                                help_hint(ui, "  Dim     = poor match (<30%)");
                                help_hint(ui, "");
                                help_hint(ui, "Score is based on: key compatibility (40%),");
                                help_hint(ui, "BPM match (35%), energy fit (15%),");
                                help_hint(ui, "and play history (10%).");
                                help_hint(ui, "");
                                help_hint(ui, "In Browser mode (/ or o):");
                                help_section(ui, "", &[
                                    ("e", "Cycle energy: Maintain \u{2192} Build \u{2192} Drop"),
                                ]);
                                help_hint(ui, "  Maintain = similar energy level");
                                help_hint(ui, "  Build    = prefer higher energy tracks");
                                help_hint(ui, "  Drop     = prefer lower energy tracks");
                                help_section(ui, "COMMANDS (:)", &[
                                    (":scan <dir>", "Scan directory for tracks"),
                                    (":rescan", "Rescan last directory"),
                                    (":load <deck> <path>", "Load track to deck"),
                                    (":sync [a|b]", "Sync deck"),
                                    (":lib / :library", "Toggle library"),
                                    (":help", "Toggle help"),
                                    (":rec / :record", "Toggle recording"),
                                    (":save [path]", "Save recording to file"),
                                    (":sample <1-8> <path>", "Load sample to slot"),
                                    (":q / :quit", "Quit"),
                                ]);
                            });
                        state.help_scroll = scroll_output.state.offset.y;
                    });
            });
    }

    // VFX overlays
    if state.scanlines_enabled {
        crate::vfx::draw_scanlines(ctx, state.crt_intensity.max(1));
    }
    if state.glow_enabled {
        crate::vfx::draw_glow(ctx, state.beat_pulse_a, state.beat_pulse_b);
    }
    if state.noise_enabled {
        crate::vfx::draw_noise(ctx, state.frame_count, state.crt_intensity.max(1));
    }
    if state.chromatic_enabled {
        crate::vfx::draw_chromatic(ctx, state.crt_intensity.max(1));
    }
    if state.crt_intensity > 0 {
        crate::vfx::draw_vignette(ctx, state.crt_intensity);
    }
    if state.glitch_intensity > 0.01 {
        crate::vfx::draw_glitch(ctx, state.glitch_intensity, state.frame_count);
    }
    if state.drop_flash > 0.01 {
        crate::vfx::draw_drop_flash(ctx, state.drop_flash);
    }
    if state.fx_flash > 0.01 {
        crate::vfx::draw_fx_flash(ctx, state.fx_flash);
    }
    // Background grid (always on, subtle)
    {
        let bass = state.deck_a.spectrum.bands.get(..4).map(|s: &[f32]| s.iter().sum::<f32>()).unwrap_or(0.0)
            + state.deck_b.spectrum.bands.get(..4).map(|s: &[f32]| s.iter().sum::<f32>()).unwrap_or(0.0);
        crate::vfx::draw_background_grid(ctx, bass, state.frame_count);
    }

    commands
}

fn help_section(ui: &mut egui::Ui, title: &str, bindings: &[(&str, &str)]) {
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(title)
            .color(crate::theme::ACCENT_CYAN)
            .strong()
            .monospace()
            .size(11.0),
    );
    for (key, desc) in bindings {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("{:>16}", key))
                    .color(crate::theme::PRIMARY)
                    .monospace()
                    .size(11.0),
            );
            ui.label(
                egui::RichText::new(*desc)
                    .color(crate::theme::TEXT)
                    .monospace()
                    .size(11.0),
            );
        });
    }
}

fn help_hint(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(format!("  {}", text))
            .color(crate::theme::TEXT_DIM)
            .monospace()
            .size(10.0),
    );
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
