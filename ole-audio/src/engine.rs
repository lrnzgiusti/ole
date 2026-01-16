//! Audio engine - orchestrates decks, mixer, and effects

use crate::deck::{Deck, DeckState};
use crate::mixer::Mixer;
use crate::effects::{Filter, FilterType, Delay, DelayModulation, Reverb, Effect, FilterMode, LadderFilter, StateVariableFilter, SvfOutputType};
use crate::vinyl::{VinylEmulator, VinylPreset};
use crate::timestretcher::{PhaseVocoder, FftSize};
use crossbeam_channel::{Receiver, Sender, bounded};
use ole_analysis::EnhancedWaveform;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Commands sent to the audio engine
#[derive(Debug, Clone)]
pub enum AudioCommand {
    // Deck commands (samples, sample_rate, name, waveform_overview, enhanced_waveform, key)
    // Using Arc to avoid copying large sample data through channels
    LoadDeckA(Arc<Vec<f32>>, u32, Option<String>, Arc<Vec<f32>>, Arc<EnhancedWaveform>, Option<String>),
    LoadDeckB(Arc<Vec<f32>>, u32, Option<String>, Arc<Vec<f32>>, Arc<EnhancedWaveform>, Option<String>),
    PlayA,
    PlayB,
    PauseA,
    PauseB,
    StopA,
    StopB,
    ToggleA,
    ToggleB,
    SeekA(f64),
    SeekB(f64),
    NudgeA(f64),
    NudgeB(f64),
    BeatjumpA(i32),   // Jump by N beats
    BeatjumpB(i32),
    SetCueA(u8),      // Set cue point 1-4
    SetCueB(u8),
    JumpCueA(u8),     // Jump to cue point 1-4
    JumpCueB(u8),
    SetTempoA(f32),
    SetTempoB(f32),
    AdjustTempoA(f32),
    AdjustTempoB(f32),
    SetGainA(f32),
    SetGainB(f32),
    AdjustGainA(f32),
    AdjustGainB(f32),

    // Sync commands
    SyncBToA,
    SyncAToB,

    // Mixer commands
    SetCrossfader(f32),
    MoveCrossfader(f32),
    CenterCrossfader,
    SetMasterVolume(f32),

    // Effect commands for deck A
    ToggleFilterA,
    SetFilterTypeA(FilterType),
    SetFilterCutoffA(f32),
    AdjustFilterCutoffA(f32),
    ToggleDelayA,
    SetDelayTimeA(f32),
    SetDelayFeedbackA(f32),
    ToggleReverbA,

    // Effect commands for deck B
    ToggleFilterB,
    SetFilterTypeB(FilterType),
    SetFilterCutoffB(f32),
    AdjustFilterCutoffB(f32),
    ToggleDelayB,
    SetDelayTimeB(f32),
    SetDelayFeedbackB(f32),
    ToggleReverbB,

    // Preset-based effect commands (level 1-5 for delay/reverb, 1-10 for filter)
    SetDelayLevelA(u8),
    SetDelayLevelB(u8),
    SetFilterPresetA(FilterType, u8),
    SetFilterPresetB(FilterType, u8),
    SetReverbLevelA(u8),
    SetReverbLevelB(u8),

    // Filter mode selection (Biquad, Ladder, SVF)
    SetFilterModeA(FilterMode),
    SetFilterModeB(FilterMode),
    SetFilterResonanceA(f32),
    SetFilterResonanceB(f32),
    SetFilterDriveA(f32),  // Ladder filter only
    SetFilterDriveB(f32),

    // Vinyl emulation
    ToggleVinylA,
    ToggleVinylB,
    SetVinylPresetA(VinylPreset),
    SetVinylPresetB(VinylPreset),
    SetVinylWowA(f32),      // 0.0-1.0
    SetVinylWowB(f32),
    SetVinylNoiseA(f32),    // 0.0-1.0
    SetVinylNoiseB(f32),
    SetVinylWarmthA(f32),   // 0.0-1.0
    SetVinylWarmthB(f32),

    // Time stretching (phase vocoder)
    ToggleTimeStretchA,
    ToggleTimeStretchB,
    SetTimeStretchRatioA(f32),  // 0.25-4.0
    SetTimeStretchRatioB(f32),

    // Delay modulation
    SetDelayModulationA(DelayModulation),
    SetDelayModulationB(DelayModulation),

    // System
    Shutdown,
}

