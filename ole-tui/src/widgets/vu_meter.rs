//! Master VU Meter widget - shows deck levels with peak hold and LUFS

use crate::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::Span,
    widgets::{Block, Borders, Widget},
};

/// Widget for displaying master VU meter with dual deck levels
///
/// Shows vertical bar meters for both decks with:
/// - Peak level bars (green/yellow/red zones)
/// - Peak hold indicators
/// - LUFS momentary readout
/// - Gain reduction indicator
pub struct MasterVuMeterWidget<'a> {
    theme: &'a Theme,
    level_a: f32,     // Deck A peak level (0.0-1.0+)
    level_b: f32,     // Deck B peak level
    peak_hold_a: f32, // Peak hold for A
    peak_hold_b: f32, // Peak hold for B
    is_clipping_a: bool,
    is_clipping_b: bool,
    lufs_momentary: f32, // LUFS display
    gain_reduction: f32, // Limiter GR in dB
}

impl<'a> MasterVuMeterWidget<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            level_a: 0.0,
            level_b: 0.0,
            peak_hold_a: 0.0,
            peak_hold_b: 0.0,
            is_clipping_a: false,
            is_clipping_b: false,
            lufs_momentary: -70.0,
            gain_reduction: 0.0,
        }
    }

    pub fn levels(mut self, level_a: f32, level_b: f32) -> Self {
        self.level_a = level_a;
        self.level_b = level_b;
        self
    }

    pub fn peak_holds(mut self, peak_a: f32, peak_b: f32) -> Self {
        self.peak_hold_a = peak_a;
        self.peak_hold_b = peak_b;
        self
    }

    pub fn clipping(mut self, is_clipping_a: bool, is_clipping_b: bool) -> Self {
        self.is_clipping_a = is_clipping_a;
        self.is_clipping_b = is_clipping_b;
        self
    }

    pub fn lufs(mut self, lufs_momentary: f32) -> Self {
        self.lufs_momentary = lufs_momentary;
        self
    }

    pub fn gain_reduction(mut self, gr: f32) -> Self {
        self.gain_reduction = gr;
        self
    }

    /// Convert linear level to dB
    fn level_to_db(level: f32) -> f32 {
        if level <= 0.0 {
            -60.0
        } else {
            20.0 * level.log10()
        }
    }

    /// Map dB value to meter position (0.0-1.0)
    /// Range: -48dB to +6dB
    fn db_to_position(db: f32) -> f32 {
        const MIN_DB: f32 = -48.0;
        const MAX_DB: f32 = 6.0;
        ((db - MIN_DB) / (MAX_DB - MIN_DB)).clamp(0.0, 1.0)
    }

    /// Get color for a given dB level
    fn color_for_db(&self, db: f32) -> Style {
        if db > 0.0 {
            Style::default().fg(self.theme.danger) // Clipping - red
        } else if db > -6.0 {
            Style::default().fg(self.theme.warning) // Hot - yellow
        } else {
            Style::default().fg(self.theme.accent) // Normal - green
        }
    }
}

