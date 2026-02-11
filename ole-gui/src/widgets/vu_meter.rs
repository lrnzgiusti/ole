use egui::{Rect, Ui, Vec2};

use crate::theme;

pub fn draw_vu_meter(ui: &mut Ui, level: f32, peak_hold: f32, is_clipping: bool) {
    let desired_size = Vec2::new(12.0, ui.available_height().min(60.0));
    let (response, painter) = ui.allocate_painter(desired_size, egui::Sense::hover());
    let rect = response.rect;

    // Background
    painter.rect_filled(rect, 2.0, theme::DIM);

    // Fill bar (bottom-up)
    let fill_height = (rect.height() * level.clamp(0.0, 1.0)).min(rect.height());
    let fill_rect = Rect::from_min_max(
        egui::pos2(rect.left() + 1.0, rect.bottom() - fill_height),
        egui::pos2(rect.right() - 1.0, rect.bottom()),
    );
    let fill_color = theme::CyberTheme::meter_color(level);
    painter.rect_filled(fill_rect, 0.0, fill_color);

    // Peak hold marker
    if peak_hold > 0.01 {
        let peak_y = rect.bottom() - (rect.height() * peak_hold.clamp(0.0, 1.0));
        let peak_color = theme::CyberTheme::meter_color(peak_hold);
        painter.line_segment(
            [
                egui::pos2(rect.left() + 1.0, peak_y),
                egui::pos2(rect.right() - 1.0, peak_y),
            ],
            egui::Stroke::new(2.0, peak_color),
        );
    }

    // Clip indicator
    if is_clipping {
        painter.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(rect.width(), 3.0)),
            0.0,
            theme::DANGER,
        );
    }
}
