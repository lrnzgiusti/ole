//! Terminal UI for OLE - widgets, themes, and layout
//!
//! Provides vintage CRT-style terminal interface for DJing.

mod app;
mod theme;
pub mod widgets;

pub use app::{App, AppState, CrtEffects, CrtIntensity, FocusedPane, WaveformZoom, SPECTRUM_BANDS};
pub use theme::{Theme, CRT_AMBER, CRT_GREEN, CYBERPUNK};
pub use widgets::status_bar::HelpWidget;
pub use widgets::{
    CamelotWheelWidget, CrossfaderWidget, DeckWidget, EnhancedWaveformWidget,
    HarmonicCompatibility, LibraryState, LibraryWidget, PhaseWidget, ScopeMode, ScopeWidget,
    SpectrumWidget, StatusBarWidget,
};
