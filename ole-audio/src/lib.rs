//! Audio engine for OLE - decks, mixer, and effects
//!
//! This module provides the core audio processing pipeline:
//! - Deck: Track playback with pitch/tempo control
//! - Mixer: Crossfader and channel routing
//! - Effects: Filter, delay, and other DSP effects
//! - Vinyl: Turntable emulation (motor, wow/flutter, warmth, noise)
//! - Timestretcher: Phase vocoder for pitch-independent tempo

mod deck;
mod mixer;
mod effects;
mod engine;
mod vinyl;
pub mod timestretcher;

pub use deck::{Deck, DeckState, PlaybackState, BeatGridInfo, SyncTransition, SCOPE_SAMPLES_SIZE};
pub use mixer::{Mixer, CrossfaderCurve};
pub use effects::{Effect, Filter, FilterType, FilterMode, LadderFilter, StateVariableFilter, SvfOutputType, Delay, DelayInterpolation, DelayModulation, Reverb};
pub use engine::{AudioEngine, AudioCommand, AudioEvent, EngineState};
pub use vinyl::{VinylEmulator, VinylPreset, TurntableMotor, WowFlutter, AnalogWarmth, SaturationType, VinylNoise};
pub use timestretcher::{PhaseVocoder, PhaseLockMode, FftSize, TimeStretchParams};
