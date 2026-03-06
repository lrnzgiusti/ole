//! Audio engine - orchestrates decks, mixer, and effects

use crate::deck::{Deck, DeckState};
use crate::effects::{
    BeatRepeat, Bitcrusher, ChannelEq, Delay, DelayMode, DelayModulation, Effect, EffectType,
    Filter, FilterMode, FilterType, Flanger, Gate, GateDivision, LadderFilter, Limiter, Phaser,
    Reverb, RingModulator, ShimmerReverb, StateVariableFilter, SvfOutputType, TapeStop, WashOut,
};
use crate::mastering::{LufsValues, MasteringChain, MasteringPreset};
use crate::mixer::Mixer;
use crate::recording::RecordingState;
use crate::sampler::Sampler;
use crate::timestretcher::{FftSize, PhaseVocoder};
use crate::vinyl::{VinylEmulator, VinylPreset};
use crossbeam_channel::{bounded, Receiver, Sender};
use ole_analysis::{EnhancedWaveform, PhraseMarker};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Commands sent to the audio engine
#[derive(Debug, Clone)]
pub enum AudioCommand {
    // Deck commands (samples, sample_rate, name, waveform_overview, enhanced_waveform, key, energy_curve, phrase_markers)
    // Using Arc to avoid copying large sample data through channels
    LoadDeckA(
        Arc<Vec<f32>>,
        u32,
        Option<String>,
        Arc<Vec<f32>>,
        Arc<EnhancedWaveform>,
        Option<String>,
        Arc<Vec<f32>>,
        Arc<Vec<PhraseMarker>>,
    ),
    LoadDeckB(
        Arc<Vec<f32>>,
        u32,
        Option<String>,
        Arc<Vec<f32>>,
        Arc<EnhancedWaveform>,
        Option<String>,
        Arc<Vec<f32>>,
        Arc<Vec<PhraseMarker>>,
    ),
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
    BeatNudgeA(f32), // Nudge by fraction of beat (e.g., 0.0625 = 1/16)
    BeatNudgeB(f32),
    BeatjumpA(i32), // Jump by N beats
    BeatjumpB(i32),
    SetCueA(u8), // Set cue point 1-4
    SetCueB(u8),
    JumpCueA(u8), // Jump to cue point 1-4
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
    SetFilterDriveA(f32), // Ladder filter only
    SetFilterDriveB(f32),

    // Vinyl emulation
    ToggleVinylA,
    ToggleVinylB,
    SetVinylPresetA(VinylPreset),
    SetVinylPresetB(VinylPreset),
    SetVinylWowA(f32), // 0.0-1.0
    SetVinylWowB(f32),
    SetVinylNoiseA(f32), // 0.0-1.0
    SetVinylNoiseB(f32),
    SetVinylWarmthA(f32), // 0.0-1.0
    SetVinylWarmthB(f32),

    // Time stretching (phase vocoder)
    ToggleTimeStretchA,
    ToggleTimeStretchB,
    SetTimeStretchRatioA(f32), // 0.25-4.0
    SetTimeStretchRatioB(f32),

    // Delay modulation
    SetDelayModulationA(DelayModulation),
    SetDelayModulationB(DelayModulation),

    // Mastering chain
    ToggleMastering,
    SetMasteringPreset(MasteringPreset),
    CycleMasteringPreset,

    // Tape Stop effect
    ToggleTapeStopA,
    ToggleTapeStopB,
    TriggerTapeStopA,
    TriggerTapeStopB,
    TriggerTapeStartA,
    TriggerTapeStartB,
    SetTapeStopTimeA(f32),
    SetTapeStopTimeB(f32),

    // Flanger effect
    ToggleFlangerA,
    ToggleFlangerB,
    SetFlangerRateA(f32),
    SetFlangerRateB(f32),
    SetFlangerDepthA(f32),
    SetFlangerDepthB(f32),
    SetFlangerFeedbackA(f32),
    SetFlangerFeedbackB(f32),

    // Bitcrusher effect
    ToggleBitcrusherA,
    ToggleBitcrusherB,
    SetBitcrusherBitsA(u8),
    SetBitcrusherBitsB(u8),
    SetBitcrusherDownsampleA(u8),
    SetBitcrusherDownsampleB(u8),

    // Phaser effect
    TogglePhaserA,
    TogglePhaserB,

    // Gate effect
    ToggleGateA,
    ToggleGateB,
    SetGateDivisionA(GateDivision),
    SetGateDivisionB(GateDivision),

    // Beat Repeat effect
    ToggleBeatRepeatA,
    ToggleBeatRepeatB,
    TriggerBeatRepeatA,
    TriggerBeatRepeatB,

    // Ring Modulator effect
    ToggleRingModA,
    ToggleRingModB,

    // Shimmer Reverb effect
    ToggleShimmerA,
    ToggleShimmerB,

    // Wash Out effect
    ToggleWashOutA,
    ToggleWashOutB,
    SetWashAmountA(f32),
    SetWashAmountB(f32),

