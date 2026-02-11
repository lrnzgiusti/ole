use egui::{Color32, Pos2, Ui, Vec2};

use crate::theme;

pub fn knob(
    ui: &mut Ui,
    _id: impl std::hash::Hash,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    label: &str,
    color: Color32,
) -> bool {
    let size = Vec2::new(32.0, 46.0); // extra height for label below knob
    let (response, painter) = ui.allocate_painter(size, egui::Sense::drag());
    let rect = response.rect;
    let knob_center_y = rect.top() + 16.0; // knob in upper portion
    let center = Pos2::new(rect.center().x, knob_center_y);
    let radius = 12.0;

    let mut changed = false;

    // Drag to change value (vertical drag)
    if response.dragged() {
        let delta = -response.drag_delta().y * 0.005;
        let range_size = range.end() - range.start();
        *value = (*value + delta * range_size).clamp(*range.start(), *range.end());
        changed = true;
    }

    // Background circle
    painter.circle_filled(center, radius, theme::DIM);
    painter.circle_stroke(center, radius, egui::Stroke::new(1.0, theme::TEXT_DIM));

    // Value arc
    let range_size = range.end() - range.start();
    let normalized = if range_size.abs() < f32::EPSILON {
        0.5
    } else {
        (*value - range.start()) / range_size
    };
    let start_angle = std::f32::consts::PI * 0.75; // 225 degrees
    let end_angle = start_angle + normalized * std::f32::consts::PI * 1.5; // 270 degree sweep

    let arc_radius = radius - 2.0;
    let steps = 20;
    let arc_color = if response.dragged() {
        Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), 255)
    } else {
        color
    };

    for i in 0..steps {
        let t0 = i as f32 / steps as f32;
        let t1 = (i + 1) as f32 / steps as f32;
        let a0 = start_angle + t0 * (end_angle - start_angle);
        let a1 = start_angle + t1 * (end_angle - start_angle);

        if a0 > end_angle { break; }

        let p0 = center + Vec2::new(a0.cos(), a0.sin()) * arc_radius;
        let p1 = center + Vec2::new(a1.cos(), a1.sin()) * arc_radius;
        painter.line_segment([p0, p1], egui::Stroke::new(2.0, arc_color));
    }

    // Indicator dot
    let indicator_pos = center + Vec2::new(end_angle.cos(), end_angle.sin()) * (radius - 5.0);
    painter.circle_filled(indicator_pos, 2.0, color);

    // Label below knob circle
    painter.text(
        Pos2::new(center.x, knob_center_y + radius + 4.0),
        egui::Align2::CENTER_TOP,
        label,
        egui::FontId::monospace(9.0),
        theme::TEXT_DIM,
    );

    changed
}
