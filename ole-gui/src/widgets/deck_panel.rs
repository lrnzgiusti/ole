use egui::{Frame, Ui};

use ole_input::{Command, DeckId};
use crate::state::{FocusedPane, GuiState};
use crate::theme;
use super::waveform::{draw_waveform, draw_overview};
use super::vu_meter::draw_vu_meter;
use super::transport::draw_transport;

pub struct DeckPanel;

impl DeckPanel {
    pub fn show(ui: &mut Ui, state: &mut GuiState, is_deck_a: bool) -> Option<Command> {
        let mut command = None;
        let deck_color = theme::CyberTheme::deck_color(is_deck_a);
        let focused = if is_deck_a {
            state.focused == FocusedPane::DeckA
        } else {
            state.focused == FocusedPane::DeckB
        };

        let border_color = if focused { deck_color } else { theme::DIM };
        let label = if is_deck_a { "DECK A" } else { "DECK B" };

        Frame::none()
            .stroke(egui::Stroke::new(1.0, border_color))
            .inner_margin(4.0)
            .show(ui, |ui| {
                // Header: deck name, track info, key, BPM
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(label)
                            .color(deck_color)
                            .strong()
                            .monospace(),
                    );
                    let d = if is_deck_a { &state.deck_a } else { &state.deck_b };
                    if let Some(ref name) = d.track_name {
                        ui.label(
                            egui::RichText::new(format!("\"{}\"", name))
                                .color(theme::TEXT)
                                .monospace(),
                        );
                    }
                    if let Some(ref key) = d.key {
                        ui.label(
                            egui::RichText::new(key)
                                .color(theme::ACCENT_CYAN)
                                .monospace(),
                        );
                    }
                    if let Some(bpm) = d.bpm {
                        let effective_bpm = bpm * d.tempo;
                        ui.label(
                            egui::RichText::new(format!("{:.1}", effective_bpm))
                                .color(theme::TEXT)
                                .monospace(),
                        );
                    }

                    // Phrase countdown indicator
                    if let Some((phrase_type, bars)) = state.next_phrase(is_deck_a) {
                        let bars_int = bars.ceil() as u32;
                        let label = phrase_type.label();
                        let (text, color) = if bars_int <= 2 {
                            (format!("!{}>{}",  bars_int, label), theme::ACCENT_PINK)
                        } else {
                            (format!("{}>{}",  bars_int, label), theme::TEXT_DIM)
                        };
                        ui.label(
                            egui::RichText::new(text)
                                .color(color)
                                .monospace(),
                        );
                    }
                });

                // Mini overview waveform (click to seek)
                let deck_id = if is_deck_a { DeckId::A } else { DeckId::B };
                if let Some(seek_frac) = draw_overview(ui, state, is_deck_a) {
                    let d = if is_deck_a { &state.deck_a } else { &state.deck_b };
                    let seek_secs = seek_frac * d.duration;
                    command = Some(Command::Seek(deck_id, seek_secs));
                }

                // Main waveform (click-to-seek)
                if let Some(seek_frac) = draw_waveform(ui, state, is_deck_a) {
                    command = Some(Command::Seek(deck_id, seek_frac));
                }

                // Transport + VU meter row
                ui.horizontal(|ui| {
                    let d = if is_deck_a { &state.deck_a } else { &state.deck_b };
                    draw_transport(ui, d.playback, d.position, d.duration);

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let peak = if is_deck_a { state.vu_peak_a } else { state.vu_peak_b };
                        let clipping = if is_deck_a {
                            state.deck_a.is_clipping
                        } else {
                            state.deck_b.is_clipping
                        };
                        let level = if is_deck_a {
                            state.deck_a.peak_level
                        } else {
                            state.deck_b.peak_level
                        };
                        draw_vu_meter(ui, level, peak, clipping);
                    });
                });

                // Tempo + Gain + EQ indicators
                ui.horizontal(|ui| {
                    let d = if is_deck_a { &state.deck_a } else { &state.deck_b };
                    let tempo_pct = (d.tempo - 1.0) * 100.0;
                    let sign = if tempo_pct >= 0.0 { "+" } else { "" };
                    ui.label(
                        egui::RichText::new(format!("Tempo {}{:.1}%", sign, tempo_pct))
                            .color(theme::TEXT)
                            .monospace(),
                    );
                    ui.label(
                        egui::RichText::new(format!("Gain {:.2}", d.gain))
                            .color(theme::TEXT)
                            .monospace(),
                    );

                    // EQ indicators
                    let (lo, mid, hi) = if is_deck_a {
                        (state.eq_a_low, state.eq_a_mid, state.eq_a_high)
                    } else {
                        (state.eq_b_low, state.eq_b_mid, state.eq_b_high)
                    };
                    let (lo_k, mid_k, hi_k) = if is_deck_a {
                        (state.eq_a_low_kill, state.eq_a_mid_kill, state.eq_a_high_kill)
                    } else {
                        (state.eq_b_low_kill, state.eq_b_mid_kill, state.eq_b_high_kill)
                    };
                    let eq_changed = lo.abs() > 0.01 || mid.abs() > 0.01 || hi.abs() > 0.01
                        || lo_k || mid_k || hi_k;
                    if eq_changed {
                        let lo_str = if lo_k { "X".to_string() } else { format!("{:.0}dB", lo) };
                        let mid_str = if mid_k { "X".to_string() } else { format!("{:.0}dB", mid) };
                        let hi_str = if hi_k { "X".to_string() } else { format!("{:.0}dB", hi) };
                        let eq_text = format!("EQ L:{} M:{} H:{}", lo_str, mid_str, hi_str);
                        ui.label(
                            egui::RichText::new(eq_text)
                                .color(theme::ACCENT_CYAN)
                                .monospace()
                                .size(10.0),
                        );
                    }
                });
            });

        command
    }
}
