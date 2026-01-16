//! Oscilloscope widget - classic CRT waveform visualization

use crate::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::Span,
    widgets::{Block, Borders, Widget},
};

/// Oscilloscope visualization mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScopeMode {
    /// Time domain - amplitude over time (classic oscilloscope)
    #[default]
    TimeDomain,
    /// Lissajous/X-Y - stereo field visualization (L vs R)
    Lissajous,
}

/// Widget for oscilloscope-style audio visualization
pub struct ScopeWidget<'a> {
    /// Audio samples from deck A (stereo interleaved)
    samples_a: &'a [f32],
    /// Audio samples from deck B (stereo interleaved)
    samples_b: &'a [f32],
    theme: &'a Theme,
    mode: ScopeMode,
}

impl<'a> ScopeWidget<'a> {
    pub fn new(samples_a: &'a [f32], samples_b: &'a [f32], theme: &'a Theme) -> Self {
        Self {
            samples_a,
            samples_b,
            theme,
            mode: ScopeMode::TimeDomain,
        }
    }

    pub fn mode(mut self, mode: ScopeMode) -> Self {
        self.mode = mode;
        self
    }

    /// Render time domain oscilloscope (amplitude over time)
    fn render_time_domain(&self, inner: Rect, buf: &mut Buffer) {
        let width = inner.width as usize;
        let height = inner.height as usize;

        if width < 4 || height < 2 {
            return;
        }

        // Unicode characters for smooth waveform rendering
        // Using braille patterns for fine detail, or block elements for simpler look
        let mid_y = height / 2;

        // Render center line (zero crossing)
        for x in 0..width {
            let px = inner.x + x as u16;
            let py = inner.y + mid_y as u16;
            if py < inner.y + inner.height {
                buf[(px, py)].set_char('─').set_style(self.theme.dim());
            }
        }

        // Helper to render a single channel's waveform
        let render_channel = |samples: &[f32], style: Style, buf: &mut Buffer| {
            if samples.is_empty() {
                return;
            }

            // Map samples to display width
            let samples_per_col = (samples.len() / 2).max(1) / width.max(1);
            let samples_per_col = samples_per_col.max(1);

            for x in 0..width {
                // Get sample range for this column
                let start = x * samples_per_col * 2; // *2 for stereo
                let end = ((x + 1) * samples_per_col * 2).min(samples.len());

                if start >= samples.len() {
                    break;
                }

                // Average the mono signal for this column
                let mut sum = 0.0f32;
                let mut count = 0;
                for i in (start..end).step_by(2) {
                    if i + 1 < samples.len() {
                        sum += (samples[i] + samples[i + 1]) * 0.5; // mono
                        count += 1;
                    }
                }

                if count == 0 {
                    continue;
                }

                let avg = sum / count as f32;

                // Map amplitude (-1.0 to 1.0) to screen position
                // Clamp to avoid drawing outside bounds
                let normalized = avg.clamp(-1.0, 1.0);
                let y_offset = (normalized * (mid_y as f32 - 0.5)) as i32;
                let y = (mid_y as i32 - y_offset).clamp(0, height as i32 - 1) as u16;

                let px = inner.x + x as u16;
                let py = inner.y + y;

                if py >= inner.y && py < inner.y + inner.height {
                    // Choose character based on amplitude
                    let ch = if normalized.abs() > 0.7 {
                        '█'
                    } else if normalized.abs() > 0.3 {
                        '▓'
                    } else if normalized.abs() > 0.1 {
                        '░'
                    } else {
                        '·'
                    };
                    buf[(px, py)].set_char(ch).set_style(style);
                }
            }
        };

        // Render both decks (A in deck_a color, B in deck_b color)
        render_channel(self.samples_a, self.theme.deck_a_style(), buf);
        render_channel(self.samples_b, self.theme.deck_b_style(), buf);
    }

    /// Render Lissajous/X-Y stereo field visualization
    fn render_lissajous(&self, inner: Rect, buf: &mut Buffer) {
        let width = inner.width as usize;
        let height = inner.height as usize;

        if width < 4 || height < 2 {
            return;
        }

        let mid_x = width / 2;
        let mid_y = height / 2;

        // Draw crosshairs at center
        for x in 0..width {
            let px = inner.x + x as u16;
            let py = inner.y + mid_y as u16;
            if py < inner.y + inner.height {
                buf[(px, py)].set_char('─').set_style(self.theme.dim());
            }
        }
        for y in 0..height {
            let px = inner.x + mid_x as u16;
            let py = inner.y + y as u16;
            if px < inner.x + inner.width {
                buf[(px, py)].set_char('│').set_style(self.theme.dim());
            }
        }
        // Center cross
        let cx = inner.x + mid_x as u16;
        let cy = inner.y + mid_y as u16;
        if cx < inner.x + inner.width && cy < inner.y + inner.height {
            buf[(cx, cy)].set_char('┼').set_style(self.theme.dim());
        }

        // Helper to render Lissajous for one deck
        let render_lissajous_deck = |samples: &[f32], style: Style, buf: &mut Buffer| {
            if samples.len() < 2 {
                return;
            }

            // Plot L vs R (X-Y mode)
            // Downsample to avoid too many points
            let step = (samples.len() / 2 / 200).max(1);

            for i in (0..samples.len()).step_by(step * 2) {
                if i + 1 >= samples.len() {
                    break;
                }

                let l = samples[i].clamp(-1.0, 1.0);
                let r = samples[i + 1].clamp(-1.0, 1.0);

                // Map L to X, R to Y
                let x = mid_x as f32 + l * (mid_x as f32 - 0.5);
                let y = mid_y as f32 - r * (mid_y as f32 - 0.5);

                let px = inner.x + (x as usize).min(width - 1) as u16;
                let py = inner.y + (y as usize).min(height - 1) as u16;

                if px >= inner.x
                    && px < inner.x + inner.width
                    && py >= inner.y
                    && py < inner.y + inner.height
                {
                    buf[(px, py)].set_char('●').set_style(style);
                }
            }
        };

        render_lissajous_deck(self.samples_a, self.theme.deck_a_style(), buf);
        render_lissajous_deck(self.samples_b, self.theme.deck_b_style(), buf);
    }
}

impl Widget for ScopeWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = match self.mode {
            ScopeMode::TimeDomain => " SCOPE ",
            ScopeMode::Lissajous => " STEREO ",
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border())
            .title(Span::styled(title, self.theme.title()));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width < 4 || inner.height < 2 {
            return;
        }

        match self.mode {
            ScopeMode::TimeDomain => self.render_time_domain(inner, buf),
            ScopeMode::Lissajous => self.render_lissajous(inner, buf),
        }
    }
}
