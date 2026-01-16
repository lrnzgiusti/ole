//! Audio effects for OLE

mod delay;
mod filter;
mod ladder_filter;
mod reverb;
mod svf;

pub use delay::{Delay, DelayInterpolation, DelayModulation};
pub use filter::{Filter, FilterType};
pub use ladder_filter::LadderFilter;
pub use reverb::Reverb;
pub use svf::{StateVariableFilter, SvfOutputType};

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
