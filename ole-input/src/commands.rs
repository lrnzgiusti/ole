//! Command definitions for OLE

use std::path::PathBuf;

// Re-export types for use in commands
pub use ole_audio::{
    DelayMode, DelayModulation, EffectType, FilterMode, FilterType, GateDivision, MasteringPreset,
};

/// Input modes (vim-style)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Command,
    Effects,
    Help,
    Browser,
}

impl Mode {
    pub fn display_name(&self) -> &'static str {
        match self {
            Mode::Normal => "NORMAL",
            Mode::Command => "COMMAND",
            Mode::Effects => "EFFECTS",
            Mode::Help => "HELP",
            Mode::Browser => "BROWSE",
        }
    }
}

/// Deck identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckId {
    A,
    B,
}

/// Energy direction preference for copilot scoring
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnergyDirection {
    #[default]
    Maintain,
    Build,
    Drop,
}

impl EnergyDirection {
    pub fn next(self) -> Self {
        match self {
            Self::Maintain => Self::Build,
            Self::Build => Self::Drop,
            Self::Drop => Self::Maintain,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Maintain => "MAINTAIN",
            Self::Build => "BUILD",
            Self::Drop => "DROP",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Self::Maintain => "=",
            Self::Build => "\u{25b2}",  // ▲
            Self::Drop => "\u{25bc}",   // ▼
        }
    }
}

/// Navigation direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
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
    BeatNudge(DeckId, f32), // Nudge by fraction of beat (e.g., 0.0625 = 1/16 beat)
    Beatjump(DeckId, i32),  // Jump by N beats (negative = backward)

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
    AdjustEffectMix(DeckId, EffectType, f32), // delta: +0.1 or -0.1
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
    LibraryRescan, // Force rescan with massive parallelism
    LibrarySelectNext,
    LibrarySelectPrev,
    LibrarySelectFirst,
    LibrarySelectLast,
    LibraryLoadToDeck(DeckId),
    LibraryFilterByKey(String),
    LibraryFilterByBpmRange(u16, u16), // Filter by BPM range (min, max)
    LibraryFilterCompatible,           // Filter to harmonically compatible keys
    LibraryClearFilter,
    LibraryToggle,
    LibraryJumpToKey(u8, bool), // Jump to key (1-12 Camelot position, true=A/false=B)
    LibraryJumpToBpm(u16),      // Jump to first track near this BPM

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

    // Mastering chain
    ToggleMastering,                     // Toggle mastering on/off
    SetMasteringPreset(MasteringPreset), // Set mastering preset
    CycleMasteringPreset,                // Cycle through presets

    // Tape Stop effect
    ToggleTapeStop(DeckId),
    TriggerTapeStop(DeckId),  // Start the stop effect
    TriggerTapeStart(DeckId), // Spin back up

    // Flanger effect
    ToggleFlanger(DeckId),

    // Bitcrusher effect
    ToggleBitcrusher(DeckId),

    // Phaser effect
    TogglePhaser(DeckId),

    // Gate effect
    ToggleGate(DeckId),

    // Beat Repeat effect
    ToggleBeatRepeat(DeckId),
    TriggerBeatRepeat(DeckId),

    // Ring Modulator effect
    ToggleRingMod(DeckId),

    // Shimmer Reverb effect
    ToggleShimmer(DeckId),

    // Wash Out effect
    ToggleWashOut(DeckId),
    SetWashAmount(DeckId, f32),

    // Delay mode (Stereo/PingPong/Mono)
    CycleDelayMode(DeckId),

    // Help navigation
    HelpScrollUp,
    HelpScrollDown,

    // Looping
    SetLoopIn(DeckId),
    SetLoopOut(DeckId),
    ToggleLoop(DeckId),
    ClearLoop(DeckId),
    AutoLoop(DeckId, f32),        // beats: 0.25, 0.5, 1, 2, 4, 8, 16
    LoopHalve(DeckId),
    LoopDouble(DeckId),
    LoopRollStart(DeckId, f32),   // beats
    LoopRollEnd(DeckId),

    // 3-Band EQ per channel
    AdjustEqLow(DeckId, f32),    // delta dB
    AdjustEqMid(DeckId, f32),
    AdjustEqHigh(DeckId, f32),
    KillEqLow(DeckId),           // toggle kill switch
    KillEqMid(DeckId),
    KillEqHigh(DeckId),

    // Quantize
    ToggleQuantize(DeckId),
    CycleQuantizeResolution(DeckId),

    // Key Lock
    ToggleKeyLock(DeckId),

    // Slip Mode
    ToggleSlip(DeckId),

    // Sampler (Phase 5)
    TriggerSampler(u8),        // Trigger slot 0-7 (0-indexed)
    StopSampler(u8),           // Stop slot 0-7 (0-indexed)
    LoadSamplerSlot(u8, PathBuf), // Load sample to slot
    ClearSamplerSlot(u8),
    ToggleSamplerLoop(u8),

    // Recording (Phase 5)
    ToggleRecording,           // Start/stop recording
    SaveRecording(PathBuf),    // Save recording to file

    // Effect Macros (Phase 5C)
    TriggerMacro(DeckId, u8),  // Activate macro 1-4

    // DJ Copilot
    ToggleCopilot,                     // Toggle copilot scoring mode
    CycleEnergyDirection,              // Cycle energy direction: Maintain → Build → Drop

    // Library enhancements (Phase 4)
    LibrarySearch(String),         // Set search query
    LibrarySearchAppend(char),     // Append char to search (interactive typing)
    LibrarySearchBackspace,        // Delete last search char
    LibrarySearchClear,            // Clear search query
    LibraryCycleSort,              // Cycle sort column
    LibraryReverseSort,            // Toggle sort direction
    LibraryShowHistory,            // Toggle history view
    LibraryPageDown,               // Jump down ~10 tracks
    LibraryPageUp,                 // Jump up ~10 tracks
}
