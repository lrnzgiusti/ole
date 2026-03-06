use egui::{Color32, Context, LayerId, Order, Rect};

/// Draw phosphor glow effect - bright elements bleed into surrounding pixels
pub fn draw_glow(ctx: &Context, beat_pulse_a: f32, beat_pulse_b: f32) {
    let screen = ctx.screen_rect();
    let painter = ctx.layer_painter(LayerId::new(Order::Background, egui::Id::new("glow")));

    let total_pulse = (beat_pulse_a + beat_pulse_b) * 0.5;
    if total_pulse < 0.01 {
        return;
    }

    // Beat-reactive ambient glow from each deck area
    let half_w = screen.width() / 2.0;

    // Deck A glow (left side, warm) — subtle
    if beat_pulse_a > 0.05 {
        let alpha = (beat_pulse_a * 6.0) as u8;
        let glow_rect = Rect::from_min_max(
            screen.left_top(),
            egui::pos2(screen.left() + half_w, screen.bottom()),
        );
        painter.rect_filled(
            glow_rect,
            0.0,
            Color32::from_rgba_unmultiplied(0, 180, 80, alpha),
        );
    }

    // Deck B glow (right side, cool) — subtle
    if beat_pulse_b > 0.05 {
        let alpha = (beat_pulse_b * 6.0) as u8;
        let glow_rect = Rect::from_min_max(
            egui::pos2(screen.left() + half_w, screen.top()),
            screen.right_bottom(),
        );
        painter.rect_filled(
            glow_rect,
            0.0,
            Color32::from_rgba_unmultiplied(0, 130, 180, alpha),
        );
    }
}

/// Draw VHS-style noise overlay
pub fn draw_noise(ctx: &Context, frame: u64, intensity: u8) {
    if intensity == 0 {
        return;
    }

    let screen = ctx.screen_rect();
    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, egui::Id::new("noise")));

    let alpha = match intensity {
        1 => 8u8,
        2 => 15,
        _ => 25,
    };

    // Simple pseudo-random noise lines (non-allocating)
    // Use frame count as seed for simple hash
    let seed = frame.wrapping_mul(2654435761);
    let line_count = match intensity {
        1 => 3,
        2 => 6,
        _ => 12,
    };

    for i in 0..line_count {
        let hash = seed.wrapping_add(i as u64).wrapping_mul(1103515245).wrapping_add(12345);
        let y = screen.top() + (hash % (screen.height() as u64).max(1)) as f32;
        let x_start = screen.left() + ((hash >> 8) % (screen.width() as u64 / 2).max(1)) as f32;
        let width = 20.0 + ((hash >> 16) % 100) as f32;

        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(x_start, y),
                egui::pos2((x_start + width).min(screen.right()), y + 1.0),
            ),
            0.0,
            Color32::from_rgba_unmultiplied(255, 255, 255, alpha),
        );
    }
}

/// Draw chromatic aberration (RGB offset on bright elements)
/// Since we can't offset individual pixels in egui, we simulate with colored edge bands
pub fn draw_chromatic(ctx: &Context, intensity: u8) {
    if intensity == 0 {
        return;
    }

    let screen = ctx.screen_rect();
    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, egui::Id::new("chromatic")));

    let offset = match intensity {
        1 => 1.0f32,
        2 => 2.0,
        _ => 3.0,
    };
    let alpha = match intensity {
        1 => 10u8,
        2 => 20,
        _ => 30,
    };

    // Red channel offset (left edge)
    painter.rect_filled(
        Rect::from_min_max(
            screen.left_top(),
            egui::pos2(screen.left() + offset, screen.bottom()),
        ),
        0.0,
        Color32::from_rgba_unmultiplied(255, 0, 0, alpha),
    );

    // Blue channel offset (right edge)
    painter.rect_filled(
        Rect::from_min_max(
            egui::pos2(screen.right() - offset, screen.top()),
            screen.right_bottom(),
        ),
        0.0,
        Color32::from_rgba_unmultiplied(0, 0, 255, alpha),
    );
}

/// Draw glitch effect (random rectangles on beat/track load)
pub fn draw_glitch(ctx: &Context, intensity: f32, frame: u64) {
    if intensity < 0.01 {
        return;
    }

    let screen = ctx.screen_rect();
    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, egui::Id::new("glitch")));

    let rect_count = (intensity * 8.0) as u32;
    let alpha = (intensity * 100.0) as u8;

    for i in 0..rect_count {
        let seed = frame.wrapping_mul(2654435761).wrapping_add(i as u64 * 7919);
        let y = screen.top() + (seed % (screen.height() as u64).max(1)) as f32;
        let h = 2.0 + ((seed >> 8) % 10) as f32;
        let x_offset = ((seed >> 16) % 20) as f32 - 10.0;

        // Horizontal slice displacement
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(screen.left() + x_offset, y),
                egui::pos2(screen.right() + x_offset, (y + h).min(screen.bottom())),
            ),
            0.0,
            Color32::from_rgba_unmultiplied(0, 255, 200, alpha / 3),
        );
    }
}

/// Draw drop flash (subtle white flash on energy spikes)
pub fn draw_drop_flash(ctx: &Context, intensity: f32) {
    if intensity < 0.01 {
        return;
    }
    let screen = ctx.screen_rect();
    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, egui::Id::new("drop_flash")));
    let alpha = (intensity * 35.0) as u8;
    painter.rect_filled(
        screen,
        0.0,
        Color32::from_rgba_unmultiplied(160, 200, 180, alpha),
    );
}

/// Draw effect activation flash (subtle tinted flash)
pub fn draw_fx_flash(ctx: &Context, intensity: f32) {
    if intensity < 0.01 {
        return;
    }
    let screen = ctx.screen_rect();
    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, egui::Id::new("fx_flash")));
    let alpha = (intensity * 18.0) as u8;
    painter.rect_filled(
        screen,
        0.0,
        Color32::from_rgba_unmultiplied(0, 180, 150, alpha),
    );
}

/// Draw audio-reactive background grid
pub fn draw_background_grid(ctx: &Context, bass_level: f32, frame: u64) {
    let screen = ctx.screen_rect();
    let painter = ctx.layer_painter(LayerId::new(Order::Background, egui::Id::new("bg_grid")));

    let base_alpha = 8u8;
    let pulse_alpha = (bass_level * 15.0).min(20.0) as u8;
    let alpha = base_alpha + pulse_alpha;
    let color = Color32::from_rgba_unmultiplied(0, 255, 120, alpha);

    // Animated offset for subtle drift
    let offset_y = (frame as f32 * 0.3) % 40.0;
    let spacing = 40.0;

    // Horizontal lines
    let mut y = screen.top() - spacing + offset_y;
    while y < screen.bottom() + spacing {
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(screen.left(), y),
                egui::pos2(screen.right(), y + 0.5),
            ),
            0.0,
            color,
        );
        y += spacing;
    }

    // Vertical lines
    let mut x = screen.left();
    while x < screen.right() {
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(x, screen.top()),
                egui::pos2(x + 0.5, screen.bottom()),
            ),
            0.0,
            color,
        );
        x += spacing;
    }
}
