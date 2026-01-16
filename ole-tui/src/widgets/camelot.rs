//! Camelot Wheel widget for harmonic mixing visualization
//!
//! Displays a linear representation of the Camelot wheel with:
//! - Current deck keys highlighted
//! - Compatible keys shown
//! - Harmonic compatibility indicator

use crate::theme::Theme;
use ole_analysis::CamelotKey;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

/// Harmonic compatibility level between two keys
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarmonicCompatibility {
    /// Same key (distance 0)
    Perfect,
    /// Adjacent or relative major/minor (distance 1)
    Harmonic,
    /// Two steps away (distance 2)
    Close,
    /// Three or more steps (distance 3+)
    Clash,
    /// One or both keys unknown
    Unknown,
}

impl HarmonicCompatibility {
    /// Calculate compatibility from two Camelot keys
    pub fn from_keys(key_a: Option<&str>, key_b: Option<&str>) -> Self {
        let (Some(a), Some(b)) = (key_a, key_b) else {
            return Self::Unknown;
        };

        let (Some(camelot_a), Some(camelot_b)) = (CamelotKey::parse(a), CamelotKey::parse(b)) else {
            return Self::Unknown;
        };

        match camelot_a.wheel_distance(&camelot_b) {
            0 => Self::Perfect,
            1 => Self::Harmonic,
            2 => Self::Close,
            _ => Self::Clash,
        }
    }

    /// Get display symbol for compatibility
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Perfect => "●",
            Self::Harmonic => "◉",
            Self::Close => "○",
            Self::Clash => "✕",
            Self::Unknown => "-",
        }
    }

    /// Get display label for compatibility
    pub fn label(&self) -> &'static str {
        match self {
            Self::Perfect => "PERFECT",
            Self::Harmonic => "HARMONIC",
            Self::Close => "CLOSE",
            Self::Clash => "CLASH",
            Self::Unknown => "---",
        }
    }
}

/// Widget for displaying Camelot wheel and harmonic compatibility
pub struct CamelotWheelWidget<'a> {
    theme: &'a Theme,
    /// Key for deck A (Camelot notation, e.g., "8A")
    key_a: Option<&'a str>,
    /// Key for deck B (Camelot notation, e.g., "12B")
    key_b: Option<&'a str>,
}

impl<'a> CamelotWheelWidget<'a> {
    /// Create a new Camelot wheel widget
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            key_a: None,
            key_b: None,
        }
    }

    /// Set the key for deck A
    pub fn key_a(mut self, key: Option<&'a str>) -> Self {
        self.key_a = key;
        self
    }

    /// Set the key for deck B
    pub fn key_b(mut self, key: Option<&'a str>) -> Self {
        self.key_b = key;
        self
    }

    /// Get style for a key on the wheel
    fn key_style(&self, key_num: u8, is_major: bool) -> Style {
        let key = CamelotKey::new(key_num, is_major).unwrap();
        let key_str = key.display();

        // Check if this is deck A's key
        let is_deck_a = self.key_a.map(|k| k == key_str).unwrap_or(false);
        // Check if this is deck B's key
        let is_deck_b = self.key_b.map(|k| k == key_str).unwrap_or(false);

        if is_deck_a && is_deck_b {
            // Both decks on same key - use highlight
            Style::default()
                .fg(self.theme.highlight)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else if is_deck_a {
            // Deck A's key
            Style::default()
                .fg(self.theme.deck_a)
                .add_modifier(Modifier::BOLD)
        } else if is_deck_b {
            // Deck B's key
            Style::default()
                .fg(self.theme.deck_b)
                .add_modifier(Modifier::BOLD)
        } else {
            // Check if compatible with either deck
            let compatible_a = self.key_a
                .and_then(CamelotKey::parse)
                .map(|k| k.is_compatible(&key))
                .unwrap_or(false);
            let compatible_b = self.key_b
                .and_then(CamelotKey::parse)
                .map(|k| k.is_compatible(&key))
                .unwrap_or(false);

            if compatible_a || compatible_b {
                // Compatible key - accent color
                Style::default().fg(self.theme.accent)
            } else {
                // Not compatible - dimmed
                self.theme.dim()
            }
        }
    }

    /// Render the wheel row (minor A or major B)
    fn render_wheel_row(&self, is_major: bool) -> Vec<Span<'a>> {
        let mut spans = Vec::new();

        for num in 1..=12u8 {
            let key = CamelotKey::new(num, is_major).unwrap();
            let style = self.key_style(num, is_major);

            // Format: " 1A" or "12B" (3 chars)
            let text = format!("{:>3}", key.display());
            spans.push(Span::styled(text, style));
        }

        spans
    }

    /// Get style for compatibility indicator
    fn compatibility_style(&self, compat: HarmonicCompatibility) -> Style {
        match compat {
            HarmonicCompatibility::Perfect => Style::default()
                .fg(self.theme.accent)
                .add_modifier(Modifier::BOLD),
            HarmonicCompatibility::Harmonic => Style::default()
                .fg(self.theme.accent),
            HarmonicCompatibility::Close => Style::default()
                .fg(self.theme.warning),
            HarmonicCompatibility::Clash => Style::default()
                .fg(self.theme.danger),
            HarmonicCompatibility::Unknown => self.theme.dim(),
        }
    }
}

impl Widget for CamelotWheelWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Need at least 40 chars wide and 5 lines tall
        if area.width < 40 || area.height < 4 {
            return;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border())
            .title(Span::styled(" CAMELOT ", self.theme.title()));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 3 || inner.width < 38 {
            return;
        }

        // Row 1: Minor keys (A row): 1A 2A 3A ... 12A
        let minor_spans = self.render_wheel_row(false);
        let minor_line = Line::from(minor_spans);
        let minor_area = Rect::new(inner.x, inner.y, inner.width, 1);
        Paragraph::new(minor_line).render(minor_area, buf);

        // Row 2: Major keys (B row): 1B 2B 3B ... 12B
        let major_spans = self.render_wheel_row(true);
        let major_line = Line::from(major_spans);
        let major_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
        Paragraph::new(major_line).render(major_area, buf);

        // Row 3: Compatibility indicator
        let compat = HarmonicCompatibility::from_keys(self.key_a, self.key_b);
        let compat_style = self.compatibility_style(compat);

        // Build indicator string: "● PERFECT (8A→8A Δ0)" or "◉ HARMONIC (8A→9A Δ1)"
        let mut indicator_spans = vec![
            Span::styled(format!(" {} ", compat.symbol()), compat_style),
            Span::styled(compat.label(), compat_style),
        ];

        // Add key info if both keys are known
        if let (Some(a), Some(b)) = (self.key_a, self.key_b) {
            if let (Some(ca), Some(cb)) = (CamelotKey::parse(a), CamelotKey::parse(b)) {
                let distance = ca.wheel_distance(&cb);
                indicator_spans.push(Span::styled(
                    format!(" ({}→{} Δ{})", a, b, distance),
                    self.theme.dim(),
                ));
            }
        }

        let indicator_line = Line::from(indicator_spans);
        let indicator_area = Rect::new(inner.x, inner.y + 2, inner.width, 1);
        Paragraph::new(indicator_line).render(indicator_area, buf);
    }
}
