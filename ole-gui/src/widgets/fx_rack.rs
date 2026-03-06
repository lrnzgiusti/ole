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
        let in_fx_mode = state.mode == ole_input::Mode::Effects;
        let cursor = state.fx_cursor;

        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(label)
                    .color(deck_color)
                    .strong()
                    .monospace(),
            );

            // Filter (uses level, no mix)
            let (filter_en, filter_lvl) = if is_deck_a {
                (state.filter_a_enabled, state.filter_a_level)
            } else {
                (state.filter_b_enabled, state.filter_b_level)
            };
            let filter_mode_str = if in_fx_mode && cursor == 0 {
                let mode = if is_deck_a { state.filter_a_mode } else { state.filter_b_mode };
                Some(match mode {
                    ole_audio::FilterMode::Biquad => "Biquad",
                    ole_audio::FilterMode::Ladder => "Ladder",
                    ole_audio::FilterMode::SVF => "SVF",
                })
            } else {
                None
            };
            Self::fx_level(ui, "FILT", filter_en, filter_lvl, deck_color,
                in_fx_mode && cursor == 0, filter_mode_str);

            // EQ (always on, show kill switches)
            let (kill_l, kill_m, kill_h) = if is_deck_a {
                (state.eq_a_low_kill, state.eq_a_mid_kill, state.eq_a_high_kill)
            } else {
                (state.eq_b_low_kill, state.eq_b_mid_kill, state.eq_b_high_kill)
            };
            Self::fx_eq(ui, kill_l, kill_m, kill_h, deck_color, in_fx_mode && cursor == 1);

            // Delay
            let delay_en = if is_deck_a { state.delay_a_enabled } else { state.delay_b_enabled };
            let delay_mix = if is_deck_a { state.delay_a_mix } else { state.delay_b_mix };
            let delay_mode_str = if in_fx_mode && cursor == 2 {
                let mode = if is_deck_a { state.delay_a_mode } else { state.delay_b_mode };
                Some(match mode {
                    ole_audio::DelayMode::Stereo => "Stereo",
                    ole_audio::DelayMode::PingPong => "PingPong",
                    ole_audio::DelayMode::Mono => "Mono",
                })
            } else {
                None
            };
            Self::fx_mix(ui, "DLY", delay_en, delay_mix, deck_color,
                in_fx_mode && cursor == 2, delay_mode_str);

            // Reverb
            let reverb_en = if is_deck_a { state.reverb_a_enabled } else { state.reverb_b_enabled };
            let reverb_mix = if is_deck_a { state.reverb_a_mix } else { state.reverb_b_mix };
            Self::fx_mix(ui, "VERB", reverb_en, reverb_mix, deck_color,
                in_fx_mode && cursor == 3, None);

            // Flanger
            let flanger_en = if is_deck_a { state.flanger_a_enabled } else { state.flanger_b_enabled };
            let flanger_mix = if is_deck_a { state.flanger_a_mix } else { state.flanger_b_mix };
            Self::fx_mix(ui, "FLNG", flanger_en, flanger_mix, deck_color,
                in_fx_mode && cursor == 4, None);

            // Phaser
            let phaser_en = if is_deck_a { state.phaser_a_enabled } else { state.phaser_b_enabled };
            let phaser_mix = if is_deck_a { state.phaser_a_mix } else { state.phaser_b_mix };
            Self::fx_mix(ui, "PHSR", phaser_en, phaser_mix, deck_color,
                in_fx_mode && cursor == 5, None);

            // Bitcrusher
            let crush_en = if is_deck_a { state.bitcrusher_a_enabled } else { state.bitcrusher_b_enabled };
            let crush_mix = if is_deck_a { state.bitcrusher_a_mix } else { state.bitcrusher_b_mix };
            Self::fx_mix(ui, "CRSH", crush_en, crush_mix, deck_color,
                in_fx_mode && cursor == 6, None);

            // Gate
            let gate_en = if is_deck_a { state.gate_a_enabled } else { state.gate_b_enabled };
            let gate_mix = if is_deck_a { state.gate_a_mix } else { state.gate_b_mix };
            Self::fx_mix(ui, "GATE", gate_en, gate_mix, deck_color,
                in_fx_mode && cursor == 7, None);

            // Beat Repeat
            let repeat_en = if is_deck_a { state.beat_repeat_a_enabled } else { state.beat_repeat_b_enabled };
            let repeat_mix = if is_deck_a { state.beat_repeat_a_mix } else { state.beat_repeat_b_mix };
            Self::fx_mix(ui, "REPT", repeat_en, repeat_mix, deck_color,
                in_fx_mode && cursor == 8, None);

            // Ring Mod
            let ringmod_en = if is_deck_a { state.ringmod_a_enabled } else { state.ringmod_b_enabled };
            let ringmod_mix = if is_deck_a { state.ringmod_a_mix } else { state.ringmod_b_mix };
            Self::fx_mix(ui, "RING", ringmod_en, ringmod_mix, deck_color,
                in_fx_mode && cursor == 9, None);

            // Shimmer
            let shimmer_en = if is_deck_a { state.shimmer_a_enabled } else { state.shimmer_b_enabled };
            let shimmer_mix = if is_deck_a { state.shimmer_a_mix } else { state.shimmer_b_mix };
            Self::fx_mix(ui, "SHIM", shimmer_en, shimmer_mix, deck_color,
                in_fx_mode && cursor == 10, None);

            // Wash Out (shows wash amount)
            let washout_en = if is_deck_a { state.washout_a_enabled } else { state.washout_b_enabled };
            let washout_amt = if is_deck_a { state.washout_a_amount } else { state.washout_b_amount };
            Self::fx_mix(ui, "WASH", washout_en, washout_amt, deck_color,
                in_fx_mode && cursor == 11, None);

            // Vinyl (on/off only)
            let vinyl_en = if is_deck_a { state.vinyl_a_enabled } else { state.vinyl_b_enabled };
            Self::fx_bool(ui, "VNYL", vinyl_en, deck_color, in_fx_mode && cursor == 12);

            // Tape Stop (on/off only)
            let tape_en = if is_deck_a { state.tape_stop_a_enabled } else { state.tape_stop_b_enabled };
            Self::fx_bool(ui, "TAPE", tape_en, deck_color, in_fx_mode && cursor == 13);
        });
    }

    /// Effect with level preset (filter)
    fn fx_level(ui: &mut Ui, name: &str, enabled: bool, level: u8, color: egui::Color32, selected: bool, mode: Option<&str>) {
        ui.horizontal(|ui| {
            let text_color = if enabled { color } else { theme::TEXT_DIM };
            let marker = if selected { ">" } else { " " };
            let status = if enabled {
                match mode {
                    Some(m) => format!("{}[{}] {} {}", marker, name, level, m),
                    None => format!("{}[{}] {}", marker, name, level),
                }
            } else {
                format!("{}[{}] OFF", marker, name)
            };
            ui.label(egui::RichText::new(status).color(text_color).monospace());
        });
    }

    /// Effect with dry/wet mix display
    fn fx_mix(ui: &mut Ui, name: &str, enabled: bool, mix: f32, color: egui::Color32, selected: bool, mode: Option<&str>) {
        ui.horizontal(|ui| {
            let text_color = if enabled { color } else { theme::TEXT_DIM };
            let marker = if selected { ">" } else { " " };
            let status = if enabled {
                // Show mix as 5-char bar: ████░
                let filled = ((mix * 5.0).round() as usize).min(5);
                let bar: String = "\u{2588}".repeat(filled) + &"\u{2591}".repeat(5 - filled);
                match mode {
                    Some(m) => format!("{}[{}] {} {}", marker, name, bar, m),
                    None => format!("{}[{}] {}", marker, name, bar),
                }
            } else {
                format!("{}[{}] OFF", marker, name)
            };
            ui.label(egui::RichText::new(status).color(text_color).monospace());
        });
    }

    /// EQ kill switch display
    fn fx_eq(ui: &mut Ui, kill_l: bool, kill_m: bool, kill_h: bool, color: egui::Color32, selected: bool) {
        ui.horizontal(|ui| {
            let any_kill = kill_l || kill_m || kill_h;
            let text_color = if any_kill { color } else { theme::TEXT_DIM };
            let marker = if selected { ">" } else { " " };
            let l = if kill_l { "X" } else { "L" };
            let m = if kill_m { "X" } else { "M" };
            let h = if kill_h { "X" } else { "H" };
            let status = format!("{}[EQ ] {} {} {}", marker, l, m, h);
            ui.label(egui::RichText::new(status).color(text_color).monospace());
        });
    }

    /// Effect with simple on/off (no mix control)
    fn fx_bool(ui: &mut Ui, name: &str, enabled: bool, color: egui::Color32, selected: bool) {
        ui.horizontal(|ui| {
            let text_color = if enabled { color } else { theme::TEXT_DIM };
            let marker = if selected { ">" } else { " " };
            let status = if enabled {
                format!("{}[{}] ON", marker, name)
            } else {
                format!("{}[{}] OFF", marker, name)
            };
            ui.label(egui::RichText::new(status).color(text_color).monospace());
        });
    }
}
