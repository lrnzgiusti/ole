//! Phase meter widget - shows beat alignment between decks

use crate::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::Span,
    widgets::{Block, Borders, Widget},
};

/// Widget for displaying beat phase alignment between two decks
///
/// Shows a horizontal track with a marker indicating the phase difference.
/// When decks are in sync, the marker is centered.
/// Also displays offset in milliseconds for precise alignment.
pub struct PhaseWidget<'a> {
    phase_a: f32, // Beat phase of deck A (0.0-1.0)
    phase_b: f32, // Beat phase of deck B (0.0-1.0)
    bpm_a: f32,   // BPM of deck A (for ms calculation)
    bpm_b: f32,   // BPM of deck B (for ms calculation)
    theme: &'a Theme,
    has_grid_a: bool, // Whether deck A has a beat grid
    has_grid_b: bool, // Whether deck B has a beat grid
}

impl<'a> PhaseWidget<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            phase_a: 0.0,
            phase_b: 0.0,
            bpm_a: 0.0,
            bpm_b: 0.0,
            theme,
            has_grid_a: false,
            has_grid_b: false,
        }
    }

    pub fn phases(mut self, phase_a: f32, phase_b: f32) -> Self {
        self.phase_a = phase_a;
        self.phase_b = phase_b;
        self
    }

    pub fn bpms(mut self, bpm_a: f32, bpm_b: f32) -> Self {
        self.bpm_a = bpm_a;
        self.bpm_b = bpm_b;
        self
    }

    pub fn has_grids(mut self, has_grid_a: bool, has_grid_b: bool) -> Self {
        self.has_grid_a = has_grid_a;
        self.has_grid_b = has_grid_b;
        self
    }

    /// Calculate offset in milliseconds (positive = B is ahead)
    fn offset_ms(&self) -> Option<f32> {
        let avg_bpm = (self.bpm_a + self.bpm_b) / 2.0;
        if avg_bpm < 1.0 {
            return None;
        }
        let ms_per_beat = 60000.0 / avg_bpm;
        let phase_diff = self.phase_difference();
        Some(phase_diff * ms_per_beat)
    }

    /// Calculate the phase difference between decks
    /// Returns a value from -0.5 to 0.5 where:
    /// - 0.0 means perfectly in sync
    /// - Negative means A is ahead
    /// - Positive means B is ahead
    fn phase_difference(&self) -> f32 {
        // Calculate the shortest angular distance between phases
        let diff = self.phase_b - self.phase_a;

        // Wrap to -0.5..0.5 range (since phase is circular)
        if diff > 0.5 {
            diff - 1.0
        } else if diff < -0.5 {
            diff + 1.0
        } else {
            diff
        }
    }
}

impl Widget for PhaseWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border())
            .title(Span::styled(" PHASE ", self.theme.title()));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width < 10 || inner.height < 1 {
            return;
        }

        let width = inner.width as usize;
        let y = inner.y + inner.height / 2;

        // If neither deck has a beat grid, show inactive state
        if !self.has_grid_a && !self.has_grid_b {
            let msg = "── no beat grid ──";
            let x_start = inner.x + (width.saturating_sub(msg.len())) as u16 / 2;
            for (i, ch) in msg.chars().enumerate() {
                let x = x_start + i as u16;
                if x < inner.x + inner.width {
                    buf[(x, y)].set_char(ch).set_style(self.theme.dim());
                }
            }
            return;
        }

        // Calculate phase difference and determine sync quality
        let phase_diff = self.phase_difference();
        let sync_quality = 1.0 - (phase_diff.abs() * 2.0).min(1.0); // 1.0 = perfect, 0.0 = 180° out

        // Determine style based on sync quality
        let (marker_style, status) = if sync_quality > 0.95 {
            // Nearly perfect sync (within ~2% of beat)
            (Style::from(self.theme.accent), "SYNC")
        } else if sync_quality > 0.8 {
            // Good sync (within ~10% of beat)
            (self.theme.highlight(), "")
        } else if sync_quality > 0.5 {
            // Moderate drift
            (Style::default().fg(self.theme.warning), "")
        } else {
            // Significant drift
            (Style::default().fg(self.theme.danger), "")
        };

        // Build the phase meter track
        // Layout: A ○━━━━━━━●━━━━━━━○ B
        let track_width = width.saturating_sub(6); // Leave room for "A " and " B"

        // Calculate marker position on the track
        // phase_diff of -0.5 to 0.5 maps to 0 to track_width-1
        let normalized = (phase_diff + 0.5).clamp(0.0, 1.0);
        let marker_pos = (normalized * (track_width.saturating_sub(1)) as f32) as usize;
        let center = track_width / 2;

        // Render "A " label
        let mut x = inner.x;
        buf[(x, y)]
            .set_char('A')
            .set_style(self.theme.deck_a_style());
        x += 1;
        buf[(x, y)].set_char(' ').set_style(self.theme.normal());
        x += 1;

        // Render the track with center anchor
        for i in 0..track_width {
            let ch = if i == marker_pos {
                '●' // Phase marker
            } else if i == center {
                '┼' // Center marker (sync point)
            } else if i == 0 || i == track_width - 1 {
                '○' // End anchors
            } else {
                '━' // Track
            };

            let style = if i == marker_pos {
                marker_style
            } else if i == center {
                self.theme.dim()
            } else {
                self.theme.normal()
            };

            buf[(x, y)].set_char(ch).set_style(style);
            x += 1;
        }

        // Render " B" label
        buf[(x, y)].set_char(' ').set_style(self.theme.normal());
        x += 1;
        buf[(x, y)]
            .set_char('B')
            .set_style(self.theme.deck_b_style());

        // Render status text on the line above (if we have space)
        if inner.height >= 2 {
            let status_y = inner.y;

            // Show offset in ms for precise alignment (e.g., "+12ms" or "-5ms")
            let offset_text = if let Some(ms) = self.offset_ms() {
                if ms.abs() < 1.0 {
                    "SYNC".to_string()
                } else if ms > 0.0 {
                    format!("+{:.0}ms", ms)
                } else {
                    format!("{:.0}ms", ms)
                }
            } else {
                status.to_string()
            };

            if !offset_text.is_empty() {
                let status_x = inner.x + (width.saturating_sub(offset_text.len())) as u16 / 2;
                for (i, ch) in offset_text.chars().enumerate() {
                    let sx = status_x + i as u16;
                    if sx < inner.x + inner.width {
                        buf[(sx, status_y)].set_char(ch).set_style(marker_style);
                    }
                }
            }
        }
    }
}
