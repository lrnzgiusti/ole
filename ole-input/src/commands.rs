//! Command definitions for OLE

use std::path::PathBuf;

// Re-export types for use in commands
pub use ole_audio::{DelayModulation, FilterMode, FilterType};

/// Deck identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckId {
    A,
    B,
}

/// Navigation direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// Effect type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectType {
    Filter,
    Delay,
    Reverb,
}

/// Vinyl preset (1-5)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VinylPresetId {
    /// Subtle - minimal coloration
    Subtle = 1,
    /// Warm - gentle warmth
    Warm = 2,
    /// Classic - traditional vinyl character
    Classic = 3,
    /// Aged - old record sound
    Aged = 4,
    /// LoFi - heavy degradation
    LoFi = 5,
}

/// Commands that can be dispatched from input
#[derive(Debug, Clone)]
pub enum Command {
    // Playback
    Play(DeckId),
    Pause(DeckId),
    Stop(DeckId),
    Toggle(DeckId),

    // Seeking
    Seek(DeckId, f64),
    Nudge(DeckId, f64),
    Beatjump(DeckId, i32), // Jump by N beats (negative = backward)

    // Cue points
    SetCue(DeckId, u8),  // Set cue point 1-4
    JumpCue(DeckId, u8), // Jump to cue point 1-4

    // Tempo
    SetTempo(DeckId, f32),
    AdjustTempo(DeckId, f32),

    // Gain
    SetGain(DeckId, f32),
    AdjustGain(DeckId, f32),

    // Sync
    Sync(DeckId),

    // Crossfader
    SetCrossfader(f32),
    MoveCrossfader(Direction),
    CenterCrossfader,

    // Effects (toggle/adjust)
    ToggleEffect(DeckId, EffectType),
    AdjustFilterCutoff(DeckId, f32),

    // Effect presets (level-based)
    SetDelayLevel(DeckId, u8),               // level 1-5
    SetFilterPreset(DeckId, FilterType, u8), // type + level 1-10
    SetReverbLevel(DeckId, u8),              // level 1-5

    // Filter mode selection (Biquad, Ladder, SVF)
    SetFilterMode(DeckId, FilterMode),
    CycleFilterMode(DeckId),

    // Vinyl emulation
    ToggleVinyl(DeckId),
    SetVinylPreset(DeckId, VinylPresetId),
    CycleVinylPreset(DeckId),
    SetVinylWow(DeckId, f32),    // 0.0-1.0
    SetVinylNoise(DeckId, f32),  // 0.0-1.0
    SetVinylWarmth(DeckId, f32), // 0.0-1.0

    // Time stretching (pitch-independent tempo)
    ToggleTimeStretch(DeckId),
    SetTimeStretchRatio(DeckId, f32), // 0.25-4.0

    // Delay modulation (tape character)
    SetDelayModulation(DeckId, DelayModulation),
    CycleDelayModulation(DeckId),

    // Track loading
    LoadTrack(DeckId, PathBuf),

    // UI
    ToggleHelp,
    ToggleScope,     // Toggle between spectrum and oscilloscope view
    CycleScopeMode,  // Cycle oscilloscope mode (time domain, lissajous)
    ZoomIn(DeckId),  // Zoom in on waveform
    ZoomOut(DeckId), // Zoom out on waveform
    SetTheme(String),
    CycleFocus,
    Focus(DeckId),

    // Mode changes
    EnterCommandMode,
    EnterEffectsMode,
    EnterNormalMode,
    EnterBrowserMode,

    // Library/Browser
    LibraryScan(PathBuf),
    LibrarySelectNext,
    LibrarySelectPrev,
    LibrarySelectFirst,
    LibrarySelectLast,
    LibraryLoadToDeck(DeckId),
    LibraryFilterByKey(String),
    LibraryClearFilter,
    LibraryToggle,

    // Application
    Quit,
    Cancel,

    // Command mode
    ExecuteCommand(String),

    // CRT screen effects
    ToggleCrt,         // Master CRT effects toggle
    ToggleGlow,        // Phosphor glow effect
    ToggleNoise,       // Static noise effect
    ToggleChromatic,   // RGB chromatic aberration
    CycleCrtIntensity, // Cycle through Off/Subtle/Medium/Heavy
}
