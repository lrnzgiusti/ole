use egui::{Frame, Ui};

use crate::state::{GuiState, SortColumn};
use crate::theme;

pub struct LibraryPanel;

impl LibraryPanel {
    pub fn show(ui: &mut Ui, state: &mut GuiState) {
        // History view
        if state.library.show_history {
            Self::show_history(ui, state);
            return;
        }

        let title = if state.library.is_scanning {
            format!(
                "LIBRARY [{}/{}]",
                state.library.scan_progress.0, state.library.scan_progress.1
            )
        } else if state.library.copilot_enabled {
            let count = state.library.filtered_tracks().len();
            let total = state.library.tracks.len();
            let dir_sym = state.library.energy_direction.symbol();
            format!("LIBRARY [COPILOT {} {}/{}]", dir_sym, count, total)
        } else {
            let count = state.library.filtered_tracks().len();
            let total = state.library.tracks.len();
            if !state.library.compatible_keys.is_empty() {
                format!("LIBRARY [{}/{} harmonic]", count, total)
            } else if state.library.filter_key.is_some() {
                format!("LIBRARY [{}/{} filtered]", count, total)
            } else if !state.library.search_query.is_empty() {
                format!("LIBRARY [{}/{} match]", count, total)
            } else {
                format!("LIBRARY [{}]", total)
            }
        };

        Frame::none()
            .stroke(egui::Stroke::new(1.0, theme::DIM))
            .inner_margin(4.0)
            .show(ui, |ui| {
                // Title + search bar
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(title)
                            .color(theme::PRIMARY)
                            .strong()
                            .monospace(),
                    );

                    // Search indicator
                    if state.library_search_active {
                        ui.label(
                            egui::RichText::new(format!("/{}_", state.library.search_query))
                                .color(theme::ACCENT_CYAN)
                                .monospace(),
                        );
                    } else if !state.library.search_query.is_empty() {
                        ui.label(
                            egui::RichText::new(format!("/{}", state.library.search_query))
                                .color(theme::TEXT_DIM)
                                .monospace(),
                        );
                    }

                    // Sort indicator (right-aligned)
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let arrow = if state.library.sort_ascending { "▲" } else { "▼" };
                        ui.label(
                            egui::RichText::new(format!("{}{}", state.library.sort_column.label(), arrow))
                                .color(theme::TEXT_DIM)
                                .monospace(),
                        );
                    });
                });

                // Column headers (clickable sort indicators)
                ui.horizontal(|ui| {
                    let cols = [
                        (SortColumn::Key, " KEY", 30.0),
                        (SortColumn::Bpm, "    BPM", 50.0),
                        (SortColumn::Duration, "  TIME", 45.0),
                        (SortColumn::Title, "  TITLE", 200.0),
                    ];
                    for (col, label, _) in &cols {
                        let is_active = state.library.sort_column == *col;
                        let color = if is_active { theme::ACCENT_CYAN } else { theme::TEXT_DIM };
                        let suffix = if is_active {
                            if state.library.sort_ascending { "▲" } else { "▼" }
                        } else { "" };
                        ui.label(
                            egui::RichText::new(format!("{}{}", label, suffix))
                                .color(color)
                                .monospace(),
                        );
                    }
                });

                // Collect track display data
                let filtered = state.library.filtered_tracks();
                let selected = state.library.selected_index;
                let copilot_on = state.library.copilot_enabled;

                // Harmonic compatibility info
                let current_key = state.library.current_playing_key.clone();
                let compatible = &state.library.compatible_keys;

                // score_threshold: high = above 0.6, medium = above 0.3
                let track_data: Vec<(String, bool, bool, f32)> = filtered
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

                        // Played indicator
                        let played = state.library.history.iter().any(|h| h.path == track.path);
                        let played_mark = if played { "\u{25b8}" } else { " " };

                        // Score bar when copilot is enabled
                        let score = state.library.copilot_scores.get(&track.path).copied().unwrap_or(0.0);
                        let score_str = if copilot_on {
                            let filled = (score * 5.0).round() as usize;
                            let empty = 5usize.saturating_sub(filled);
                            format!("{}{}", "\u{2588}".repeat(filled), "\u{2591}".repeat(empty))
                        } else {
                            String::new()
                        };

                        let text = if copilot_on {
                            format!("{}{} {} {} {} {}", played_mark, key_str, bpm_str, time_str, score_str, track.title)
                        } else {
                            format!("{}{} {} {}  {}", played_mark, key_str, bpm_str, time_str, track.title)
                        };

                        // Is this key harmonically compatible?
                        let is_compatible = track.key.as_ref().map(|k| {
                            compatible.contains(k) || current_key.as_ref().map(|ck| ck == k).unwrap_or(false)
                        }).unwrap_or(false);

                        (text, i == selected, is_compatible, score)
                    })
                    .collect();

                // Track list with scroll
                let mut clicked_index = None;
                let should_scroll = state.library.needs_scroll;
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for (i, (text, is_selected, is_compatible, score)) in track_data.iter().enumerate() {
                            let text_color = if *is_selected {
                                theme::BG
                            } else if copilot_on {
                                if *score > 0.6 {
                                    theme::PRIMARY
                                } else if *score > 0.3 {
                                    theme::TEXT
                                } else {
                                    theme::TEXT_DIM
                                }
                            } else if *is_compatible {
                                theme::PRIMARY
                            } else {
                                theme::TEXT
                            };
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

                            if *is_selected && should_scroll {
                                response.scroll_to_me(Some(egui::Align::Center));
                            }

                            if response.clicked() {
                                clicked_index = Some(i);
                            }
                        }
                    });

                if should_scroll {
                    state.library.needs_scroll = false;
                }

                if let Some(idx) = clicked_index {
                    state.library.selected_index = idx;
                }

                // Footer: key bindings hint
                ui.horizontal(|ui| {
                    let hint = if copilot_on {
                        "a/b:load  e:energy  ^P:copilot  s:sort  /:search"
                    } else {
                        "a/b:load  f:harmonic  c:clear  s:sort  /:search  h:history"
                    };
                    ui.label(
                        egui::RichText::new(hint)
                            .color(theme::TEXT_DIM)
                            .monospace()
                            .size(9.0),
                    );
                });
            });
    }

    fn show_history(ui: &mut Ui, state: &mut GuiState) {
        Frame::none()
            .stroke(egui::Stroke::new(1.0, theme::DIM))
            .inner_margin(4.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("HISTORY [{}]", state.library.history.len()))
                            .color(theme::ACCENT_CYAN)
                            .strong()
                            .monospace(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("h:back to library")
                                .color(theme::TEXT_DIM)
                                .monospace()
                                .size(9.0),
                        );
                    });
                });

                // Header
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(" #  KEY").color(theme::TEXT_DIM).monospace());
                    ui.label(egui::RichText::new("    BPM").color(theme::TEXT_DIM).monospace());
                    ui.label(egui::RichText::new("  TITLE").color(theme::TEXT_DIM).monospace());
                });

                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for (i, entry) in state.library.history.iter().enumerate().rev() {
                            let key_str = entry.key.as_ref()
                                .map(|k| format!("{:>3}", k))
                                .unwrap_or_else(|| " ? ".to_string());
                            let bpm_str = entry.bpm
                                .map(|b| format!("{:6.1}", b))
                                .unwrap_or_else(|| "  --- ".to_string());
                            let display_num = state.library.history.len() - i;
                            let text = format!("{:2}. {} {}  {}", display_num, key_str, bpm_str, entry.title);
                            ui.label(
                                egui::RichText::new(text)
                                    .color(theme::TEXT)
                                    .monospace(),
                            );
                        }
                        if state.library.history.is_empty() {
                            ui.label(
                                egui::RichText::new("  No tracks played yet")
                                    .color(theme::TEXT_DIM)
                                    .monospace(),
                            );
                        }
                    });
            });
    }
}
