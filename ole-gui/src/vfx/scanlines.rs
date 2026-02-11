use egui::{Color32, Context, LayerId, Order, Rect};

pub fn draw_scanlines(ctx: &Context) {
    let screen = ctx.screen_rect();
    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, egui::Id::new("scanlines")));

    // Scale spacing by pixels-per-point for correct HiDPI rendering
    let ppp = ctx.pixels_per_point();
    let spacing = (3.0 * ppp).max(2.0);
    let line_h = (1.0 * ppp).max(1.0);
    let mut y = screen.top();
    while y < screen.bottom() {
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(screen.left(), y),
                egui::pos2(screen.right(), y + line_h),
            ),
            0.0,
            Color32::from_rgba_unmultiplied(0, 0, 0, 25),
        );
        y += spacing;
    }
}
