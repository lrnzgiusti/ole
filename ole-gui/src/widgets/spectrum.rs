use egui::{Rect, Ui, Vec2};

use crate::state::{GuiState, AFTERGLOW_HISTORY, SPECTRUM_BANDS};
use crate::theme;

pub struct SpectrumWidget;

impl SpectrumWidget {
    pub fn show(ui: &mut Ui, state: &GuiState) {
        let desired_size = Vec2::new(ui.available_width(), 80.0);
        let (response, painter) = ui.allocate_painter(desired_size, egui::Sense::hover());
        let rect = response.rect;

        // Background
        painter.rect_filled(rect, 0.0, theme::BG);

        let bands = &state.deck_a.spectrum.bands;
        let band_count = bands.len().min(SPECTRUM_BANDS);
        if band_count == 0 {
            return;
        }

        let bar_width = rect.width() / band_count as f32;
        let max_height = rect.height();

        for (i, &band) in bands.iter().enumerate().take(band_count) {
            let value = band.clamp(0.0, 1.0);
            let bar_height = value * max_height;

            let x = rect.left() + i as f32 * bar_width;
            let bar_rect = Rect::from_min_max(
                egui::pos2(x + 1.0, rect.bottom() - bar_height),
                egui::pos2(x + bar_width - 1.0, rect.bottom()),
            );

            let color = theme::CyberTheme::spectrum_color(i, band_count);
            painter.rect_filled(bar_rect, 0.0, color);

            // Afterglow: draw previous values as dimmer bars
            for h in 0..AFTERGLOW_HISTORY {
                let hist_idx =
                    (state.spectrum_history_idx + AFTERGLOW_HISTORY - h - 1) % AFTERGLOW_HISTORY;
                let hist_val = state.spectrum_history[i.min(SPECTRUM_BANDS - 1)][hist_idx].clamp(0.0, 1.0);
                if hist_val > value {
                    let decay = 1.0 - (h as f32 / AFTERGLOW_HISTORY as f32);
                    let alpha = (decay * 80.0) as u8;
                    let ghost_height = hist_val * max_height;
                    let ghost_rect = Rect::from_min_max(
                        egui::pos2(x + 1.0, rect.bottom() - ghost_height),
                        egui::pos2(x + bar_width - 1.0, rect.bottom() - bar_height),
                    );
                    let c = color;
                    let ghost_color = egui::Color32::from_rgba_premultiplied(
                        (c.r() as u32 * alpha as u32 / 255) as u8,
                        (c.g() as u32 * alpha as u32 / 255) as u8,
                        (c.b() as u32 * alpha as u32 / 255) as u8,
                        alpha,
                    );
                    painter.rect_filled(ghost_rect, 0.0, ghost_color);
                }
            }
        }

        // Label
        painter.text(
            egui::pos2(rect.left() + 4.0, rect.top() + 2.0),
            egui::Align2::LEFT_TOP,
            "SPECTRUM",
            egui::FontId::monospace(10.0),
            theme::TEXT_DIM,
        );
    }
}