    // Generic effect mix (dry/wet)
    SetEffectMixA(EffectType, f32),
    SetEffectMixB(EffectType, f32),

    // Delay mode (Stereo/PingPong/Mono)
    SetDelayModeA(DelayMode),
    SetDelayModeB(DelayMode),
    CycleDelayModeA,
    CycleDelayModeB,

    // Looping
    SetLoopInA,
    SetLoopInB,
    SetLoopOutA,
    SetLoopOutB,
    ToggleLoopA,
    ToggleLoopB,
    ClearLoopA,
    ClearLoopB,
    AutoLoopA(f32), // beats
    AutoLoopB(f32),
    LoopHalveA,
    LoopHalveB,
    LoopDoubleA,
    LoopDoubleB,
    LoopRollStartA(f32),
    LoopRollStartB(f32),
    LoopRollEndA,
    LoopRollEndB,

    // 3-Band Channel EQ
    AdjustEqLowA(f32),
    AdjustEqLowB(f32),
    AdjustEqMidA(f32),
    AdjustEqMidB(f32),
    AdjustEqHighA(f32),
    AdjustEqHighB(f32),
    KillEqLowA,
    KillEqLowB,
    KillEqMidA,
    KillEqMidB,
    KillEqHighA,
    KillEqHighB,

    // Quantize
    ToggleQuantizeA,
    ToggleQuantizeB,
    CycleQuantizeResolutionA,
    CycleQuantizeResolutionB,

    // Key Lock
    ToggleKeyLockA,
    ToggleKeyLockB,

    // Slip Mode
    ToggleSlipA,
    ToggleSlipB,

    // Sampler
    LoadSamplerSlot(u8, Arc<Vec<f32>>, u32, Option<String>),
    ClearSamplerSlot(u8),
    TriggerSampler(u8),
    StopSampler(u8),
    SetSamplerGain(u8, f32),
    SetSamplerLoop(u8, bool),

    // Recording
    StartRecording,
    StopRecording,

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
        // Mastering state
        mastering_enabled: bool,
        mastering_preset: MasteringPreset,
        mastering_lufs: LufsValues,
        mastering_gain_reduction: f32,
        // New effects state
        flanger_a_enabled: bool,
        flanger_b_enabled: bool,
        bitcrusher_a_enabled: bool,
        bitcrusher_b_enabled: bool,
        tape_stop_a_enabled: bool,
        tape_stop_b_enabled: bool,
        phaser_a_enabled: bool,
        phaser_b_enabled: bool,
        gate_a_enabled: bool,
        gate_b_enabled: bool,
        beat_repeat_a_enabled: bool,
        beat_repeat_b_enabled: bool,
        ringmod_a_enabled: bool,
        ringmod_b_enabled: bool,
        shimmer_a_enabled: bool,
        shimmer_b_enabled: bool,
        washout_a_enabled: bool,
        washout_b_enabled: bool,
        washout_a_amount: f32,
        washout_b_amount: f32,
        delay_a_mode: DelayMode,
        delay_b_mode: DelayMode,
        // Effect mix levels (dry/wet 0.0-1.0)
        flanger_a_mix: f32,
        flanger_b_mix: f32,
        phaser_a_mix: f32,
        phaser_b_mix: f32,
        bitcrusher_a_mix: f32,
        bitcrusher_b_mix: f32,
        gate_a_mix: f32,
        gate_b_mix: f32,
        beat_repeat_a_mix: f32,
        beat_repeat_b_mix: f32,
        ringmod_a_mix: f32,
        ringmod_b_mix: f32,
        shimmer_a_mix: f32,
        shimmer_b_mix: f32,
        delay_a_mix: f32,
        delay_b_mix: f32,
        reverb_a_mix: f32,
        reverb_b_mix: f32,
        // Channel EQ state
        eq_a_low: f32,
        eq_a_mid: f32,
        eq_a_high: f32,
        eq_a_low_kill: bool,
        eq_a_mid_kill: bool,
        eq_a_high_kill: bool,
        eq_b_low: f32,
        eq_b_mid: f32,
        eq_b_high: f32,
        eq_b_low_kill: bool,
        eq_b_mid_kill: bool,
        eq_b_high_kill: bool,
        // Sampler state: (loaded, playing, loop_enabled, name) per slot
        sampler_slots: [(bool, bool, bool, Option<String>); 8],
        // Recording state
        is_recording: bool,
        recording_duration: f64,
    },
    /// Track loaded successfully
    TrackLoaded { deck: char },
    /// Error occurred
    Error(String),
}

