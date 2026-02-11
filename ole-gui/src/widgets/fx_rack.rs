use crossbeam_channel::Sender;
use egui::Ui;

use ole_audio::AudioCommand;
use crate::state::GuiState;
use crate::theme;

pub struct FxRack;

impl FxRack {
    pub fn show(ui: &mut Ui, state: &mut GuiState, _cmd_tx: &Sender<AudioCommand>, is_deck_a: bool) {
        let label = if is_deck_a { "FX DECK A" } else { "FX DECK B" };
        let deck_color = theme::CyberTheme::deck_color(is_deck_a);

        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(label)
                    .color(deck_color)
                    .strong()
                    .monospace(),
            );

            // Filter
            let (filter_en, filter_lvl) = if is_deck_a {
                (state.filter_a_enabled, state.filter_a_level)
            } else {
                (state.filter_b_enabled, state.filter_b_level)
            };
            Self::fx_toggle(ui, "FILT", filter_en, filter_lvl, deck_color);

            // Delay
            let (delay_en, delay_lvl) = if is_deck_a {
                (state.delay_a_enabled, state.delay_a_level)
            } else {
                (state.delay_b_enabled, state.delay_b_level)
            };
            Self::fx_toggle(ui, "DLY", delay_en, delay_lvl, deck_color);

            // Reverb
            let (reverb_en, reverb_lvl) = if is_deck_a {
                (state.reverb_a_enabled, state.reverb_a_level)
            } else {
                (state.reverb_b_enabled, state.reverb_b_level)
            };
            Self::fx_toggle(ui, "VERB", reverb_en, reverb_lvl, deck_color);
        });
    }

    fn fx_toggle(ui: &mut Ui, name: &str, enabled: bool, level: u8, color: egui::Color32) {
        ui.horizontal(|ui| {
            let text_color = if enabled { color } else { theme::TEXT_DIM };
            let status = if enabled {
                format!("[{}] {}", name, level)
            } else {
                format!("[{}] OFF", name)
            };
            ui.label(egui::RichText::new(status).color(text_color).monospace());
        });
    }
}
