use egui::{Frame, Ui};

use crate::state::GuiState;
use crate::theme;

pub struct LibraryPanel;

impl LibraryPanel {
    pub fn show(ui: &mut Ui, state: &mut GuiState) {
        let title = if state.library.is_scanning {
            format!(
                "LIBRARY [{}/{}]",
                state.library.scan_progress.0, state.library.scan_progress.1
            )
        } else {
            let count = state.library.filtered_tracks().len();
            if state.library.filter_key.is_some() {
                format!("LIBRARY [{} filtered]", count)
            } else {
                format!("LIBRARY [{}]", count)
            }
        };

        Frame::none()
            .stroke(egui::Stroke::new(1.0, theme::DIM))
            .inner_margin(4.0)
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(title)
                        .color(theme::PRIMARY)
                        .strong()
                        .monospace(),
                );

                // Header
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("KEY").color(theme::TEXT_DIM).monospace());
                    ui.label(egui::RichText::new("    BPM").color(theme::TEXT_DIM).monospace());
                    ui.label(egui::RichText::new("  TIME").color(theme::TEXT_DIM).monospace());
                    ui.label(egui::RichText::new("  TITLE").color(theme::TEXT_DIM).monospace());
                });

                // Collect track display data before entering scroll area
                let filtered = state.library.filtered_tracks();
                let selected = state.library.selected_index;
                let track_data: Vec<(String, bool)> = filtered
                    .iter()
                    .enumerate()
                    .map(|(i, track)| {
                        let key_str = track
                            .key
                            .as_ref()
                            .map(|k| format!("{:>3}", k))
                            .unwrap_or_else(|| " ? ".to_string());
                        let bpm_str = track
                            .bpm
                            .map(|b| format!("{:6.1}", b))
                            .unwrap_or_else(|| "  --- ".to_string());
                        let dur_m = (track.duration_secs / 60.0) as u32;
                        let dur_s = (track.duration_secs % 60.0) as u32;
                        let time_str = format!("{:2}:{:02}", dur_m, dur_s);
                        let text = format!("{} {} {}  {}", key_str, bpm_str, time_str, track.title);
                        (text, i == selected)
                    })
                    .collect();

                // Track list with scroll
                let mut clicked_index = None;
                let should_scroll = state.library.needs_scroll;
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for (i, (text, is_selected)) in track_data.iter().enumerate() {
                            let text_color = if *is_selected { theme::BG } else { theme::TEXT };
                            let bg = if *is_selected { theme::PRIMARY } else { theme::BG };

                            let response = ui.add(
                                egui::Label::new(
                                    egui::RichText::new(text)
                                        .color(text_color)
                                        .background_color(bg)
                                        .monospace(),
                                )
                                .sense(egui::Sense::click()),
                            );

                            // Only scroll when selection changed programmatically
                            if *is_selected && should_scroll {
                                response.scroll_to_me(Some(egui::Align::Center));
                            }

                            if response.clicked() {
                                clicked_index = Some(i);
                            }
                        }
                    });
                // Consume the scroll flag
                if should_scroll {
                    state.library.needs_scroll = false;
                }

                if let Some(idx) = clicked_index {
                    state.library.selected_index = idx;
                }
            });
    }
}
