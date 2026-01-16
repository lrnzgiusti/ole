//! UI Widgets for OLE

mod camelot;
mod crossfader;
mod deck;
mod library;
mod phase;
mod scope;
mod spectrum;
mod waveform;
pub mod status_bar;

pub use camelot::{CamelotWheelWidget, HarmonicCompatibility};
pub use crossfader::CrossfaderWidget;
pub use deck::DeckWidget;
pub use library::{LibraryState, LibraryWidget};
pub use phase::PhaseWidget;
pub use scope::{ScopeWidget, ScopeMode};
pub use spectrum::SpectrumWidget;
pub use status_bar::StatusBarWidget;
pub use waveform::EnhancedWaveformWidget;
