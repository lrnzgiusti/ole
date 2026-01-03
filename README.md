```
 ▄██████▄   ▄█          ▄████████
███    ███ ███         ███    ███
███    ███ ███         ███    █▀
███    ███ ███        ▄███▄▄▄
███    ███ ███       ▀▀███▀▀▀
███    ███ ███         ███    █▄
███    ███ ███▌    ▄   ███    ███
 ▀██████▀  █████▄▄██   ██████████
           ▀
```

# OLE - Open Live Engine

> *"In the beginning was the command line. And the command line said: DROP."*

A terminal-based DJ application with vintage 1970s-80s CRT aesthetic and vim-style keyboard controls. Think **Mixxx meets Vim** — powerful, keyboard-driven, and brutally efficient.

## Features

### Audio Engine ✅
- **Dual Decks** - Load and mix two tracks simultaneously
- **Beat Sync** - BPM detection with phase-aligned tempo synchronization
- **Effects** - Filter (LP/HP/BP), Delay, Reverb with preset levels
- **Crossfader** - Smooth mixing with multiple curve options
- **Format Support** - MP3, FLAC, WAV, OGG, AAC

### Terminal UI ✅
- **CRT Aesthetic** - Phosphor green, amber, and cyberpunk themes
- **Spectrum Analyzer** - Real-time 32-band FFT visualization
- **Waveform Display** - Track progress with cue point markers
- **Modal Interface** - Vim-style keyboard navigation

### Keyboard-Driven ✅
- **No Mouse Required** - Every function accessible via keyboard
- **Modal Editing** - Normal, Command, Effects, and Help modes
- **Composable Commands** - Chain operations efficiently

### Coming Soon
- 🔜 Waveform zoom/scroll
- 🔜 Looping system
- 🔜 More effects (flanger, phaser, compressor)
- 🔜 File browser
- 🔜 AI Digital Twin

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/yourusername/OLE.git
cd OLE

# Build release binary
cargo build --release

# Run
./target/release/ole
```

### Requirements
- Rust 1.75+ (MSRV)
- macOS (CoreAudio), Linux (ALSA/PulseAudio), or Windows (WASAPI)

## Quick Start

```
┌─────────────────────────────────────────────────────────────────┐
│  1. Launch OLE                                                  │
│     $ ./target/release/ole                                      │
│                                                                 │
│  2. Load tracks                                                 │
│     :load a '/path/to/track1.mp3'                              │
│     :load b '/path/to/track2.mp3'                              │
│                                                                 │
│  3. Play & Mix                                                  │
│     Space - Toggle play Deck A                                  │
│     Shift+Space - Toggle play Deck B                            │
│     -/= - Move crossfader left/right                           │
│     y - Sync Deck A tempo to Deck B                            │
│                                                                 │
│  4. Add Effects (press 'e' for effects mode)                   │
│     fl5 - Low-pass filter level 5                              │
│     d3  - Delay level 3                                        │
│     r2  - Reverb level 2                                       │
│                                                                 │
│  5. Press ? for full help, :q to quit                          │
└─────────────────────────────────────────────────────────────────┘
```

## Keyboard Controls

### Normal Mode (default)

| Key | Action |
|-----|--------|
| `Space` / `Shift+Space` | Toggle play Deck A / B |
| `s` / `S` | Stop Deck A / B |
| `h` / `l` | Nudge Deck A back / forward |
| `j` / `k` | Beat jump Deck A back / forward |
| `-` / `=` | Crossfader left / right |
| `0` | Center crossfader |
| `y` / `Y` | Sync A→B / B→A |
| `[` / `]` | Tempo -/+ 1% (Deck A) |
| `{` / `}` | Tempo -/+ 5% (Deck A) |
| `1-4` | Set cue point 1-4 (Deck A) |
| `!@#$` | Jump to cue point 1-4 (Deck A) |
| `Tab` | Cycle focus |
| `:` | Command mode |
| `e` | Effects mode |
| `?` | Show help |
| `Ctrl+C` | Quit |

### Command Mode (`:`)

```
:load a <path>    Load track to Deck A
:load b <path>    Load track to Deck B
:theme <name>     Switch theme (green/amber/cyberpunk)
:q                Quit OLE
:help             Show help
```

### Effects Mode (`e`)

Effect sequences: `<effect><level>` where level is 0-5 (0 = off)

| Sequence | Action |
|----------|--------|
| `d3` | Delay level 3 |
| `r2` | Reverb level 2 |
| `fl5` | Low-pass filter level 5 |
| `fh7` | High-pass filter level 7 |
| `fb4` | Band-pass filter level 4 |
| `f0` | Filter off |
| `a` / `b` | Switch to deck A / B |
| `Esc` | Return to Normal mode |

## Themes

Switch themes with `:theme <name>`

| Theme | Description |
|-------|-------------|
| `green` | Classic phosphor green CRT (default) |
| `amber` | 1980s amber monochrome |
| `cyberpunk` | Neon cyberpunk |

## Architecture

```
ole/
├── ole-app/        # Main binary - event loop, rendering
├── ole-audio/      # Audio engine - decks, mixer, effects
├── ole-analysis/   # DSP - spectrum FFT, BPM detection
├── ole-tui/        # Terminal UI - widgets, themes
├── ole-input/      # Keyboard handling - modal state machine
└── ole-library/    # Track loading - decoder, resampler
```


## Tech Stack

- **Language**: Rust (MSRV 1.75)
- **Audio**: cpal + symphonia + rubato
- **TUI**: ratatui + crossterm
- **DSP**: rustfft for spectrum analysis
- **Concurrency**: crossbeam-channel for thread communication

## Troubleshooting

### Audio Issues

**No sound output**
- Check audio device is connected and selected as default
- Try `RUST_LOG=ole_audio=debug cargo run` to see audio initialization

**Audio glitches/clicks**
- Reduce system load or close other audio applications
- Check for buffer underruns in debug log

### Terminal Issues

**Display corruption**
- Ensure terminal supports Unicode (UTF-8)
- Try a different terminal emulator (iTerm2, Alacritty, kitty)

**Key bindings not working**
- Check current mode in status bar
- Press `Esc` to return to Normal mode
- Press `?` for help overlay

### Track Loading Issues

**"Failed to load track"**
- Verify file exists and is readable
- Check file format is supported (MP3, FLAC, WAV, OGG, AAC)
- Use quotes for paths with spaces: `:load a '/path/to/my track.mp3'`

## Roadmap

See [TODO.md](TODO.md) for the full roadmap with complexity estimates.

**Phase 2** (Next):
- Waveform zoom and scroll
- Looping system
- More cue points

**Phase 3**:
- Key detection
- More effects (flanger, phaser, compressor)
- 3-band EQ
- Recording

**Phase 5**:
- AI Digital Twin (see [AGENTS.md](AGENTS.md))

## Contributing

See [DEV.md](DEV.md) for development setup and guidelines.

Quick start:
```bash
cargo build
cargo test
cargo clippy
RUST_LOG=debug cargo run
```

## License

MIT License - See [LICENSE](LICENSE) for details.

---

<p align="center">
  <i>"Mix like you're hacking the mainframe"</i>
</p>
