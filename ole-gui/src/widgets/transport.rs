use egui::Ui;

use ole_audio::PlaybackState;
use crate::theme;

pub fn draw_transport(ui: &mut Ui, playback: PlaybackState, position: f64, duration: f64) {
    ui.horizontal(|ui| {
        let play_text = match playback {
            PlaybackState::Playing => "[||]",
            PlaybackState::Paused => "[>]",
            PlaybackState::Stopped => "[>]",
        };
        let play_color = match playback {
            PlaybackState::Playing => theme::PRIMARY,
            _ => theme::TEXT_DIM,
        };
        ui.label(egui::RichText::new(play_text).color(play_color).monospace());

        // Time display
        let pos_m = (position / 60.0) as u32;
        let pos_s = (position % 60.0) as u32;
        let dur_m = (duration / 60.0) as u32;
        let dur_s = (duration % 60.0) as u32;
        ui.label(
            egui::RichText::new(format!("{}:{:02}/{}:{:02}", pos_m, pos_s, dur_m, dur_s))
                .color(theme::TEXT)
                .monospace(),
        );

        // Beat phase dots
        // (will be enhanced in Phase 2)
    });
}