/// Events sent from the audio engine
#[derive(Debug, Clone)]
pub enum AudioEvent {
    /// State update for UI rendering
    StateUpdate {
        deck_a: Box<DeckState>,
        deck_b: Box<DeckState>,
        crossfader: f32,
        master_volume: f32,
        // Filter state
        filter_a_enabled: bool,
        filter_a_cutoff: f32,
        filter_a_type: FilterType,
        filter_a_level: u8,
        filter_a_mode: FilterMode,
        filter_b_enabled: bool,
        filter_b_cutoff: f32,
        filter_b_type: FilterType,
        filter_b_level: u8,
        filter_b_mode: FilterMode,
        // Delay state
        delay_a_enabled: bool,
        delay_a_level: u8,
        delay_a_modulation: DelayModulation,
        delay_b_enabled: bool,
        delay_b_level: u8,
        delay_b_modulation: DelayModulation,
        // Reverb state
        reverb_a_enabled: bool,
        reverb_a_level: u8,
        reverb_b_enabled: bool,
        reverb_b_level: u8,
        // Vinyl emulation state
        vinyl_a_enabled: bool,
        vinyl_a_preset: VinylPreset,
        vinyl_b_enabled: bool,
        vinyl_b_preset: VinylPreset,
        // Time stretch state
        time_stretch_a_enabled: bool,
        time_stretch_a_ratio: f32,
        time_stretch_b_enabled: bool,
        time_stretch_b_ratio: f32,
    },
    /// Track loaded successfully
    TrackLoaded { deck: char },
    /// Error occurred
    Error(String),
}

/// Maximum buffer size for pre-allocated processing buffers
/// Sized for 2048 stereo samples (typical maximum)
const MAX_BUFFER_SIZE: usize = 4096;

/// Audio engine state (held in audio thread)
pub struct EngineState {
    pub deck_a: Deck,
    pub deck_b: Deck,
    pub mixer: Mixer,
    // Original biquad filters
    pub filter_a: Filter,
    pub filter_b: Filter,
    // New ladder filters (Moog-style)
    pub ladder_a: LadderFilter,
    pub ladder_b: LadderFilter,
    // New SVF filters
    pub svf_a: StateVariableFilter,
    pub svf_b: StateVariableFilter,
    // Other effects
    pub delay_a: Delay,
    pub reverb_a: Reverb,
    pub delay_b: Delay,
    pub reverb_b: Reverb,
    // Vinyl emulation
    pub vinyl_a: VinylEmulator,
    pub vinyl_b: VinylEmulator,
    // Phase vocoder for time stretching
    pub phase_vocoder_a: PhaseVocoder,
    pub phase_vocoder_b: PhaseVocoder,
    sample_rate: u32,
    // Current effect levels (0 = off, 1-5 for delay/reverb, 1-10 for filter)
    filter_a_level: u8,
    filter_b_level: u8,
    delay_a_level: u8,
    delay_b_level: u8,
    delay_a_modulation: DelayModulation,
    delay_b_modulation: DelayModulation,
    // Filter mode selection
    filter_mode_a: FilterMode,
    filter_mode_b: FilterMode,
    // Pre-allocated processing buffers (avoids allocation in audio callback)
    buffer_a: Vec<f32>,
    buffer_b: Vec<f32>,
}