impl Widget for MasterVuMeterWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border())
            .title(Span::styled(" VU ", self.theme.title()));

        let inner = block.inner(area);
        block.render(area, buf);

        // Minimum size check
        if inner.width < 12 || inner.height < 5 {
            return;
        }

        let width = inner.width as usize;
        let height = inner.height as usize;

        // Reserve space: 1 row for LUFS, 1 row for labels, rest for meters
        let meter_height = height.saturating_sub(2);
        if meter_height < 3 {
            return;
        }

        // Calculate dB values
        let db_a = Self::level_to_db(self.level_a);
        let db_b = Self::level_to_db(self.level_b);
        let db_peak_a = Self::level_to_db(self.peak_hold_a);
        let db_peak_b = Self::level_to_db(self.peak_hold_b);

        // Calculate meter fill positions
        let fill_a = Self::db_to_position(db_a);
        let fill_b = Self::db_to_position(db_b);
        let peak_pos_a = Self::db_to_position(db_peak_a);
        let peak_pos_b = Self::db_to_position(db_peak_b);

        // Layout: dB scale | A meter | gap | B meter
        // Calculate positions for meters
        let scale_width = 3; // "-12"
        let meter_width = 3; // "┃█┃"
        let gap = 2;

        // Center the meters in available space
        let content_width = scale_width + 1 + meter_width + gap + meter_width;
        let start_x = inner.x + (width.saturating_sub(content_width)) as u16 / 2;

        // dB markers at specific heights
        let db_markers = [6, 0, -6, -12, -24, -36, -48];

        // Render dB scale and meters
        for row in 0..meter_height {
            let y = inner.y + row as u16;

            // Calculate dB value for this row (top = +6dB, bottom = -48dB)
            let row_ratio = row as f32 / (meter_height - 1) as f32;
            let row_db = 6.0 - (row_ratio * 54.0); // 6 to -48 dB range

            // dB scale markers
            let mut x = start_x;
            let marker_text = db_markers
                .iter()
                .find(|&&db| {
                    let marker_row =
                        ((6.0 - db as f32) / 54.0 * (meter_height - 1) as f32) as usize;
                    marker_row == row
                })
                .map(|&db| format!("{:>3}", db));

            if let Some(text) = marker_text {
                for (i, ch) in text.chars().enumerate() {
                    buf[(x + i as u16, y)]
                        .set_char(ch)
                        .set_style(self.theme.dim());
                }
            }
            x += scale_width as u16 + 1;

            // Calculate fill level for this row (inverted: top is high dB)
            let row_fill_threshold = 1.0 - row_ratio;
            let peak_row_size = 1.0 / meter_height as f32;

            // Meter A
            let state_a = MeterRowState {
                is_filled: fill_a >= row_fill_threshold,
                is_peak: peak_pos_a >= row_fill_threshold
                    && peak_pos_a < row_fill_threshold + peak_row_size,
                is_clip_indicator: self.is_clipping_a && row == 0,
            };
            self.render_meter_row(buf, x, y, state_a, row_db);
            x += meter_width as u16 + gap as u16;

            // Meter B
            let state_b = MeterRowState {
                is_filled: fill_b >= row_fill_threshold,
                is_peak: peak_pos_b >= row_fill_threshold
                    && peak_pos_b < row_fill_threshold + peak_row_size,
                is_clip_indicator: self.is_clipping_b && row == 0,
            };
            self.render_meter_row(buf, x, y, state_b, row_db);
        }

        // Labels row (A and B)
        let label_y = inner.y + meter_height as u16;
        let label_a_x = start_x + scale_width as u16 + 1 + 1; // Center under meter
        let label_b_x = label_a_x + meter_width as u16 + gap as u16;

        buf[(label_a_x, label_y)]
            .set_char('A')
            .set_style(self.theme.deck_a_style());
        buf[(label_b_x, label_y)]
            .set_char('B')
            .set_style(self.theme.deck_b_style());

        // LUFS and GR row
        let info_y = inner.y + meter_height as u16 + 1;
        if info_y < inner.y + inner.height {
            let lufs_text = format!("{:.1}L", self.lufs_momentary);
            let gr_text = if self.gain_reduction > 0.1 {
                format!(" -{:.1}dB", self.gain_reduction)
            } else {
                String::new()
            };

            let info_text = format!("{}{}", lufs_text, gr_text);
            let info_x = inner.x + (width.saturating_sub(info_text.len())) as u16 / 2;

            // LUFS value
            for (i, ch) in lufs_text.chars().enumerate() {
                let style = if self.lufs_momentary > -6.0 {
                    Style::default().fg(self.theme.danger)
                } else if self.lufs_momentary > -14.0 {
                    Style::default().fg(self.theme.warning)
                } else {
                    self.theme.dim()
                };
                buf[(info_x + i as u16, info_y)]
                    .set_char(ch)
                    .set_style(style);
            }

            // GR value (red if limiting)
            if !gr_text.is_empty() {
                let gr_x = info_x + lufs_text.len() as u16;
                for (i, ch) in gr_text.chars().enumerate() {
                    buf[(gr_x + i as u16, info_y)]
                        .set_char(ch)
                        .set_style(Style::default().fg(self.theme.danger));
                }
            }
        }
    }
}

/// Meter row state for rendering
struct MeterRowState {
    is_filled: bool,
    is_peak: bool,
    is_clip_indicator: bool,
}

impl MasterVuMeterWidget<'_> {
    /// Render a single row of a meter
    #[allow(clippy::too_many_arguments)]
    fn render_meter_row(
        &self,
        buf: &mut Buffer,
        x: u16,
        y: u16,
        state: MeterRowState,
        row_db: f32,
    ) {
        let color = self.color_for_db(row_db);

        // Left bracket
        buf[(x, y)].set_char('┃').set_style(self.theme.dim());

        // Fill character and style
        let (fill_char, fill_style) = if state.is_clip_indicator {
            ('!', Style::default().fg(self.theme.danger))
        } else if state.is_peak {
            ('▓', color)
        } else if state.is_filled {
            ('█', color)
        } else {
            (' ', self.theme.normal())
        };

        buf[(x + 1, y)].set_char(fill_char).set_style(fill_style);

        // Right bracket
        buf[(x + 2, y)].set_char('┃').set_style(self.theme.dim());
    }
}
