use egui::Ui;

use crate::state::GuiState;
use crate::theme;

pub struct PhaseWidget;

impl PhaseWidget {
    pub fn show(ui: &mut Ui, state: &GuiState) {
        ui.horizontal(|ui| {
            // Deck A phase
            Self::draw_phase_dots(ui, state.deck_a.beat_phase, theme::DECK_A, "A");

            ui.separator();

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

            // BPM display
            let bpm_a = state.deck_a.bpm.unwrap_or(0.0) * state.deck_a.tempo;
            let bpm_b = state.deck_b.bpm.unwrap_or(0.0) * state.deck_b.tempo;
            if bpm_a > 0.0 && bpm_b > 0.0 {
                let diff = bpm_a - bpm_b;
                let sign = if diff >= 0.0 { "+" } else { "" };
                ui.label(
                    egui::RichText::new(format!("{}{:.1}", sign, diff))
                        .color(theme::TEXT_DIM)
                        .monospace(),
                );
            }

            ui.separator();

            // Deck B phase
            Self::draw_phase_dots(ui, state.deck_b.beat_phase, theme::DECK_B, "B");
        });
    }

    fn draw_phase_dots(ui: &mut Ui, phase: f32, color: egui::Color32, label: &str) {
        ui.label(egui::RichText::new(label).color(color).monospace());

        let current_beat = ((phase * 4.0) as usize).min(3);
        let dots: String = (0..4)
            .map(|i| if i == current_beat { 'o' } else { '.' })
            .collect();
        let beat_num = current_beat + 1;
        ui.label(
            egui::RichText::new(format!("[{}] {}/4", dots, beat_num))
                .color(color)
                .monospace(),
        );
    }
}