/// Maximum buffer size for pre-allocated processing buffers
/// Sized for 2048 stereo samples (typical maximum)
const MAX_BUFFER_SIZE: usize = 8192;

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
    // Mastering chain (EQ, compressor, saturation, stereo enhancement)
    pub mastering: MasteringChain,
    // Master limiter (brickwall, always on for safety)
    pub master_limiter: Limiter,
    // Tape stop effect
    pub tape_stop_a: TapeStop,
    pub tape_stop_b: TapeStop,
    // Flanger effect
    pub flanger_a: Flanger,
    pub flanger_b: Flanger,
    // Bitcrusher effect
    pub bitcrusher_a: Bitcrusher,
    pub bitcrusher_b: Bitcrusher,
    // Phaser effect
    pub phaser_a: Phaser,
    pub phaser_b: Phaser,
    // Gate effect
    pub gate_a: Gate,
    pub gate_b: Gate,
    // Beat Repeat effect
    pub beat_repeat_a: BeatRepeat,
    pub beat_repeat_b: BeatRepeat,
    // Ring Modulator effect
    pub ringmod_a: RingModulator,
    pub ringmod_b: RingModulator,
    // Shimmer Reverb effect
    pub shimmer_a: ShimmerReverb,
    pub shimmer_b: ShimmerReverb,
    // Wash Out effect
    pub washout_a: WashOut,
    pub washout_b: WashOut,
    // Channel EQ (3-band per deck)
    pub eq_a: ChannelEq,
    pub eq_b: ChannelEq,
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
    // Sampler (8 one-shot/loop slots)
    pub sampler: Sampler,
    // Recording (master output capture)
    pub recording: RecordingState,
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
            // Mastering chain (enabled by default with Clean preset)
            mastering: MasteringChain::new(sample_rate as f32),
            // Master limiter (always on for safety, -0.1dBFS ceiling)
            master_limiter: Limiter::new(sample_rate as f32),
            // Tape stop effects
            tape_stop_a: TapeStop::new(sample_rate as f32),
            tape_stop_b: TapeStop::new(sample_rate as f32),
            // Flanger effects
            flanger_a: Flanger::new(sample_rate as f32),
            flanger_b: Flanger::new(sample_rate as f32),
            // Bitcrusher effects
            bitcrusher_a: Bitcrusher::new(sample_rate as f32),
            bitcrusher_b: Bitcrusher::new(sample_rate as f32),
            // Phaser effects
            phaser_a: Phaser::new(sample_rate as f32),
            phaser_b: Phaser::new(sample_rate as f32),
            // Gate effects
            gate_a: Gate::new(sample_rate as f32),
            gate_b: Gate::new(sample_rate as f32),
            // Beat Repeat effects
            beat_repeat_a: BeatRepeat::new(sample_rate as f32),
            beat_repeat_b: BeatRepeat::new(sample_rate as f32),
            // Ring Modulator effects
            ringmod_a: RingModulator::new(sample_rate as f32),
            ringmod_b: RingModulator::new(sample_rate as f32),
            // Shimmer Reverb effects
            shimmer_a: ShimmerReverb::new(sample_rate as f32),
            shimmer_b: ShimmerReverb::new(sample_rate as f32),
            // Wash Out effects
            washout_a: WashOut::new(sample_rate as f32),
            washout_b: WashOut::new(sample_rate as f32),
            // Channel EQ (3-band per deck)
            eq_a: ChannelEq::new(sample_rate as f32),
            eq_b: ChannelEq::new(sample_rate as f32),
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
            // Sampler
            sampler: Sampler::new(sample_rate),
            // Recording
            recording: RecordingState::new(sample_rate),
        }
    }

    /// Lookup table for delay level (1-5) to delay time in ms
    /// Index 0 is default, indices 1-5 map to levels 1-5
    const DELAY_LEVEL_MS: [f32; 6] = [250.0, 100.0, 200.0, 300.0, 400.0, 500.0];

    /// Lookup table for filter level (1-10) to cutoff frequency in Hz
    /// Index 0 is default, indices 1-10 map to levels 1-10
    const FILTER_LEVEL_CUTOFF: [f32; 11] = [
        1000.0, // default (index 0)
        200.0, 400.0, 600.0, 1000.0, 2000.0, // levels 1-5
        4000.0, 6000.0, 10000.0, 15000.0, 20000.0, // levels 6-10
    ];

    /// Map delay level (1-5) to delay time in ms
    #[inline]
    fn delay_level_to_ms(level: u8) -> f32 {
        Self::DELAY_LEVEL_MS
            .get(level as usize)
            .copied()
            .unwrap_or(250.0)
    }

    /// Map filter level (1-10) to cutoff frequency in Hz
    #[inline]
    fn filter_level_to_cutoff(level: u8) -> f32 {
        Self::FILTER_LEVEL_CUTOFF
            .get(level as usize)
            .copied()
            .unwrap_or(1000.0)
    }

    /// Process a command
    pub fn handle_command(&mut self, cmd: AudioCommand) {
        match cmd {
            // Deck A commands
            AudioCommand::LoadDeckA(samples, sr, name, waveform, enhanced, key, energy_curve, phrases) => {
                self.deck_a.load(samples, sr, name, waveform, enhanced, key);
                self.deck_a.set_phrase_data(energy_curve, phrases);
            }
            AudioCommand::PlayA => self.deck_a.play(),
            AudioCommand::PauseA => self.deck_a.pause(),
            AudioCommand::StopA => {
                self.deck_a.stop();
                self.delay_a.set_enabled(false);
            }
            AudioCommand::ToggleA => self.deck_a.toggle(),
            AudioCommand::SeekA(pos) => self.deck_a.seek(pos),
            AudioCommand::NudgeA(delta) => self.deck_a.nudge(delta),
            AudioCommand::BeatNudgeA(beats) => self.deck_a.beat_nudge(beats),
            AudioCommand::BeatjumpA(beats) => self.deck_a.beatjump(beats),
            AudioCommand::SetCueA(num) => self.deck_a.set_cue(num),
            AudioCommand::JumpCueA(num) => self.deck_a.jump_cue(num),
            AudioCommand::SetTempoA(tempo) => self.deck_a.set_tempo(tempo),
            AudioCommand::AdjustTempoA(delta) => self.deck_a.adjust_tempo(delta),
            AudioCommand::SetGainA(gain) => self.deck_a.set_gain(gain),
            AudioCommand::AdjustGainA(delta) => self.deck_a.adjust_gain(delta),

            // Deck B commands
            AudioCommand::LoadDeckB(samples, sr, name, waveform, enhanced, key, energy_curve, phrases) => {
                self.deck_b.load(samples, sr, name, waveform, enhanced, key);
                self.deck_b.set_phrase_data(energy_curve, phrases);
            }
            AudioCommand::PlayB => self.deck_b.play(),
            AudioCommand::PauseB => self.deck_b.pause(),
            AudioCommand::StopB => {
                self.deck_b.stop();
                self.delay_b.set_enabled(false);
            }
            AudioCommand::ToggleB => self.deck_b.toggle(),
            AudioCommand::SeekB(pos) => self.deck_b.seek(pos),
            AudioCommand::NudgeB(delta) => self.deck_b.nudge(delta),
            AudioCommand::BeatNudgeB(beats) => self.deck_b.beat_nudge(beats),
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
                    self.filter_a
                        .set_cutoff(Self::filter_level_to_cutoff(level));
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
                    self.filter_b
                        .set_cutoff(Self::filter_level_to_cutoff(level));
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

            // Mastering chain commands
            AudioCommand::ToggleMastering => {
                let enabled = !self.mastering.is_enabled();
                self.mastering.set_enabled(enabled);
            }
            AudioCommand::SetMasteringPreset(preset) => {
                self.mastering.set_preset(preset);
            }
            AudioCommand::CycleMasteringPreset => {
                self.mastering.cycle_preset();
            }

            // Tape Stop commands
            AudioCommand::ToggleTapeStopA => {
                let enabled = !self.tape_stop_a.is_enabled();
                self.tape_stop_a.set_enabled(enabled);
            }
            AudioCommand::ToggleTapeStopB => {
                let enabled = !self.tape_stop_b.is_enabled();
                self.tape_stop_b.set_enabled(enabled);
            }
            AudioCommand::TriggerTapeStopA => {
                self.tape_stop_a.set_enabled(true);
                self.tape_stop_a.trigger_stop();
            }
            AudioCommand::TriggerTapeStopB => {
                self.tape_stop_b.set_enabled(true);
                self.tape_stop_b.trigger_stop();
            }
            AudioCommand::TriggerTapeStartA => {
                self.tape_stop_a.trigger_start();
            }
            AudioCommand::TriggerTapeStartB => {
                self.tape_stop_b.trigger_start();
            }
            AudioCommand::SetTapeStopTimeA(time) => {
                self.tape_stop_a.set_stop_time(time);
            }
            AudioCommand::SetTapeStopTimeB(time) => {
                self.tape_stop_b.set_stop_time(time);
            }

            // Flanger commands
            AudioCommand::ToggleFlangerA => {
                let enabled = !self.flanger_a.is_enabled();
                self.flanger_a.set_enabled(enabled);
            }
            AudioCommand::ToggleFlangerB => {
                let enabled = !self.flanger_b.is_enabled();
                self.flanger_b.set_enabled(enabled);
            }
            AudioCommand::SetFlangerRateA(rate) => {
                self.flanger_a.set_rate(rate);
            }
            AudioCommand::SetFlangerRateB(rate) => {
                self.flanger_b.set_rate(rate);
            }
            AudioCommand::SetFlangerDepthA(depth) => {
                self.flanger_a.set_depth(depth);
            }
            AudioCommand::SetFlangerDepthB(depth) => {
                self.flanger_b.set_depth(depth);
            }
            AudioCommand::SetFlangerFeedbackA(fb) => {
                self.flanger_a.set_feedback(fb);
            }
            AudioCommand::SetFlangerFeedbackB(fb) => {
                self.flanger_b.set_feedback(fb);
            }

            // Bitcrusher commands
            AudioCommand::ToggleBitcrusherA => {
                let enabled = !self.bitcrusher_a.is_enabled();
                self.bitcrusher_a.set_enabled(enabled);
            }
            AudioCommand::ToggleBitcrusherB => {
                let enabled = !self.bitcrusher_b.is_enabled();
                self.bitcrusher_b.set_enabled(enabled);
            }
            AudioCommand::SetBitcrusherBitsA(bits) => {
                self.bitcrusher_a.set_bits(bits);
            }
            AudioCommand::SetBitcrusherBitsB(bits) => {
                self.bitcrusher_b.set_bits(bits);
            }
            AudioCommand::SetBitcrusherDownsampleA(ds) => {
                self.bitcrusher_a.set_downsample(ds);
            }
            AudioCommand::SetBitcrusherDownsampleB(ds) => {
                self.bitcrusher_b.set_downsample(ds);
            }

            // Phaser commands
            AudioCommand::TogglePhaserA => {
                let enabled = !self.phaser_a.is_enabled();
                self.phaser_a.set_enabled(enabled);
            }
            AudioCommand::TogglePhaserB => {
                let enabled = !self.phaser_b.is_enabled();
                self.phaser_b.set_enabled(enabled);
            }

            // Gate commands
            AudioCommand::ToggleGateA => {
                let enabled = !self.gate_a.is_enabled();
                self.gate_a.set_enabled(enabled);
            }
            AudioCommand::ToggleGateB => {
                let enabled = !self.gate_b.is_enabled();
                self.gate_b.set_enabled(enabled);
            }
            AudioCommand::SetGateDivisionA(div) => {
                self.gate_a.set_division(div);
            }
            AudioCommand::SetGateDivisionB(div) => {
                self.gate_b.set_division(div);
            }

            // Beat Repeat commands
            AudioCommand::ToggleBeatRepeatA => {
                let enabled = !self.beat_repeat_a.is_enabled();
                self.beat_repeat_a.set_enabled(enabled);
            }
            AudioCommand::ToggleBeatRepeatB => {
                let enabled = !self.beat_repeat_b.is_enabled();
                self.beat_repeat_b.set_enabled(enabled);
            }
            AudioCommand::TriggerBeatRepeatA => {
                self.beat_repeat_a.set_enabled(true);
                self.beat_repeat_a.trigger();
            }
            AudioCommand::TriggerBeatRepeatB => {
                self.beat_repeat_b.set_enabled(true);
                self.beat_repeat_b.trigger();
            }

            // Ring Modulator commands
            AudioCommand::ToggleRingModA => {
                let enabled = !self.ringmod_a.is_enabled();
                self.ringmod_a.set_enabled(enabled);
            }
            AudioCommand::ToggleRingModB => {
                let enabled = !self.ringmod_b.is_enabled();
                self.ringmod_b.set_enabled(enabled);
            }

            // Shimmer Reverb commands
            AudioCommand::ToggleShimmerA => {
                let enabled = !self.shimmer_a.is_enabled();
                self.shimmer_a.set_enabled(enabled);
            }
            AudioCommand::ToggleShimmerB => {
                let enabled = !self.shimmer_b.is_enabled();
                self.shimmer_b.set_enabled(enabled);
            }

            // Wash Out commands
            AudioCommand::ToggleWashOutA => {
                let enabled = !self.washout_a.is_enabled();
                self.washout_a.set_enabled(enabled);
            }
            AudioCommand::ToggleWashOutB => {
                let enabled = !self.washout_b.is_enabled();
                self.washout_b.set_enabled(enabled);
            }
            AudioCommand::SetWashAmountA(amount) => {
                self.washout_a.set_wash(amount);
            }
            AudioCommand::SetWashAmountB(amount) => {
                self.washout_b.set_wash(amount);
            }

            // Generic effect mix
            AudioCommand::SetEffectMixA(effect_type, mix) => {
                match effect_type {
                    EffectType::Flanger => self.flanger_a.set_mix(mix),
                    EffectType::Phaser => self.phaser_a.set_mix(mix),
                    EffectType::Bitcrusher => self.bitcrusher_a.set_mix(mix),
                    EffectType::Gate => self.gate_a.set_mix(mix),
                    EffectType::BeatRepeat => self.beat_repeat_a.set_mix(mix),
                    EffectType::RingMod => self.ringmod_a.set_mix(mix),
                    EffectType::Shimmer => self.shimmer_a.set_mix(mix),
                    EffectType::Delay => self.delay_a.set_mix(mix),
                    EffectType::Reverb => self.reverb_a.set_wet(mix),
                    _ => {} // Filter, TapeStop, WashOut have own controls
                }
            }
            AudioCommand::SetEffectMixB(effect_type, mix) => {
                match effect_type {
                    EffectType::Flanger => self.flanger_b.set_mix(mix),
                    EffectType::Phaser => self.phaser_b.set_mix(mix),
                    EffectType::Bitcrusher => self.bitcrusher_b.set_mix(mix),
                    EffectType::Gate => self.gate_b.set_mix(mix),
                    EffectType::BeatRepeat => self.beat_repeat_b.set_mix(mix),
                    EffectType::RingMod => self.ringmod_b.set_mix(mix),
                    EffectType::Shimmer => self.shimmer_b.set_mix(mix),
                    EffectType::Delay => self.delay_b.set_mix(mix),
                    EffectType::Reverb => self.reverb_b.set_wet(mix),
                    _ => {}
                }
            }

            // Delay mode commands
            AudioCommand::SetDelayModeA(mode) => {
                self.delay_a.set_mode(mode);
            }
            AudioCommand::SetDelayModeB(mode) => {
                self.delay_b.set_mode(mode);
            }
            AudioCommand::CycleDelayModeA => {
                let next = self.delay_a.mode().next();
                self.delay_a.set_mode(next);
            }
            AudioCommand::CycleDelayModeB => {
                let next = self.delay_b.mode().next();
                self.delay_b.set_mode(next);
            }

            // Looping commands
            AudioCommand::SetLoopInA => self.deck_a.set_loop_in(),
            AudioCommand::SetLoopInB => self.deck_b.set_loop_in(),
            AudioCommand::SetLoopOutA => self.deck_a.set_loop_out(),
            AudioCommand::SetLoopOutB => self.deck_b.set_loop_out(),
            AudioCommand::ToggleLoopA => self.deck_a.toggle_loop(),
            AudioCommand::ToggleLoopB => self.deck_b.toggle_loop(),
            AudioCommand::ClearLoopA => self.deck_a.clear_loop(),
            AudioCommand::ClearLoopB => self.deck_b.clear_loop(),
            AudioCommand::AutoLoopA(beats) => self.deck_a.auto_loop(beats),
            AudioCommand::AutoLoopB(beats) => self.deck_b.auto_loop(beats),
            AudioCommand::LoopHalveA => self.deck_a.loop_halve(),
            AudioCommand::LoopHalveB => self.deck_b.loop_halve(),
            AudioCommand::LoopDoubleA => self.deck_a.loop_double(),
            AudioCommand::LoopDoubleB => self.deck_b.loop_double(),
            AudioCommand::LoopRollStartA(beats) => self.deck_a.start_loop_roll(beats),
            AudioCommand::LoopRollStartB(beats) => self.deck_b.start_loop_roll(beats),
            AudioCommand::LoopRollEndA => self.deck_a.end_loop_roll(),
            AudioCommand::LoopRollEndB => self.deck_b.end_loop_roll(),

            // Channel EQ commands
            AudioCommand::AdjustEqLowA(d) => self.eq_a.adjust_low(d),
            AudioCommand::AdjustEqLowB(d) => self.eq_b.adjust_low(d),
            AudioCommand::AdjustEqMidA(d) => self.eq_a.adjust_mid(d),
            AudioCommand::AdjustEqMidB(d) => self.eq_b.adjust_mid(d),
            AudioCommand::AdjustEqHighA(d) => self.eq_a.adjust_high(d),
            AudioCommand::AdjustEqHighB(d) => self.eq_b.adjust_high(d),
            AudioCommand::KillEqLowA => self.eq_a.toggle_low_kill(),
            AudioCommand::KillEqLowB => self.eq_b.toggle_low_kill(),
            AudioCommand::KillEqMidA => self.eq_a.toggle_mid_kill(),
            AudioCommand::KillEqMidB => self.eq_b.toggle_mid_kill(),
            AudioCommand::KillEqHighA => self.eq_a.toggle_high_kill(),
            AudioCommand::KillEqHighB => self.eq_b.toggle_high_kill(),

            // Quantize commands
            AudioCommand::ToggleQuantizeA => self.deck_a.toggle_quantize(),
            AudioCommand::ToggleQuantizeB => self.deck_b.toggle_quantize(),
            AudioCommand::CycleQuantizeResolutionA => self.deck_a.cycle_quantize_resolution(),
            AudioCommand::CycleQuantizeResolutionB => self.deck_b.cycle_quantize_resolution(),

            // Key Lock commands
            AudioCommand::ToggleKeyLockA => self.deck_a.toggle_key_lock(),
            AudioCommand::ToggleKeyLockB => self.deck_b.toggle_key_lock(),

            // Slip Mode commands
            AudioCommand::ToggleSlipA => self.deck_a.toggle_slip(),
            AudioCommand::ToggleSlipB => self.deck_b.toggle_slip(),

            // Sampler commands
            AudioCommand::LoadSamplerSlot(idx, samples, sr, name) => {
                self.sampler.load_slot(idx, samples, sr, name);
            }
            AudioCommand::ClearSamplerSlot(idx) => self.sampler.clear_slot(idx),
            AudioCommand::TriggerSampler(idx) => self.sampler.trigger(idx),
            AudioCommand::StopSampler(idx) => self.sampler.stop(idx),
            AudioCommand::SetSamplerGain(idx, gain) => self.sampler.set_gain(idx, gain),
            AudioCommand::SetSamplerLoop(idx, enabled) => self.sampler.set_loop(idx, enabled),

            // Recording commands
            AudioCommand::StartRecording => self.recording.start(),
            AudioCommand::StopRecording => self.recording.stop(),

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
            // Mastering state
            mastering_enabled: self.mastering.is_enabled(),
            mastering_preset: self.mastering.preset(),
            mastering_lufs: self.mastering.lufs(),
            mastering_gain_reduction: self.mastering.gain_reduction_db(),
            // New effects state
            flanger_a_enabled: self.flanger_a.is_enabled(),
            flanger_b_enabled: self.flanger_b.is_enabled(),
            bitcrusher_a_enabled: self.bitcrusher_a.is_enabled(),
            bitcrusher_b_enabled: self.bitcrusher_b.is_enabled(),
            tape_stop_a_enabled: self.tape_stop_a.is_enabled(),
            tape_stop_b_enabled: self.tape_stop_b.is_enabled(),
            phaser_a_enabled: self.phaser_a.is_enabled(),
            phaser_b_enabled: self.phaser_b.is_enabled(),
            gate_a_enabled: self.gate_a.is_enabled(),
            gate_b_enabled: self.gate_b.is_enabled(),
            beat_repeat_a_enabled: self.beat_repeat_a.is_enabled(),
            beat_repeat_b_enabled: self.beat_repeat_b.is_enabled(),
            ringmod_a_enabled: self.ringmod_a.is_enabled(),
            ringmod_b_enabled: self.ringmod_b.is_enabled(),
            shimmer_a_enabled: self.shimmer_a.is_enabled(),
            shimmer_b_enabled: self.shimmer_b.is_enabled(),
            washout_a_enabled: self.washout_a.is_enabled(),
            washout_b_enabled: self.washout_b.is_enabled(),
            washout_a_amount: self.washout_a.wash(),
            washout_b_amount: self.washout_b.wash(),
            delay_a_mode: self.delay_a.mode(),
            delay_b_mode: self.delay_b.mode(),
            // Effect mix levels
            flanger_a_mix: self.flanger_a.mix(),
            flanger_b_mix: self.flanger_b.mix(),
            phaser_a_mix: self.phaser_a.mix(),
            phaser_b_mix: self.phaser_b.mix(),
            bitcrusher_a_mix: self.bitcrusher_a.mix(),
            bitcrusher_b_mix: self.bitcrusher_b.mix(),
            gate_a_mix: self.gate_a.mix(),
            gate_b_mix: self.gate_b.mix(),
            beat_repeat_a_mix: self.beat_repeat_a.mix(),
            beat_repeat_b_mix: self.beat_repeat_b.mix(),
            ringmod_a_mix: self.ringmod_a.mix(),
            ringmod_b_mix: self.ringmod_b.mix(),
            shimmer_a_mix: self.shimmer_a.mix(),
            shimmer_b_mix: self.shimmer_b.mix(),
            delay_a_mix: self.delay_a.mix(),
            delay_b_mix: self.delay_b.mix(),
            reverb_a_mix: self.reverb_a.wet(),
            reverb_b_mix: self.reverb_b.wet(),
            // Channel EQ state
            eq_a_low: self.eq_a.low_gain(),
            eq_a_mid: self.eq_a.mid_gain(),
            eq_a_high: self.eq_a.high_gain(),
            eq_a_low_kill: self.eq_a.low_kill(),
            eq_a_mid_kill: self.eq_a.mid_kill(),
            eq_a_high_kill: self.eq_a.high_kill(),
            eq_b_low: self.eq_b.low_gain(),
            eq_b_mid: self.eq_b.mid_gain(),
            eq_b_high: self.eq_b.high_gain(),
            eq_b_low_kill: self.eq_b.low_kill(),
            eq_b_mid_kill: self.eq_b.mid_kill(),
            eq_b_high_kill: self.eq_b.high_kill(),
            // Sampler state
            sampler_slots: self.sampler.slot_states(),
            // Recording state
            is_recording: self.recording.is_recording,
            recording_duration: self.recording.duration_secs(),
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
        let (source_grid, source_phase) = match (self.deck_a.beat_grid(), self.deck_a.beat_phase())
        {
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
        let phase_offset = self
            .deck_b
            .phase_offset_to_align(source_phase)
            .unwrap_or(0.0);

        // Step 3: Start smooth transition (~500ms at 44.1kHz)
        let transition_duration = (self.sample_rate as f64 * 0.5) as u64;
        self.deck_b
            .start_sync_transition(new_tempo, phase_offset, transition_duration);
    }

    /// Smart sync: sync Deck A's tempo and phase to Deck B
    fn smart_sync_a_to_b(&mut self) {
        // Get beat grids from both decks
        let (source_grid, source_phase) = match (self.deck_b.beat_grid(), self.deck_b.beat_phase())
        {
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
        let phase_offset = self
            .deck_a
            .phase_offset_to_align(source_phase)
            .unwrap_or(0.0);

        // Start smooth transition
        let transition_duration = (self.sample_rate as f64 * 0.5) as u64;
        self.deck_a
            .start_sync_transition(new_tempo, phase_offset, transition_duration);
    }

    /// Fallback tempo-only sync (no phase alignment)
    fn tempo_only_sync_b_to_a(&mut self) {
        if let (Some(bpm_a), Some(_bpm_b)) = (self.deck_a.current_bpm(), self.deck_b.current_bpm())
        {
            if let Some(original_b) = self
                .deck_b
                .state()
                .bpm
                .map(|b| b / self.deck_b.state().tempo)
            {
                let new_tempo = bpm_a / original_b;
                self.deck_b.set_tempo(new_tempo);
            }
        }
    }

    /// Fallback tempo-only sync (no phase alignment)
    fn tempo_only_sync_a_to_b(&mut self) {
        if let (Some(_bpm_a), Some(bpm_b)) = (self.deck_a.current_bpm(), self.deck_b.current_bpm())
        {
            if let Some(original_a) = self
                .deck_a
                .state()
                .bpm
                .map(|b| b / self.deck_a.state().tempo)
            {
                let new_tempo = bpm_b / original_a;
                self.deck_a.set_tempo(new_tempo);
            }
        }
    }

    /// Process audio for output buffer
    pub fn process(&mut self, output: &mut [f32]) {
        // Clamp to pre-allocated buffer size to avoid allocations in audio callback
        let len = output.len().min(self.buffer_a.len());
        let output = &mut output[..len];

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

        // Feed BPM to beat-synced effects
        if let Some(bpm_a) = self.deck_a.current_bpm() {
            self.gate_a.set_bpm(bpm_a);
            self.beat_repeat_a.set_bpm(bpm_a);
        }
        if let Some(bpm_b) = self.deck_b.current_bpm() {
            self.gate_b.set_bpm(bpm_b);
            self.beat_repeat_b.set_bpm(bpm_b);
        }

        // Apply effects chain:
        // TapeStop → BeatRepeat → Vinyl → Bitcrusher → RingMod → Gate
        //   → Filter → Flanger → Phaser → Delay → Reverb → ShimmerReverb → WashOut

        // Deck A chain
        // TapeStop → BeatRepeat → Vinyl → Bitcrusher → RingMod → Gate
        //   → EQ → Filter → Flanger → Phaser → Delay → Reverb → Shimmer → WashOut
        self.tape_stop_a.process(buf_a);
        self.beat_repeat_a.process(buf_a);
        self.vinyl_a.process(buf_a);
        self.bitcrusher_a.process(buf_a);
        self.ringmod_a.process(buf_a);
        self.gate_a.process(buf_a);
        self.eq_a.process(buf_a);
        match self.filter_mode_a {
            FilterMode::Biquad => self.filter_a.process(buf_a),
            FilterMode::Ladder => self.ladder_a.process(buf_a),
            FilterMode::SVF => self.svf_a.process(buf_a),
        }
        self.flanger_a.process(buf_a);
        self.phaser_a.process(buf_a);
        self.delay_a.process(buf_a);
        self.reverb_a.process(buf_a);
        self.shimmer_a.process(buf_a);
        self.washout_a.process(buf_a);

        // Deck B chain
        self.tape_stop_b.process(buf_b);
        self.beat_repeat_b.process(buf_b);
        self.vinyl_b.process(buf_b);
        self.bitcrusher_b.process(buf_b);
        self.ringmod_b.process(buf_b);
        self.gate_b.process(buf_b);
        self.eq_b.process(buf_b);
        match self.filter_mode_b {
            FilterMode::Biquad => self.filter_b.process(buf_b),
            FilterMode::Ladder => self.ladder_b.process(buf_b),
            FilterMode::SVF => self.svf_b.process(buf_b),
        }
        self.flanger_b.process(buf_b);
        self.phaser_b.process(buf_b);
        self.delay_b.process(buf_b);
        self.reverb_b.process(buf_b);
        self.shimmer_b.process(buf_b);
        self.washout_b.process(buf_b);

        // Mix to output
        self.mixer.mix(buf_a, buf_b, output);

        // Mix sampler slots into master output
        self.sampler.process(output);

        // Mastering chain - EQ, compression, saturation, stereo enhancement
        // Applied before the limiter for transparent processing
        self.mastering.process(output);

        // Master limiter - brickwall limiting to prevent clipping
        self.master_limiter.process(output);

        // Final safety hard clip at limiter ceiling (-1.0 dBFS = 0.891)
        // This should never trigger if the limiter is working correctly
        const CEILING: f32 = 0.891;
        for sample in output.iter_mut() {
            *sample = sample.clamp(-CEILING, CEILING);
        }

        // Recording tap (after mastering + limiter for loudness-normalized output)
        if self.recording.is_recording {
            self.recording.add_samples(output);
        }
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
    pub fn create_channels() -> (
        Sender<AudioCommand>,
        Receiver<AudioCommand>,
        Sender<AudioEvent>,
        Receiver<AudioEvent>,
    ) {
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
