//! Audio effects for OLE

mod beat_repeat;
mod bitcrusher;
mod delay;
mod eq;
mod filter;
mod flanger;
pub(crate) mod gate;
mod ladder_filter;
mod limiter;
mod reverb;
mod phaser;
mod ringmod;
mod shimmer;
mod svf;
mod tape_stop;
mod washout;

pub use beat_repeat::BeatRepeat;
pub use bitcrusher::Bitcrusher;
pub use delay::{Delay, DelayInterpolation, DelayMode, DelayModulation};
pub use eq::ChannelEq;
pub use filter::{Filter, FilterType};
pub use flanger::Flanger;
pub use gate::{Gate, GateDivision, GateShape};
pub use ladder_filter::LadderFilter;
pub use limiter::Limiter;
pub use phaser::Phaser;
pub use reverb::Reverb;
pub use ringmod::{RingModWaveform, RingModulator};
pub use shimmer::{ShimmerPitch, ShimmerReverb};
pub use svf::{StateVariableFilter, SvfOutputType};
pub use tape_stop::TapeStop;
pub use washout::{WashColor, WashOut};

/// Effect type identifier (for generic commands like mix adjustment)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectType {
    Filter,
    Delay,
    Reverb,
    TapeStop,
    Flanger,
    Bitcrusher,
    Phaser,
    Gate,
    BeatRepeat,
    RingMod,
    Shimmer,
    WashOut,
}

/// Filter mode - selects which filter implementation to use
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FilterMode {
    /// Original biquad filter (clean digital)
    #[default]
    Biquad,
    /// Moog-style 4-pole ladder (analog warmth)
    Ladder,
    /// State Variable Filter (clean, all outputs)
    SVF,
}

/// Trait for audio effects
pub trait Effect: Send {
    /// Process audio samples in place (stereo interleaved)
    fn process(&mut self, samples: &mut [f32]);

    /// Reset effect state
    fn reset(&mut self);

    /// Check if effect is enabled
    fn is_enabled(&self) -> bool;

    /// Enable/disable the effect
    fn set_enabled(&mut self, enabled: bool);

    /// Get effect name
    fn name(&self) -> &'static str;
}
