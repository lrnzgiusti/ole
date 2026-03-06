use egui::{Color32, Ui};

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

            // Feature indicators
            // Quantize
            if state.deck_a.quantize_enabled || state.deck_b.quantize_enabled {
                let label = if state.deck_a.quantize_enabled && state.deck_b.quantize_enabled {
                    "Q:AB"
                } else if state.deck_a.quantize_enabled {
                    "Q:A"
                } else {
                    "Q:B"
                };
                ui.label(
                    egui::RichText::new(label)
                        .color(Color32::from_rgb(255, 200, 0))
                        .monospace(),
                );
            }

            // Key Lock
            if state.deck_a.key_lock || state.deck_b.key_lock {
                let label = if state.deck_a.key_lock && state.deck_b.key_lock {
                    "KL:AB"
                } else if state.deck_a.key_lock {
                    "KL:A"
                } else {
                    "KL:B"
                };
                ui.label(
                    egui::RichText::new(label)
                        .color(Color32::from_rgb(100, 200, 255))
                        .monospace(),
                );
            }

            // Slip Mode
            if state.deck_a.slip_enabled || state.deck_b.slip_enabled {
                let label = if state.deck_a.slip_enabled && state.deck_b.slip_enabled {
                    "SLP:AB"
                } else if state.deck_a.slip_enabled {
                    "SLP:A"
                } else {
                    "SLP:B"
                };
                ui.label(
                    egui::RichText::new(label)
                        .color(Color32::from_rgb(255, 100, 255))
                        .monospace(),
                );
            }

            // Recording indicator
            if state.is_recording {
                let rec_m = (state.recording_duration / 60.0) as u32;
                let rec_s = (state.recording_duration % 60.0) as u32;
                ui.label(
                    egui::RichText::new(format!("REC {:02}:{:02}", rec_m, rec_s))
                        .color(Color32::from_rgb(255, 50, 50))
                        .strong()
                        .monospace(),
                );
            }

            // Sampler active indicator
            let active_slots: Vec<usize> = state.sampler_slots.iter().enumerate()
                .filter(|(_, (_, playing, _, _))| *playing)
                .map(|(i, _)| i + 1)
                .collect();
            if !active_slots.is_empty() {
                let slots_str: Vec<String> = active_slots.iter().map(|s| s.to_string()).collect();
                ui.label(
                    egui::RichText::new(format!("SPL:{}", slots_str.join(",")))
                        .color(Color32::from_rgb(255, 200, 100))
                        .monospace(),
                );
            }

            // Copilot indicator
            if state.library.copilot_enabled {
                ui.label(
                    egui::RichText::new(format!(
                        "\u{2605}{}",
                        state.library.energy_direction.symbol()
                    ))
                    .color(theme::ACCENT_CYAN)
                    .monospace(),
                );
            }

            // Mix cue: warn when a deck is approaching break/outro within 16 bars
            for (deck_label, is_a) in [("A", true), ("B", false)] {
                if let Some((phrase_type, bars)) = state.next_phrase(is_a) {
                    if bars <= 16.0
                        && (phrase_type == ole_analysis::PhraseType::Outro
                            || phrase_type == ole_analysis::PhraseType::Break)
                    {
                        ui.label(
                            egui::RichText::new(format!(
                                "MIX\u{2192}{}",
                                deck_label
                            ))
                            .color(Color32::from_rgb(255, 200, 0))
                            .strong()
                            .monospace(),
                        );
                    }
                }
            }

            // Mode indicator (right-aligned)
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mode_text = format!("[{:?}]", state.mode);
                ui.label(egui::RichText::new(mode_text).color(theme::PRIMARY).monospace());

                // Message display
                if let Some(ref msg) = state.message {
                    let msg_color = match state.message_type {
                        crate::state::MessageType::Info => theme::TEXT,
                        crate::state::MessageType::Success => theme::PRIMARY,
                        crate::state::MessageType::Warning => theme::WARNING,
                        crate::state::MessageType::Error => theme::ACCENT_PINK,
                    };
                    ui.label(egui::RichText::new(msg.as_str()).color(msg_color).monospace());
                }
            });
        });
    }
}
