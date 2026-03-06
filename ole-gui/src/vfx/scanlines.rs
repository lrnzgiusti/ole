use egui::{Color32, Context, LayerId, Order, Rect};

pub fn draw_scanlines(ctx: &Context, intensity: u8) {
    if intensity == 0 {
        return;
    }

    let screen = ctx.screen_rect();
    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, egui::Id::new("scanlines")));

    // Scale spacing by pixels-per-point for correct HiDPI rendering
    let ppp = ctx.pixels_per_point();
    let spacing = (3.0 * ppp).max(2.0);
    let line_h = (1.0 * ppp).max(1.0);

    // Intensity affects alpha: subtle=15, medium=25, heavy=40
    let alpha = match intensity {
        1 => 15,
        2 => 25,
        _ => 40,
    };

    let mut y = screen.top();
    while y < screen.bottom() {
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(screen.left(), y),
                egui::pos2(screen.right(), y + line_h),
            ),
            0.0,
            Color32::from_rgba_unmultiplied(0, 0, 0, alpha),
        );
        y += spacing;
    }
}

/// Draw CRT screen curvature vignette (darkened edges)
pub fn draw_vignette(ctx: &Context, intensity: u8) {
    if intensity == 0 {
        return;
    }

    let screen = ctx.screen_rect();
    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, egui::Id::new("vignette")));

    let alpha_base = match intensity {
        1 => 20u8,
        2 => 40,
        _ => 60,
    };

    // Draw gradient borders (top, bottom, left, right)
    let edge_width = screen.width() * 0.08;
    let edge_height = screen.height() * 0.08;

    // Top edge
    for i in 0..10 {
        let frac = i as f32 / 10.0;
        let a = (alpha_base as f32 * (1.0 - frac)) as u8;
        let y = screen.top() + frac * edge_height;
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(screen.left(), y),
                egui::pos2(screen.right(), y + edge_height / 10.0),
            ),
            0.0,
            Color32::from_rgba_unmultiplied(0, 0, 0, a),
        );
    }

    // Bottom edge
    for i in 0..10 {
        let frac = i as f32 / 10.0;
        let a = (alpha_base as f32 * frac) as u8;
        let y = screen.bottom() - edge_height + frac * edge_height;
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(screen.left(), y),
                egui::pos2(screen.right(), y + edge_height / 10.0),
            ),
            0.0,
            Color32::from_rgba_unmultiplied(0, 0, 0, a),
        );
    }

    // Left edge
    for i in 0..10 {
        let frac = i as f32 / 10.0;
        let a = (alpha_base as f32 * (1.0 - frac)) as u8;
        let x = screen.left() + frac * edge_width;
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(x, screen.top()),
                egui::pos2(x + edge_width / 10.0, screen.bottom()),
            ),
            0.0,
            Color32::from_rgba_unmultiplied(0, 0, 0, a),
        );
    }

    // Right edge
    for i in 0..10 {
        let frac = i as f32 / 10.0;
        let a = (alpha_base as f32 * frac) as u8;
        let x = screen.right() - edge_width + frac * edge_width;
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(x, screen.top()),
                egui::pos2(x + edge_width / 10.0, screen.bottom()),
            ),
            0.0,
            Color32::from_rgba_unmultiplied(0, 0, 0, a),
        );
    }
}