impl EngineState {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            deck_a: Deck::new(sample_rate),
            deck_b: Deck::new(sample_rate),
            mixer: Mixer::new(),
            // Original biquad filters
            filter_a: Filter::new(sample_rate as f32),
            filter_b: Filter::new(sample_rate as f32),
            // New ladder filters (Moog-style)
            ladder_a: LadderFilter::new(sample_rate as f32),
            ladder_b: LadderFilter::new(sample_rate as f32),
            // New SVF filters
            svf_a: StateVariableFilter::new(sample_rate as f32),
            svf_b: StateVariableFilter::new(sample_rate as f32),
            // Other effects
            delay_a: Delay::new(sample_rate),
            reverb_a: Reverb::new(sample_rate),
            delay_b: Delay::new(sample_rate),
            reverb_b: Reverb::new(sample_rate),
            // Vinyl emulation (disabled by default)
            vinyl_a: VinylEmulator::new(sample_rate as f32),
            vinyl_b: VinylEmulator::new(sample_rate as f32),
            // Phase vocoder (disabled by default, medium FFT size for balance)
            phase_vocoder_a: PhaseVocoder::new(FftSize::Medium),
            phase_vocoder_b: PhaseVocoder::new(FftSize::Medium),
            sample_rate,
            filter_a_level: 0,
            filter_b_level: 0,
            delay_a_level: 0,
            delay_b_level: 0,
            delay_a_modulation: DelayModulation::Off,
            delay_b_modulation: DelayModulation::Off,
            filter_mode_a: FilterMode::default(),
            filter_mode_b: FilterMode::default(),
            // Pre-allocate buffers to avoid allocation in audio callback
            buffer_a: vec![0.0f32; MAX_BUFFER_SIZE],
            buffer_b: vec![0.0f32; MAX_BUFFER_SIZE],
        }
    }

    /// Lookup table for delay level (1-5) to delay time in ms
    /// Index 0 is default, indices 1-5 map to levels 1-5
    const DELAY_LEVEL_MS: [f32; 6] = [250.0, 100.0, 200.0, 300.0, 400.0, 500.0];

    /// Lookup table for filter level (1-10) to cutoff frequency in Hz
    /// Index 0 is default, indices 1-10 map to levels 1-10
    const FILTER_LEVEL_CUTOFF: [f32; 11] = [
        1000.0,  // default (index 0)
        200.0, 400.0, 600.0, 1000.0, 2000.0,  // levels 1-5
        4000.0, 6000.0, 10000.0, 15000.0, 20000.0,  // levels 6-10
    ];

    /// Map delay level (1-5) to delay time in ms
    #[inline]
    fn delay_level_to_ms(level: u8) -> f32 {
        Self::DELAY_LEVEL_MS.get(level as usize).copied().unwrap_or(250.0)
    }

    /// Map filter level (1-10) to cutoff frequency in Hz
    #[inline]
    fn filter_level_to_cutoff(level: u8) -> f32 {
        Self::FILTER_LEVEL_CUTOFF.get(level as usize).copied().unwrap_or(1000.0)
    }

    /// Process a command
    pub fn handle_command(&mut self, cmd: AudioCommand) {
        match cmd {
            // Deck A commands
            AudioCommand::LoadDeckA(samples, sr, name, waveform, enhanced, key) => self.deck_a.load(samples, sr, name, waveform, enhanced, key),
            AudioCommand::PlayA => self.deck_a.play(),
            AudioCommand::PauseA => self.deck_a.pause(),
            AudioCommand::StopA => self.deck_a.stop(),
            AudioCommand::ToggleA => self.deck_a.toggle(),
            AudioCommand::SeekA(pos) => self.deck_a.seek(pos),
            AudioCommand::NudgeA(delta) => self.deck_a.nudge(delta),
            AudioCommand::BeatjumpA(beats) => self.deck_a.beatjump(beats),
            AudioCommand::SetCueA(num) => self.deck_a.set_cue(num),
            AudioCommand::JumpCueA(num) => self.deck_a.jump_cue(num),
            AudioCommand::SetTempoA(tempo) => self.deck_a.set_tempo(tempo),
            AudioCommand::AdjustTempoA(delta) => self.deck_a.adjust_tempo(delta),
            AudioCommand::SetGainA(gain) => self.deck_a.set_gain(gain),
            AudioCommand::AdjustGainA(delta) => self.deck_a.adjust_gain(delta),

            // Deck B commands
            AudioCommand::LoadDeckB(samples, sr, name, waveform, enhanced, key) => self.deck_b.load(samples, sr, name, waveform, enhanced, key),
            AudioCommand::PlayB => self.deck_b.play(),
            AudioCommand::PauseB => self.deck_b.pause(),
            AudioCommand::StopB => self.deck_b.stop(),
            AudioCommand::ToggleB => self.deck_b.toggle(),
            AudioCommand::SeekB(pos) => self.deck_b.seek(pos),
            AudioCommand::NudgeB(delta) => self.deck_b.nudge(delta),
            AudioCommand::BeatjumpB(beats) => self.deck_b.beatjump(beats),
            AudioCommand::SetCueB(num) => self.deck_b.set_cue(num),
            AudioCommand::JumpCueB(num) => self.deck_b.jump_cue(num),
            AudioCommand::SetTempoB(tempo) => self.deck_b.set_tempo(tempo),
            AudioCommand::AdjustTempoB(delta) => self.deck_b.adjust_tempo(delta),
            AudioCommand::SetGainB(gain) => self.deck_b.set_gain(gain),
            AudioCommand::AdjustGainB(delta) => self.deck_b.adjust_gain(delta),

            // Sync commands - smart sync with phase alignment
            AudioCommand::SyncBToA => {
                self.smart_sync_b_to_a();
            }
            AudioCommand::SyncAToB => {
                self.smart_sync_a_to_b();
            }

            // Mixer commands
            AudioCommand::SetCrossfader(pos) => self.mixer.set_crossfader(pos),
            AudioCommand::MoveCrossfader(delta) => self.mixer.move_crossfader(delta),
            AudioCommand::CenterCrossfader => self.mixer.center_crossfader(),
            AudioCommand::SetMasterVolume(vol) => self.mixer.set_master_volume(vol),

            // Effect commands - Deck A
            AudioCommand::ToggleFilterA => {
                // Toggle the currently selected filter mode
                match self.filter_mode_a {
                    FilterMode::Biquad => {
                        let enabled = !self.filter_a.is_enabled();
                        self.filter_a.set_enabled(enabled);
                    }
                    FilterMode::Ladder => {
                        let enabled = !self.ladder_a.is_enabled();
                        self.ladder_a.set_enabled(enabled);
                    }
                    FilterMode::SVF => {
                        let enabled = !self.svf_a.is_enabled();
                        self.svf_a.set_enabled(enabled);
                    }
                }
            }
            AudioCommand::SetFilterTypeA(ft) => {
                self.filter_a.set_type(ft);
                // Also update SVF output type if in SVF mode
                if self.filter_mode_a == FilterMode::SVF {
                    self.svf_a.set_output_type(match ft {
                        FilterType::LowPass => SvfOutputType::LowPass,
                        FilterType::HighPass => SvfOutputType::HighPass,
                        FilterType::BandPass => SvfOutputType::BandPass,
                    });
                }
            }
            AudioCommand::SetFilterCutoffA(cutoff) => {
                // Update biquad (source of truth) and active filter only
                self.filter_a.set_cutoff(cutoff);
                match self.filter_mode_a {
                    FilterMode::Ladder => self.ladder_a.set_cutoff(cutoff),
                    FilterMode::SVF => self.svf_a.set_cutoff(cutoff),
                    FilterMode::Biquad => {} // Already updated above
                }
            }
            AudioCommand::AdjustFilterCutoffA(delta) => {
                let current = self.filter_a.cutoff();
                // Exponential adjustment for more natural feel
                let factor: f32 = if delta > 0.0 { 1.1 } else { 0.9 };
                let new_cutoff = current * factor.powf(delta.abs());
                // Update biquad (source of truth) and active filter only
                self.filter_a.set_cutoff(new_cutoff);
                match self.filter_mode_a {
                    FilterMode::Ladder => self.ladder_a.set_cutoff(new_cutoff),
                    FilterMode::SVF => self.svf_a.set_cutoff(new_cutoff),
                    FilterMode::Biquad => {} // Already updated above
                }
            }
            AudioCommand::ToggleDelayA => {
                let enabled = !self.delay_a.is_enabled();
                self.delay_a.set_enabled(enabled);
            }
            AudioCommand::SetDelayTimeA(ms) => self.delay_a.set_delay_ms(ms),
            AudioCommand::SetDelayFeedbackA(fb) => self.delay_a.set_feedback(fb),

            // Effect commands - Deck B
            AudioCommand::ToggleFilterB => {
                // Toggle the currently selected filter mode
                match self.filter_mode_b {
                    FilterMode::Biquad => {
                        let enabled = !self.filter_b.is_enabled();
                        self.filter_b.set_enabled(enabled);
                    }
                    FilterMode::Ladder => {
                        let enabled = !self.ladder_b.is_enabled();
                        self.ladder_b.set_enabled(enabled);
                    }
                    FilterMode::SVF => {
                        let enabled = !self.svf_b.is_enabled();
                        self.svf_b.set_enabled(enabled);
                    }
                }
            }
            AudioCommand::SetFilterTypeB(ft) => {
                self.filter_b.set_type(ft);
                // Also update SVF output type if in SVF mode
                if self.filter_mode_b == FilterMode::SVF {
                    self.svf_b.set_output_type(match ft {
                        FilterType::LowPass => SvfOutputType::LowPass,
                        FilterType::HighPass => SvfOutputType::HighPass,
                        FilterType::BandPass => SvfOutputType::BandPass,
                    });
                }
            }
            AudioCommand::SetFilterCutoffB(cutoff) => {
                // Update biquad (source of truth) and active filter only
                self.filter_b.set_cutoff(cutoff);
                match self.filter_mode_b {
                    FilterMode::Ladder => self.ladder_b.set_cutoff(cutoff),
                    FilterMode::SVF => self.svf_b.set_cutoff(cutoff),
                    FilterMode::Biquad => {} // Already updated above
                }
            }
            AudioCommand::AdjustFilterCutoffB(delta) => {
                let current = self.filter_b.cutoff();
                let factor: f32 = if delta > 0.0 { 1.1 } else { 0.9 };
                let new_cutoff = current * factor.powf(delta.abs());
                // Update biquad (source of truth) and active filter only
                self.filter_b.set_cutoff(new_cutoff);
                match self.filter_mode_b {
                    FilterMode::Ladder => self.ladder_b.set_cutoff(new_cutoff),
                    FilterMode::SVF => self.svf_b.set_cutoff(new_cutoff),
                    FilterMode::Biquad => {} // Already updated above
                }
            }
            AudioCommand::ToggleDelayB => {
                let enabled = !self.delay_b.is_enabled();
                self.delay_b.set_enabled(enabled);
            }
            AudioCommand::SetDelayTimeB(ms) => self.delay_b.set_delay_ms(ms),
            AudioCommand::SetDelayFeedbackB(fb) => self.delay_b.set_feedback(fb),

            // Reverb toggle commands
            AudioCommand::ToggleReverbA => {
                let enabled = !self.reverb_a.is_enabled();
                self.reverb_a.set_enabled(enabled);
            }
            AudioCommand::ToggleReverbB => {
                let enabled = !self.reverb_b.is_enabled();
                self.reverb_b.set_enabled(enabled);
            }

            // Preset-based effect commands
            AudioCommand::SetDelayLevelA(level) => {
                if level == 0 {
                    self.delay_a.set_enabled(false);
                    self.delay_a_level = 0;
                } else {
                    self.delay_a.set_delay_ms(Self::delay_level_to_ms(level));
                    self.delay_a.set_enabled(true);
                    self.delay_a_level = level;
                }
            }
            AudioCommand::SetDelayLevelB(level) => {
                if level == 0 {
                    self.delay_b.set_enabled(false);
                    self.delay_b_level = 0;
                } else {
                    self.delay_b.set_delay_ms(Self::delay_level_to_ms(level));
                    self.delay_b.set_enabled(true);
                    self.delay_b_level = level;
                }
            }
            AudioCommand::SetFilterPresetA(filter_type, level) => {
                if level == 0 {
                    self.filter_a.set_enabled(false);
                    self.filter_a_level = 0;
                } else {
                    self.filter_a.set_type(filter_type);
                    self.filter_a.set_cutoff(Self::filter_level_to_cutoff(level));
                    self.filter_a.set_enabled(true);
                    self.filter_a_level = level;
                }
            }
            AudioCommand::SetFilterPresetB(filter_type, level) => {
                if level == 0 {
                    self.filter_b.set_enabled(false);
                    self.filter_b_level = 0;
                } else {
                    self.filter_b.set_type(filter_type);
                    self.filter_b.set_cutoff(Self::filter_level_to_cutoff(level));
                    self.filter_b.set_enabled(true);
                    self.filter_b_level = level;
                }
            }
            AudioCommand::SetReverbLevelA(level) => {
                if level == 0 {
                    self.reverb_a.set_enabled(false);
                } else {
                    self.reverb_a.set_level(level);
                }
            }
            AudioCommand::SetReverbLevelB(level) => {
                if level == 0 {
                    self.reverb_b.set_enabled(false);
                } else {
                    self.reverb_b.set_level(level);
                }
            }

            // Filter mode and parameter commands
            AudioCommand::SetFilterModeA(mode) => {
                self.filter_mode_a = mode;
                // Sync cutoff and resonance to the new filter
                let cutoff = self.filter_a.cutoff();
                let resonance = self.filter_a.resonance();
                match mode {
                    FilterMode::Ladder => {
                        self.ladder_a.set_cutoff(cutoff);
                        self.ladder_a.set_resonance(resonance / 20.0); // Scale Q to 0-1
                    }
                    FilterMode::SVF => {
                        self.svf_a.set_cutoff(cutoff);
                        self.svf_a.set_resonance(resonance / 20.0);
                    }
                    FilterMode::Biquad => {}
                }
            }
            AudioCommand::SetFilterModeB(mode) => {
                self.filter_mode_b = mode;
                let cutoff = self.filter_b.cutoff();
                let resonance = self.filter_b.resonance();
                match mode {
                    FilterMode::Ladder => {
                        self.ladder_b.set_cutoff(cutoff);
                        self.ladder_b.set_resonance(resonance / 20.0);
                    }
                    FilterMode::SVF => {
                        self.svf_b.set_cutoff(cutoff);
                        self.svf_b.set_resonance(resonance / 20.0);
                    }
                    FilterMode::Biquad => {}
                }
            }
            AudioCommand::SetFilterResonanceA(res) => {
                let res_clamped = res.clamp(0.0, 1.0);
                match self.filter_mode_a {
                    FilterMode::Biquad => self.filter_a.set_resonance(0.5 + res_clamped * 19.5),
                    FilterMode::Ladder => self.ladder_a.set_resonance(res_clamped),
                    FilterMode::SVF => self.svf_a.set_resonance(res_clamped),
                }
            }
            AudioCommand::SetFilterResonanceB(res) => {
                let res_clamped = res.clamp(0.0, 1.0);
                match self.filter_mode_b {
                    FilterMode::Biquad => self.filter_b.set_resonance(0.5 + res_clamped * 19.5),
                    FilterMode::Ladder => self.ladder_b.set_resonance(res_clamped),
                    FilterMode::SVF => self.svf_b.set_resonance(res_clamped),
                }
            }
            AudioCommand::SetFilterDriveA(drive) => {
                self.ladder_a.set_drive(drive);
            }
            AudioCommand::SetFilterDriveB(drive) => {
                self.ladder_b.set_drive(drive);
            }

            // Vinyl emulation commands
            AudioCommand::ToggleVinylA => {
                let enabled = !self.vinyl_a.is_enabled();
                self.vinyl_a.set_enabled(enabled);
            }
            AudioCommand::ToggleVinylB => {
                let enabled = !self.vinyl_b.is_enabled();
                self.vinyl_b.set_enabled(enabled);
            }
            AudioCommand::SetVinylPresetA(preset) => {
                self.vinyl_a.set_preset(preset);
            }
            AudioCommand::SetVinylPresetB(preset) => {
                self.vinyl_b.set_preset(preset);
            }
            AudioCommand::SetVinylWowA(amount) => {
                self.vinyl_a.set_wow_amount(amount);
            }
            AudioCommand::SetVinylWowB(amount) => {
                self.vinyl_b.set_wow_amount(amount);
            }
            AudioCommand::SetVinylNoiseA(amount) => {
                self.vinyl_a.set_noise_amount(amount);
            }
            AudioCommand::SetVinylNoiseB(amount) => {
                self.vinyl_b.set_noise_amount(amount);
            }
            AudioCommand::SetVinylWarmthA(amount) => {
                self.vinyl_a.set_warmth_amount(amount);
            }
            AudioCommand::SetVinylWarmthB(amount) => {
                self.vinyl_b.set_warmth_amount(amount);
            }

            // Time stretching commands
            AudioCommand::ToggleTimeStretchA => {
                let enabled = !self.phase_vocoder_a.is_enabled();
                self.phase_vocoder_a.set_enabled(enabled);
            }
            AudioCommand::ToggleTimeStretchB => {
                let enabled = !self.phase_vocoder_b.is_enabled();
                self.phase_vocoder_b.set_enabled(enabled);
            }
            AudioCommand::SetTimeStretchRatioA(ratio) => {
                self.phase_vocoder_a.set_stretch_ratio(ratio);
            }
            AudioCommand::SetTimeStretchRatioB(ratio) => {
                self.phase_vocoder_b.set_stretch_ratio(ratio);
            }

            // Delay modulation commands
            AudioCommand::SetDelayModulationA(mode) => {
                self.delay_a.set_modulation(mode);
                self.delay_a_modulation = mode;
            }
            AudioCommand::SetDelayModulationB(mode) => {
                self.delay_b.set_modulation(mode);
                self.delay_b_modulation = mode;
            }

            AudioCommand::Shutdown => {} // Handled at higher level
        }
    }

    /// Generate current state for UI
    pub fn get_state(&self) -> AudioEvent {
        // Get enabled state based on current filter mode
        let filter_a_enabled = match self.filter_mode_a {
            FilterMode::Biquad => self.filter_a.is_enabled(),
            FilterMode::Ladder => self.ladder_a.is_enabled(),
            FilterMode::SVF => self.svf_a.is_enabled(),
        };
        let filter_b_enabled = match self.filter_mode_b {
            FilterMode::Biquad => self.filter_b.is_enabled(),
            FilterMode::Ladder => self.ladder_b.is_enabled(),
            FilterMode::SVF => self.svf_b.is_enabled(),
        };

        AudioEvent::StateUpdate {
            deck_a: Box::new(self.deck_a.state()),
            deck_b: Box::new(self.deck_b.state()),
            crossfader: self.mixer.crossfader(),
            master_volume: self.mixer.master_volume(),
            // Filter state
            filter_a_enabled,
            filter_a_cutoff: self.filter_a.cutoff(),
            filter_a_type: self.filter_a.filter_type(),
            filter_a_level: self.filter_a_level,
            filter_a_mode: self.filter_mode_a,
            filter_b_enabled,
            filter_b_cutoff: self.filter_b.cutoff(),
            filter_b_type: self.filter_b.filter_type(),
            filter_b_level: self.filter_b_level,
            filter_b_mode: self.filter_mode_b,
            // Delay state
            delay_a_enabled: self.delay_a.is_enabled(),
            delay_a_level: self.delay_a_level,
            delay_a_modulation: self.delay_a_modulation,
            delay_b_enabled: self.delay_b.is_enabled(),
            delay_b_level: self.delay_b_level,
            delay_b_modulation: self.delay_b_modulation,
            // Reverb state
            reverb_a_enabled: self.reverb_a.is_enabled(),
            reverb_a_level: self.reverb_a.level(),
            reverb_b_enabled: self.reverb_b.is_enabled(),
            reverb_b_level: self.reverb_b.level(),
            // Vinyl emulation state
            vinyl_a_enabled: self.vinyl_a.is_enabled(),
            vinyl_a_preset: self.vinyl_a.preset(),
            vinyl_b_enabled: self.vinyl_b.is_enabled(),
            vinyl_b_preset: self.vinyl_b.preset(),
            // Time stretch state
            time_stretch_a_enabled: self.phase_vocoder_a.is_enabled(),
            time_stretch_a_ratio: self.phase_vocoder_a.stretch_ratio(),
            time_stretch_b_enabled: self.phase_vocoder_b.is_enabled(),
            time_stretch_b_ratio: self.phase_vocoder_b.stretch_ratio(),
        }
    }

    /// Smart sync: sync Deck B's tempo and phase to Deck A
    ///
    /// This performs professional-style beat sync:
    /// 1. Matches tempo so both decks play at the same BPM
    /// 2. Aligns beat phases so transients (kicks) land together
    /// 3. Uses smooth transition to avoid jarring jumps
    fn smart_sync_b_to_a(&mut self) {
        // Get beat grids from both decks
        let (source_grid, source_phase) = match (self.deck_a.beat_grid(), self.deck_a.beat_phase()) {
            (Some(g), Some(p)) => (g, p),
            _ => {
                // Fallback to tempo-only sync if no beat grid
                self.tempo_only_sync_b_to_a();
                return;
            }
        };

        let target_grid = match self.deck_b.beat_grid() {
            Some(g) => g,
            None => {
                self.tempo_only_sync_b_to_a();
                return;
            }
        };

        // Step 1: Calculate target tempo to match BPMs
        let source_effective_bpm = source_grid.bpm * self.deck_a.state().tempo;
        // Use target deck's original BPM (from beat grid, not adjusted for tempo)
        let target_original_bpm = target_grid.bpm;
        let new_tempo = (source_effective_bpm / target_original_bpm).clamp(0.5, 2.0);

        // Step 2: Calculate phase offset needed to align beats
        let phase_offset = self.deck_b.phase_offset_to_align(source_phase).unwrap_or(0.0);

        // Step 3: Start smooth transition (~500ms at 44.1kHz)
        let transition_duration = (self.sample_rate as f64 * 0.5) as u64;
        self.deck_b.start_sync_transition(new_tempo, phase_offset, transition_duration);
    }

    /// Smart sync: sync Deck A's tempo and phase to Deck B
    fn smart_sync_a_to_b(&mut self) {
        // Get beat grids from both decks
        let (source_grid, source_phase) = match (self.deck_b.beat_grid(), self.deck_b.beat_phase()) {
            (Some(g), Some(p)) => (g, p),
            _ => {
                self.tempo_only_sync_a_to_b();
                return;
            }
        };

        let target_grid = match self.deck_a.beat_grid() {
            Some(g) => g,
            None => {
                self.tempo_only_sync_a_to_b();
                return;
            }
        };

        // Calculate target tempo
        let source_effective_bpm = source_grid.bpm * self.deck_b.state().tempo;
        let target_original_bpm = target_grid.bpm;
        let new_tempo = (source_effective_bpm / target_original_bpm).clamp(0.5, 2.0);

        // Calculate phase offset
        let phase_offset = self.deck_a.phase_offset_to_align(source_phase).unwrap_or(0.0);

        // Start smooth transition
        let transition_duration = (self.sample_rate as f64 * 0.5) as u64;
        self.deck_a.start_sync_transition(new_tempo, phase_offset, transition_duration);
    }

    /// Fallback tempo-only sync (no phase alignment)
    fn tempo_only_sync_b_to_a(&mut self) {
        if let (Some(bpm_a), Some(_bpm_b)) = (self.deck_a.current_bpm(), self.deck_b.current_bpm()) {
            if let Some(original_b) = self.deck_b.state().bpm.map(|b| b / self.deck_b.state().tempo) {
                let new_tempo = bpm_a / original_b;
                self.deck_b.set_tempo(new_tempo);
            }
        }
    }

    /// Fallback tempo-only sync (no phase alignment)
    fn tempo_only_sync_a_to_b(&mut self) {
        if let (Some(_bpm_a), Some(bpm_b)) = (self.deck_a.current_bpm(), self.deck_b.current_bpm()) {
            if let Some(original_a) = self.deck_a.state().bpm.map(|b| b / self.deck_a.state().tempo) {
                let new_tempo = bpm_b / original_a;
                self.deck_a.set_tempo(new_tempo);
            }
        }
    }

    /// Process audio for output buffer
    pub fn process(&mut self, output: &mut [f32]) {
        let len = output.len();

        // Ensure pre-allocated buffers are large enough
        // This should rarely happen after the first call
        if len > self.buffer_a.len() {
            self.buffer_a.resize(len, 0.0);
            self.buffer_b.resize(len, 0.0);
        }

        // Zero the buffers (no allocation - just memset)
        self.buffer_a[..len].fill(0.0);
        self.buffer_b[..len].fill(0.0);

        // Use slices of pre-allocated buffers
        let (buf_a, buf_b) = {
            let (a, _) = self.buffer_a.split_at_mut(len);
            let (b, _) = self.buffer_b.split_at_mut(len);
            (a, b)
        };

        // Process each deck
        self.deck_a.process(buf_a);
        self.deck_b.process(buf_b);

        // Apply effects chain:
        // Deck → Vinyl Emulation → Filter → Delay → Reverb → Mixer

        // Deck A chain
        // 1. Vinyl emulation (adds warmth, noise, wow/flutter)
        self.vinyl_a.process(buf_a);

        // 2. Filter (mode-selected)
        match self.filter_mode_a {
            FilterMode::Biquad => self.filter_a.process(buf_a),
            FilterMode::Ladder => self.ladder_a.process(buf_a),
            FilterMode::SVF => self.svf_a.process(buf_a),
        }

        // 3. Delay
        self.delay_a.process(buf_a);

        // 4. Reverb
        self.reverb_a.process(buf_a);

        // Deck B chain
        // 1. Vinyl emulation
        self.vinyl_b.process(buf_b);

        // 2. Filter (mode-selected)
        match self.filter_mode_b {
            FilterMode::Biquad => self.filter_b.process(buf_b),
            FilterMode::Ladder => self.ladder_b.process(buf_b),
            FilterMode::SVF => self.svf_b.process(buf_b),
        }

        // 3. Delay
        self.delay_b.process(buf_b);

        // 4. Reverb
        self.reverb_b.process(buf_b);

        // Mix to output
        self.mixer.mix(buf_a, buf_b, output);
    }
}

