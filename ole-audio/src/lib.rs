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
pub mod timestretcher;
mod vinyl;

pub use deck::{BeatGridInfo, Deck, DeckState, PlaybackState, SyncTransition, SCOPE_SAMPLES_SIZE};
pub use effects::{
    Delay, DelayInterpolation, DelayModulation, Effect, Filter, FilterMode, FilterType,
    LadderFilter, Reverb, StateVariableFilter, SvfOutputType,
};
pub use engine::{AudioCommand, AudioEngine, AudioEvent, EngineState};
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
