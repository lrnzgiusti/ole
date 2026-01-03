//! Spectrum analyzer widget - FFT visualization

use ole_analysis::{SpectrumData, SPECTRUM_BANDS};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::Span,
    widgets::{Block, Borders, Widget},
};
use crate::theme::Theme;

/// Characters for vertical bar rendering (8 levels)
const BAR_CHARS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Widget for displaying frequency spectrum
pub struct SpectrumWidget<'a> {
    spectrum_a: &'a SpectrumData,
    spectrum_b: &'a SpectrumData,
    theme: &'a Theme,
    show_both: bool,
}

impl<'a> SpectrumWidget<'a> {
    pub fn new(spectrum_a: &'a SpectrumData, spectrum_b: &'a SpectrumData, theme: &'a Theme) -> Self {
        Self {
            spectrum_a,
            spectrum_b,
            theme,
            show_both: true,
        }
    }

    #[allow(dead_code)]
    pub fn single(spectrum: &'a SpectrumData, theme: &'a Theme) -> Self {
        Self {
            spectrum_a: spectrum,
            spectrum_b: spectrum,
            theme,
            show_both: false,
        }
    }

    /// Get full height bar representation
    fn render_bar(magnitude: f32, height: u16) -> Vec<char> {
        let total_levels = (magnitude.clamp(0.0, 1.0) * 8.0 * height as f32) as usize;
        let full_blocks = total_levels / 8;
        let partial = total_levels % 8;

        let mut bar = Vec::with_capacity(height as usize);

        // Build from bottom to top
        for row in 0..height as usize {
            let char = if row < full_blocks {
                '█'
            } else if row == full_blocks && partial > 0 {
                BAR_CHARS[partial]
            } else {
                ' '
            };
            bar.push(char);
        }

        bar
    }
}

impl Widget for SpectrumWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border())
            .title(Span::styled(" SPECTRUM ", self.theme.title()));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 2 || inner.width < SPECTRUM_BANDS as u16 {
            return;
        }

        let width = inner.width as usize;
        let height = inner.height as usize;

        // Calculate how many bands we can display
        let bands_to_show = width.min(SPECTRUM_BANDS);
        let band_width = if self.show_both { 2 } else { 1 };
        let total_band_width = bands_to_show * band_width;
        let start_x = (width.saturating_sub(total_band_width)) / 2;

        for band in 0..bands_to_show {
            let band_idx = (band * SPECTRUM_BANDS) / bands_to_show;
            let mag_a = self.spectrum_a.bands[band_idx];
            let mag_b = self.spectrum_b.bands[band_idx];

            let bar_a = Self::render_bar(mag_a, height as u16);
            let bar_b = Self::render_bar(mag_b, height as u16);

            let x_a = inner.x + (start_x + band * band_width) as u16;

            // Render from bottom to top
            for row in 0..height {
                let y = inner.y + inner.height - 1 - row as u16;
                let char_a = bar_a[row];

                let style_a = self.theme.spectrum_style(band_idx, SPECTRUM_BANDS);

                if char_a != ' ' {
                    buf[(x_a, y)].set_char(char_a).set_style(style_a);
                }

                if self.show_both && band_width > 1 {
                    let x_b = x_a + 1;
                    let char_b = bar_b[row];
                    let style_b = self.theme.deck_b_style();

                    if char_b != ' ' && x_b < inner.x + inner.width {
                        buf[(x_b, y)].set_char(char_b).set_style(style_b);
                    }
                }
            }
        }
    }
}
