//! CRT-style themes for OLE

use ratatui::style::{Color, Modifier, Style};

/// Theme configuration for the UI
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: &'static str,
    /// Primary foreground color (text, borders)
    pub fg: Color,
    /// Dimmed foreground (secondary text)
    pub fg_dim: Color,
    /// Background color
    pub bg: Color,
    /// Highlight color (selected items, active elements)
    pub highlight: Color,
    /// Accent color (meters, spectrum peaks)
    pub accent: Color,
    /// Warning color
    pub warning: Color,
    /// Error/danger color
    pub danger: Color,
    /// Deck A color
    pub deck_a: Color,
    /// Deck B color
    pub deck_b: Color,
}

impl Theme {
    /// Get style for normal text
    pub fn normal(&self) -> Style {
        Style::default().fg(self.fg).bg(self.bg)
    }

    /// Get style for dimmed text
    pub fn dim(&self) -> Style {
        Style::default().fg(self.fg_dim).bg(self.bg)
    }

    /// Get style for highlighted/selected items
    pub fn highlight(&self) -> Style {
        Style::default()
            .fg(self.bg)
            .bg(self.highlight)
            .add_modifier(Modifier::BOLD)
    }

    /// Get style for borders
    pub fn border(&self) -> Style {
        Style::default().fg(self.fg_dim)
    }

    /// Get style for active borders
    pub fn border_active(&self) -> Style {
        Style::default().fg(self.highlight)
    }

    /// Get style for deck A
    pub fn deck_a_style(&self) -> Style {
        Style::default().fg(self.deck_a)
    }

    /// Get style for deck B
    pub fn deck_b_style(&self) -> Style {
        Style::default().fg(self.deck_b)
    }

    /// Get style for meters/bars based on level (0.0 - 1.0)
    pub fn meter_style(&self, level: f32) -> Style {
        let color = if level > 0.9 {
            self.danger
        } else if level > 0.75 {
            self.warning
        } else {
            self.accent
        };
        Style::default().fg(color)
    }

    /// Get title style
    pub fn title(&self) -> Style {
        Style::default()
            .fg(self.highlight)
            .add_modifier(Modifier::BOLD)
    }

    /// Get style for spectrum bars based on frequency band
    pub fn spectrum_style(&self, band: usize, total_bands: usize) -> Style {
        // Color gradient: bass (warm) -> treble (cool)
        let ratio = band as f32 / total_bands as f32;
        let color = if ratio < 0.33 {
            self.deck_a // Bass - warm
        } else if ratio < 0.66 {
            self.accent // Mid
        } else {
            self.deck_b // Treble - cool
        };
        Style::default().fg(color)
    }

    /// Get style for waveform based on playhead position
    pub fn waveform_style(&self, is_future: bool) -> Style {
        if is_future {
            Style::default().fg(self.fg_dim)
        } else {
            Style::default().fg(self.accent)
        }
    }

    /// Get style for effect when enabled
    pub fn fx_enabled(&self) -> Style {
        Style::default()
            .fg(self.bg)
            .bg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    /// Get style for effect when disabled
    pub fn fx_disabled(&self) -> Style {
        Style::default().fg(self.fg_dim)
    }
}

/// Classic phosphor green CRT theme
pub const CRT_GREEN: Theme = Theme {
    name: "phosphor-green",
    fg: Color::Rgb(51, 255, 51),        // #33ff33 - phosphor green
    fg_dim: Color::Rgb(25, 128, 25),    // dimmed green
    bg: Color::Rgb(0, 10, 0),           // near black with green tint
    highlight: Color::Rgb(180, 255, 180), // bright green
    accent: Color::Rgb(100, 255, 100),  // medium green
    warning: Color::Rgb(255, 255, 100), // yellow-green
    danger: Color::Rgb(255, 100, 100),  // red warning
    deck_a: Color::Rgb(100, 255, 150),  // green-cyan
    deck_b: Color::Rgb(150, 255, 100),  // yellow-green
};

/// Amber CRT theme (1980s monochrome)
pub const CRT_AMBER: Theme = Theme {
    name: "amber",
    fg: Color::Rgb(255, 176, 0),        // #ffb000 - amber
    fg_dim: Color::Rgb(128, 88, 0),     // dimmed amber
    bg: Color::Rgb(10, 5, 0),           // near black with amber tint
    highlight: Color::Rgb(255, 220, 128), // bright amber
    accent: Color::Rgb(255, 200, 64),   // medium amber
    warning: Color::Rgb(255, 255, 100), // yellow
    danger: Color::Rgb(255, 100, 100),  // red warning
    deck_a: Color::Rgb(255, 180, 50),   // orange-amber
    deck_b: Color::Rgb(255, 220, 100),  // yellow-amber
};

/// Cyberpunk neon theme
pub const CYBERPUNK: Theme = Theme {
    name: "cyberpunk",
    fg: Color::Rgb(0, 255, 255),        // cyan
    fg_dim: Color::Rgb(0, 128, 128),    // dim cyan
    bg: Color::Rgb(5, 0, 10),           // dark purple-black
    highlight: Color::Rgb(255, 0, 255), // magenta
    accent: Color::Rgb(0, 255, 128),    // neon green
    warning: Color::Rgb(255, 255, 0),   // yellow
    danger: Color::Rgb(255, 50, 50),    // red
    deck_a: Color::Rgb(255, 100, 255),  // pink
    deck_b: Color::Rgb(100, 255, 255),  // light cyan
};

impl Default for Theme {
    fn default() -> Self {
        CRT_GREEN
    }
}
