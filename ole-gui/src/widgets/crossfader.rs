use egui::{Rect, Sense, Ui, Vec2};

use crate::theme;

pub fn draw_crossfader(ui: &mut Ui, value: &mut f32) -> bool {
    let desired_size = Vec2::new(ui.available_width().min(200.0), 20.0);
    let (response, painter) = ui.allocate_painter(desired_size, Sense::drag());
    let rect = response.rect;

    let mut changed = false;

    // Track
    let track_rect = Rect::from_min_max(
        egui::pos2(rect.left() + 10.0, rect.center().y - 2.0),
        egui::pos2(rect.right() - 10.0, rect.center().y + 2.0),
    );
    painter.rect_filled(track_rect, 2.0, theme::DIM);

    // Center mark
    let center_x = track_rect.center().x;
    painter.line_segment(
        [
            egui::pos2(center_x, track_rect.top() - 2.0),
            egui::pos2(center_x, track_rect.bottom() + 2.0),
        ],
        egui::Stroke::new(1.0, theme::TEXT_DIM),
    );

    // Drag interaction
    if response.dragged() {
        let track_width = track_rect.width();
        let delta = response.drag_delta().x / track_width * 2.0;
        *value = (*value + delta).clamp(-1.0, 1.0);
        changed = true;
    }

    // Position indicator
    let normalized = (*value + 1.0) / 2.0; // -1..1 -> 0..1
    let pos_x = track_rect.left() + normalized * track_rect.width();
    let handle_rect = Rect::from_center_size(
        egui::pos2(pos_x, rect.center().y),
        Vec2::new(8.0, 16.0),
    );
    let handle_color = if response.dragged() { theme::PRIMARY } else { theme::TEXT };
    painter.rect_filled(handle_rect, 2.0, handle_color);

    // A/B labels
    painter.text(
        egui::pos2(rect.left() + 2.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        "A",
        egui::FontId::monospace(10.0),
        theme::DECK_A,
    );
    painter.text(
        egui::pos2(rect.right() - 2.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        "B",
        egui::FontId::monospace(10.0),
        theme::DECK_B,
    );

    changed
}
