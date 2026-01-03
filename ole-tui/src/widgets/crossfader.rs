//! Crossfader widget - visual mixer control

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::Span,
    widgets::{Block, Borders, Widget},
};
use crate::theme::Theme;

/// Widget for displaying the crossfader
pub struct CrossfaderWidget<'a> {
    position: f32, // -1.0 to 1.0
    theme: &'a Theme,
    bpm_a: Option<f32>,
    bpm_b: Option<f32>,
}

impl<'a> CrossfaderWidget<'a> {
    pub fn new(position: f32, theme: &'a Theme) -> Self {
        Self {
            position,
            theme,
            bpm_a: None,
            bpm_b: None,
        }
    }

    pub fn bpms(mut self, bpm_a: Option<f32>, bpm_b: Option<f32>) -> Self {
        self.bpm_a = bpm_a;
        self.bpm_b = bpm_b;
        self
    }
}

impl Widget for CrossfaderWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        use ratatui::style::{Modifier, Style};

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border())
            .title(Span::styled(" CROSSFADER ", self.theme.title()));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width < 10 || inner.height < 1 {
            return;
        }

        let width = inner.width as usize;

        // Row 0: BPM display with difference (if we have BPM data)
        if inner.height >= 1 {
            let bpm_a_str = self.bpm_a
                .map(|b| format!("{:.1}", b))
                .unwrap_or_else(|| "---".to_string());
            let bpm_b_str = self.bpm_b
                .map(|b| format!("{:.1}", b))
                .unwrap_or_else(|| "---".to_string());

            // Calculate BPM difference and style
            let (diff_str, diff_style) = match (self.bpm_a, self.bpm_b) {
                (Some(a), Some(b)) => {
                    let diff = b - a;
                    let style = if diff.abs() < 0.5 {
                        Style::from(self.theme.accent).add_modifier(Modifier::BOLD)
                    } else if diff.abs() < 2.0 {
                        Style::default().fg(self.theme.warning)
                    } else {
                        Style::default().fg(self.theme.danger)
                    };
                    let text = if diff.abs() < 0.1 {
                        "SYNC".to_string()
                    } else {
                        format!("{:+.1}", diff)
                    };
                    (text, style)
                }
                _ => ("--".to_string(), self.theme.dim()),
            };

            // Format: "A:128.0  [diff]  B:130.3"
            let bpm_line = format!("A:{}  {}  B:{}", bpm_a_str, diff_str, bpm_b_str);
            let bpm_x = inner.x + (width.saturating_sub(bpm_line.len())) as u16 / 2;
            let bpm_y = inner.y;

            // Render BPM line with proper styling
            let mut x = bpm_x;
            // A:xxx.x
            for ch in format!("A:{}", bpm_a_str).chars() {
                if x < inner.x + inner.width {
                    let style = if ch == 'A' || ch == ':' {
                        self.theme.deck_a_style()
                    } else {
                        self.theme.normal()
                    };
                    buf[(x, bpm_y)].set_char(ch).set_style(style);
                    x += 1;
                }
            }
            // Spacing
            for _ in 0..2 {
                if x < inner.x + inner.width {
                    buf[(x, bpm_y)].set_char(' ');
                    x += 1;
                }
            }
            // Diff
            for ch in diff_str.chars() {
                if x < inner.x + inner.width {
                    buf[(x, bpm_y)].set_char(ch).set_style(diff_style);
                    x += 1;
                }
            }
            // Spacing
            for _ in 0..2 {
                if x < inner.x + inner.width {
                    buf[(x, bpm_y)].set_char(' ');
                    x += 1;
                }
            }
            // B:xxx.x
            for ch in format!("B:{}", bpm_b_str).chars() {
                if x < inner.x + inner.width {
                    let style = if ch == 'B' || ch == ':' {
                        self.theme.deck_b_style()
                    } else {
                        self.theme.normal()
                    };
                    buf[(x, bpm_y)].set_char(ch).set_style(style);
                    x += 1;
                }
            }
        }

        // Calculate fader position
        // position: -1.0 (full A) to 1.0 (full B)
        let normalized = (self.position + 1.0) / 2.0; // 0.0 to 1.0
        let fader_pos = (normalized * (width - 1) as f32) as usize;

        // Build the crossfader line
        let mut line = String::with_capacity(width);

        // Label for A side
        line.push('A');

        for i in 1..width - 1 {
            if i == fader_pos {
                line.push('●');
            } else if i == width / 2 {
                line.push('┼');
            } else {
                line.push('─');
            }
        }

        // Label for B side
        line.push('B');

        // Render fader on middle row
        let y = inner.y + inner.height / 2;
        for (i, ch) in line.chars().enumerate() {
            let x = inner.x + i as u16;
            let style = match ch {
                'A' => self.theme.deck_a_style(),
                'B' => self.theme.deck_b_style(),
                '●' => self.theme.highlight(),
                '┼' => self.theme.dim(),
                _ => self.theme.normal(),
            };
            buf[(x, y)].set_char(ch).set_style(style);
        }
    }
}