/// Handle to communicate with the audio engine
pub struct AudioEngine {
    /// Send commands to audio thread
    pub command_tx: Sender<AudioCommand>,
    /// Receive events from audio thread
    pub event_rx: Receiver<AudioEvent>,
    /// Shutdown flag
    shutdown: Arc<AtomicBool>,
}

impl AudioEngine {
    /// Create channels for engine communication
    /// Buffer size of 1024 provides headroom for command bursts without saturation
    pub fn create_channels() -> (Sender<AudioCommand>, Receiver<AudioCommand>, Sender<AudioEvent>, Receiver<AudioEvent>) {
        let (cmd_tx, cmd_rx) = bounded(1024);
        let (evt_tx, evt_rx) = bounded(1024);
        (cmd_tx, cmd_rx, evt_tx, evt_rx)
    }

    /// Create a new engine handle
    pub fn new(command_tx: Sender<AudioCommand>, event_rx: Receiver<AudioEvent>) -> Self {
        Self {
            command_tx,
            event_rx,
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Send a command to the audio engine
    pub fn send(&self, cmd: AudioCommand) {
        let _ = self.command_tx.try_send(cmd);
    }

    /// Check if shutdown was requested
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Relaxed)
    }

    /// Request shutdown
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = self.command_tx.try_send(AudioCommand::Shutdown);
    }
}
