//! Audio engine for OLE - decks, mixer, and effects
//!
//! This module provides the core audio processing pipeline:
//! - Deck: Track playback with pitch/tempo control
//! - Mixer: Crossfader and channel routing
//! - Effects: Filter, delay, and other DSP effects
//! - Vinyl: Turntable emulation (motor, wow/flutter, warmth, noise)
//! - Timestretcher: Phase vocoder for pitch-independent tempo

mod deck;
mod effects;
mod engine;
pub mod mastering;
mod mixer;
pub mod recording;
pub mod sampler;
pub mod timestretcher;
mod vinyl;

pub use deck::{BeatGridInfo, Deck, DeckState, LoopState, PlaybackState, QuantizeResolution, SyncTransition, SCOPE_SAMPLES_SIZE};
pub use effects::{
    BeatRepeat, Bitcrusher, ChannelEq, Delay, DelayInterpolation, DelayMode, DelayModulation,
    Effect, EffectType, Filter, FilterMode, FilterType, Flanger, Gate, GateDivision, GateShape,
    LadderFilter, Limiter, Phaser, Reverb, RingModWaveform, RingModulator, ShimmerPitch,
    ShimmerReverb, StateVariableFilter, SvfOutputType, TapeStop, WashColor, WashOut,
};
pub use engine::{AudioCommand, AudioEngine, AudioEvent, EngineState};
pub use recording::RecordingState;
pub use sampler::Sampler;
pub use mastering::{
    LoudnessMeter, LufsValues, MasteringChain, MasteringCompressor, MasteringEQ, MasteringPreset,
    MasteringSaturation, SaturationMode, StereoEnhancer,
};
pub use mixer::{CrossfaderCurve, Mixer};
pub use timestretcher::{FftSize, PhaseLockMode, PhaseVocoder, TimeStretchParams};
pub use vinyl::{
    AnalogWarmth, SaturationType, TurntableMotor, VinylEmulator, VinylNoise, VinylPreset,
    WowFlutter,
};
