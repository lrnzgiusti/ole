use egui::Ui;

use crate::theme;

pub fn _draw_tempo_slider(ui: &mut Ui, tempo: f32) {
    let pct = (tempo - 1.0) * 100.0;
    let sign = if pct >= 0.0 { "+" } else { "" };
    ui.label(
        egui::RichText::new(format!("{}{:.1}%", sign, pct))
            .color(theme::TEXT)
            .monospace(),
    );
}
