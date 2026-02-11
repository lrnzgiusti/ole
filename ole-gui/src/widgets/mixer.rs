use crossbeam_channel::Sender;
use egui::Ui;

use ole_audio::AudioCommand;
use crate::state::GuiState;
use crate::theme;
use crate::widgets::crossfader::draw_crossfader;
use crate::widgets::knob::knob;

pub struct MixerPanel;

impl MixerPanel {
    pub fn show(ui: &mut Ui, state: &mut GuiState, cmd_tx: &Sender<AudioCommand>) {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("MIXER")
                    .color(theme::PRIMARY)
                    .strong()
                    .monospace(),
            );

            // Master volume knob
            let mut vol = state.master_volume;
            if knob(ui, "master_vol", &mut vol, 0.0..=2.0, "MASTER", theme::PRIMARY) {
                let _ = cmd_tx.send(AudioCommand::SetMasterVolume(vol));
                state.master_volume = vol;
            }

            ui.add_space(4.0);

            // EQ knobs (visual only for now)
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("HI").color(theme::TEXT_DIM).monospace());
                ui.label(egui::RichText::new("MID").color(theme::TEXT_DIM).monospace());
                ui.label(egui::RichText::new("LO").color(theme::TEXT_DIM).monospace());
            });

            ui.add_space(4.0);

            // Crossfader
            let mut xf = state.crossfader;
            if draw_crossfader(ui, &mut xf) {
                let _ = cmd_tx.send(AudioCommand::SetCrossfader(xf));
                state.crossfader = xf;
            }

            ui.add_space(4.0);

            // Sync buttons
            ui.horizontal(|ui| {
                if ui
                    .button(egui::RichText::new("SYNC A>B").color(theme::DECK_A).monospace())
                    .clicked()
                {
                    let _ = cmd_tx.send(AudioCommand::SyncAToB);
                }
                if ui
                    .button(egui::RichText::new("SYNC B>A").color(theme::DECK_B).monospace())
                    .clicked()
                {
                    let _ = cmd_tx.send(AudioCommand::SyncBToA);
                }
            });
        });
    }
}
