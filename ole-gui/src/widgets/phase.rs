use egui::{Color32, Ui, Vec2};

use crate::state::GuiState;
use crate::theme;

pub struct PhaseWidget;

impl PhaseWidget {
    pub fn show(ui: &mut Ui, state: &GuiState) {
        ui.horizontal(|ui| {
            // Deck A phase circle
            Self::draw_phase_circle(ui, state.deck_a.beat_phase, theme::DECK_A);
            ui.label(egui::RichText::new("A").color(theme::DECK_A).monospace());

            ui.add_space(4.0);

            // Sync quality
            let quality_text = if state.sync_quality > 0.95 {
                "LOCKED"
            } else if state.sync_quality > 0.5 {
                "SYNC"
            } else {
                "---"
            };
            let quality_color = if state.sync_quality > 0.95 {
                theme::PRIMARY
            } else if state.sync_quality > 0.5 {
                theme::WARNING
            } else {
                theme::TEXT_DIM
            };
            ui.label(egui::RichText::new(quality_text).color(quality_color).monospace());

            // BPM diff
            let bpm_a = state.deck_a.bpm.unwrap_or(0.0) * state.deck_a.tempo;
            let bpm_b = state.deck_b.bpm.unwrap_or(0.0) * state.deck_b.tempo;
            if bpm_a > 0.0 && bpm_b > 0.0 {
                let diff = bpm_a - bpm_b;
                let sign = if diff >= 0.0 { "+" } else { "" };
                let diff_color = if diff.abs() < 0.1 {
                    theme::PRIMARY
                } else if diff.abs() < 1.0 {
                    theme::WARNING
                } else {
                    theme::ACCENT_PINK
                };
                ui.label(
                    egui::RichText::new(format!("{}{:.1}", sign, diff))
                        .color(diff_color)
                        .monospace(),
                );
                // Drift direction arrow
                if diff.abs() > 0.05 {
                    let arrow = if diff > 0.0 { ">" } else { "<" };
                    ui.label(egui::RichText::new(arrow).color(diff_color).monospace());
                }
            }

            ui.add_space(4.0);

            // Deck B phase circle
            ui.label(egui::RichText::new("B").color(theme::DECK_B).monospace());
            Self::draw_phase_circle(ui, state.deck_b.beat_phase, theme::DECK_B);
        });
    }

    /// Draw a circular phase indicator (rotating dot on a circle)
    fn draw_phase_circle(ui: &mut Ui, phase: f32, color: Color32) {
        let size = 16.0;
        let (response, painter) = ui.allocate_painter(Vec2::splat(size), egui::Sense::hover());
        let center = response.rect.center();
        let radius = size / 2.0 - 1.0;

        // Draw circle outline
        painter.circle_stroke(
            center,
            radius,
            egui::Stroke::new(1.0, Color32::from_rgba_premultiplied(
                color.r(), color.g(), color.b(), 60,
            )),
        );

        // Draw 4 tick marks (beat divisions)
        for i in 0..4 {
            let angle = std::f32::consts::PI * 2.0 * (i as f32 / 4.0) - std::f32::consts::FRAC_PI_2;
            let inner = radius - 2.0;
            painter.line_segment(
                [
                    egui::pos2(center.x + angle.cos() * inner, center.y + angle.sin() * inner),
                    egui::pos2(center.x + angle.cos() * radius, center.y + angle.sin() * radius),
                ],
                egui::Stroke::new(1.0, Color32::from_rgba_premultiplied(
                    color.r(), color.g(), color.b(), 40,
                )),
            );
        }

        // Draw rotating dot at current phase
        let angle = std::f32::consts::PI * 2.0 * phase - std::f32::consts::FRAC_PI_2;
        let dot_pos = egui::pos2(
            center.x + angle.cos() * (radius - 1.0),
            center.y + angle.sin() * (radius - 1.0),
        );
        painter.circle_filled(dot_pos, 2.5, color);
    }
}
