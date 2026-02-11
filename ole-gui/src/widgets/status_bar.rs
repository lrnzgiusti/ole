use egui::Ui;

use crate::state::GuiState;
use crate::theme;

pub struct StatusBar;

impl StatusBar {
    pub fn show(ui: &mut Ui, state: &GuiState) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("OLE")
                    .color(theme::PRIMARY)
                    .strong()
                    .monospace(),
            );

            // BPM display
            let bpm_a = state.deck_a.bpm.unwrap_or(0.0) * state.deck_a.tempo;
            if bpm_a > 0.0 {
                ui.label(
                    egui::RichText::new(format!("BPM:{:.1}", bpm_a))
                        .color(theme::TEXT)
                        .monospace(),
                );
            }

            // Master volume
            let vol_db = if state.master_volume > 0.0 {
                20.0 * state.master_volume.log10()
            } else {
                -60.0
            };
            ui.label(
                egui::RichText::new(format!("MASTER {:.1}dB", vol_db))
                    .color(theme::TEXT)
                    .monospace(),
            );

            // Mastering indicator
            if state.mastering_enabled {
                ui.label(
                    egui::RichText::new(format!("[{}]", state.mastering_preset.display_name()))
                        .color(theme::ACCENT_CYAN)
                        .monospace(),
                );
            }

            // LUFS
            if state.mastering_lufs.momentary > -60.0 {
                ui.label(
                    egui::RichText::new(format!("{:.1}LUFS", state.mastering_lufs.momentary))
                        .color(theme::TEXT_DIM)
                        .monospace(),
                );
            }

            // Mode indicator (right-aligned)
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mode_text = format!("[{:?}]", state.mode);
                ui.label(egui::RichText::new(mode_text).color(theme::PRIMARY).monospace());
            });
        });
    }
}
