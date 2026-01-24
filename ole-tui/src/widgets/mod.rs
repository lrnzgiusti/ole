//! UI Widgets for OLE

mod camelot;
mod crossfader;
mod deck;
mod library;
mod phase;
mod scope;
mod spectrum;
pub mod status_bar;
mod vu_meter;
mod waveform;

pub use camelot::{CamelotWheelWidget, HarmonicCompatibility};
pub use crossfader::CrossfaderWidget;
pub use deck::DeckWidget;
pub use library::{LibraryState, LibraryWidget};
pub use phase::PhaseWidget;
pub use scope::{ScopeMode, ScopeWidget};
pub use spectrum::SpectrumWidget;
pub use status_bar::StatusBarWidget;
pub use vu_meter::MasterVuMeterWidget;
pub use waveform::EnhancedWaveformWidget;
