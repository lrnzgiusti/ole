use egui::{Color32, Rect, Sense, Ui, Vec2};

use ole_analysis::{FrequencyBand, PhraseType};
use crate::state::GuiState;
use crate::theme;

/// Neon colors for hot cue markers (1-8)
const CUE_COLORS: [Color32; 8] = [
    Color32::from_rgb(0xff, 0x00, 0x40), // 1: Hot pink
    Color32::from_rgb(0xff, 0x80, 0x00), // 2: Orange
    Color32::from_rgb(0xff, 0xff, 0x00), // 3: Yellow
    Color32::from_rgb(0x00, 0xff, 0x80), // 4: Green
    Color32::from_rgb(0x00, 0xcc, 0xff), // 5: Cyan
    Color32::from_rgb(0x80, 0x40, 0xff), // 6: Purple
    Color32::from_rgb(0xff, 0x00, 0xff), // 7: Magenta
    Color32::from_rgb(0xff, 0xff, 0xff), // 8: White
];

/// Draw the main scrolling waveform with center playhead
pub fn draw_waveform(ui: &mut Ui, state: &GuiState, is_deck_a: bool) -> Option<f64> {
    let deck = if is_deck_a { &state.deck_a } else { &state.deck_b };
    let zoom = if is_deck_a { state.zoom_a } else { state.zoom_b };
    let deck_color = theme::CyberTheme::deck_color(is_deck_a);
    let beat_pulse = if is_deck_a { state.beat_pulse_a } else { state.beat_pulse_b };

    let desired_size = Vec2::new(ui.available_width(), 60.0);
    let (response, painter) = ui.allocate_painter(desired_size, Sense::click());
    let rect = response.rect;

    // Background
    painter.rect_filled(rect, 0.0, theme::BG);

    let waveform = &deck.enhanced_waveform;
    if waveform.points.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "NO TRACK",
            egui::FontId::monospace(11.0),
            theme::TEXT_DIM,
        );
        return None;
    }

    let total_len = waveform.points.len();
    let viewport = zoom.viewport_fraction();
    let position_frac = if deck.duration > 0.0 {
        deck.position / deck.duration
    } else {
        0.0
    };

    // Scrolling waveform: playhead is at the center, waveform scrolls past
    let half_view = viewport / 2.0;
    let view_start = (position_frac - half_view).max(0.0);
    let view_end = (view_start + viewport).min(1.0);
    let view_start = (view_end - viewport).max(0.0);

    let start_idx = (view_start * total_len as f64) as usize;
    let end_idx = ((view_end * total_len as f64) as usize).min(total_len);
    let visible_len = end_idx.saturating_sub(start_idx).max(1);

    let width = rect.width();
    let height = rect.height();
    let center_y = rect.center().y;

    // Draw loop region highlight (before waveform for background effect)
    if let (Some(loop_in), Some(loop_out)) = (deck.loop_in, deck.loop_out) {
        if deck.duration > 0.0 {
            let li_frac = loop_in / deck.duration;
            let lo_frac = loop_out / deck.duration;
            let li_x = rect.left() + ((li_frac - view_start) / viewport * width as f64) as f32;
            let lo_x = rect.left() + ((lo_frac - view_start) / viewport * width as f64) as f32;
            let li_x = li_x.clamp(rect.left(), rect.right());
            let lo_x = lo_x.clamp(rect.left(), rect.right());

            if lo_x > li_x {
                let loop_color = if deck.loop_active {
                    Color32::from_rgba_premultiplied(0, 255, 128, 25)
                } else {
                    Color32::from_rgba_premultiplied(128, 128, 128, 15)
                };
                painter.rect_filled(
                    Rect::from_min_max(
                        egui::pos2(li_x, rect.top()),
                        egui::pos2(lo_x, rect.bottom()),
                    ),
                    0.0,
                    loop_color,
                );
                // Loop boundary lines
                let line_color = if deck.loop_active {
                    Color32::from_rgb(0, 255, 128)
                } else {
                    theme::DIM
                };
                painter.line_segment(
                    [egui::pos2(li_x, rect.top()), egui::pos2(li_x, rect.bottom())],
                    egui::Stroke::new(1.0, line_color),
                );
                painter.line_segment(
                    [egui::pos2(lo_x, rect.top()), egui::pos2(lo_x, rect.bottom())],
                    egui::Stroke::new(1.0, line_color),
                );
            }
        }
    }

    // Draw waveform points with frequency-based coloring
    let step = visible_len as f32 / width;
    let mut x = rect.left();
    let mut i = start_idx as f32;
    while x < rect.right() && (i as usize) < end_idx {
        let idx = (i as usize).min(total_len.saturating_sub(1));
        let point = &waveform.points[idx];

        let color = match point.band {
            FrequencyBand::Bass => theme::ACCENT_PINK,
            FrequencyBand::Mid => theme::PRIMARY,
            FrequencyBand::High => theme::ACCENT_CYAN,
        };

        // Played vs future brightness
        let pos_frac = (i as f64 - start_idx as f64) / visible_len as f64;
        let is_future = pos_frac > ((position_frac - view_start) / viewport);

        let alpha: u8 = if is_future { 80 } else { 200 };
        let c = Color32::from_rgba_premultiplied(
            (color.r() as u32 * alpha as u32 / 255) as u8,
            (color.g() as u32 * alpha as u32 / 255) as u8,
            (color.b() as u32 * alpha as u32 / 255) as u8,
            alpha,
        );

        let bar_h = point.amplitude * height * 0.4;
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(x, center_y - bar_h),
                egui::pos2(x + 1.0, center_y + bar_h),
            ),
            0.0,
            c,
        );

        x += 1.0;
        i += step.max(0.001);
    }

    // Draw beat markers with bar emphasis
    if let Some(ref grid) = deck.beat_grid_info {
        if grid.has_grid && grid.bpm > 0.0 && deck.duration > 0.0 {
            let beat_dur = 60.0 / grid.bpm as f64;
            let mut beat_time = grid.first_beat_offset_secs;
            let mut beat_num = 0u32;
            while beat_time < deck.duration {
                let frac = beat_time / deck.duration;
                if frac >= view_start && frac <= view_end {
                    let bx = rect.left()
                        + ((frac - view_start) / viewport * rect.width() as f64) as f32;
                    let is_bar = beat_num.is_multiple_of(4);
                    let tick_h = if is_bar { 8.0 } else { 4.0 };
                    let tick_color = if is_bar {
                        Color32::from_rgba_premultiplied(255, 255, 255, 60)
                    } else {
                        theme::DIM
                    };
                    painter.line_segment(
                        [egui::pos2(bx, rect.top()), egui::pos2(bx, rect.top() + tick_h)],
                        egui::Stroke::new(1.0, tick_color),
                    );
                }
                beat_time += beat_dur;
                beat_num += 1;
            }
        }
    }

    // Draw hot cue markers with neon colors
    for (ci, cue) in deck.cue_points.iter().enumerate() {
        if let Some(cue_pos) = cue {
            if deck.duration > 0.0 {
                let frac = cue_pos / deck.duration;
                if frac >= view_start && frac <= view_end {
                    let cx = rect.left()
                        + ((frac - view_start) / viewport * rect.width() as f64) as f32;
                    let cue_color = CUE_COLORS[ci % CUE_COLORS.len()];
                    // Draw cue triangle at top
                    let tri_size = 6.0;
                    painter.add(egui::Shape::convex_polygon(
                        vec![
                            egui::pos2(cx, rect.top()),
                            egui::pos2(cx - tri_size / 2.0, rect.top() + tri_size),
                            egui::pos2(cx + tri_size / 2.0, rect.top() + tri_size),
                        ],
                        cue_color,
                        egui::Stroke::NONE,
                    ));
                    // Draw line down
                    painter.line_segment(
                        [egui::pos2(cx, rect.top() + tri_size), egui::pos2(cx, rect.bottom())],
                        egui::Stroke::new(1.0, Color32::from_rgba_premultiplied(
                            cue_color.r(), cue_color.g(), cue_color.b(), 80,
                        )),
                    );
                    // Cue number label
                    painter.text(
                        egui::pos2(cx + 2.0, rect.top() + tri_size + 1.0),
                        egui::Align2::LEFT_TOP,
                        format!("{}", ci + 1),
                        egui::FontId::monospace(8.0),
                        cue_color,
                    );
                }
            }
        }
    }

    // Draw slip mode ghost playhead
    if deck.slip_enabled {
        if let Some(slip_pos) = deck.slip_position {
            if deck.duration > 0.0 {
                let slip_frac = slip_pos / deck.duration;
                let slip_x = rect.left()
                    + ((slip_frac - view_start) / viewport * rect.width() as f64) as f32;
                if slip_x >= rect.left() && slip_x <= rect.right() {
                    // Ghost playhead: dashed white line
                    let dash_len = 4.0;
                    let mut y = rect.top();
                    let mut draw = true;
                    while y < rect.bottom() {
                        if draw {
                            let y_end = (y + dash_len).min(rect.bottom());
                            painter.line_segment(
                                [egui::pos2(slip_x, y), egui::pos2(slip_x, y_end)],
                                egui::Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 100)),
                            );
                        }
                        y += dash_len;
                        draw = !draw;
                    }
                }
            }
        }
    }

    // Draw center playhead with beat pulse glow
    let playhead_x = rect.left()
        + ((position_frac - view_start) / viewport * rect.width() as f64) as f32;
    if playhead_x >= rect.left() && playhead_x <= rect.right() {
        // Glow effect on beat pulse
        if beat_pulse > 0.01 {
            let glow_width = 4.0 + beat_pulse * 6.0;
            let glow_alpha = (beat_pulse * 80.0) as u8;
            painter.rect_filled(
                Rect::from_min_max(
                    egui::pos2(playhead_x - glow_width / 2.0, rect.top()),
                    egui::pos2(playhead_x + glow_width / 2.0, rect.bottom()),
                ),
                0.0,
                Color32::from_rgba_premultiplied(
                    deck_color.r(), deck_color.g(), deck_color.b(), glow_alpha,
                ),
            );
        }
        painter.line_segment(
            [
                egui::pos2(playhead_x, rect.top()),
                egui::pos2(playhead_x, rect.bottom()),
            ],
            egui::Stroke::new(2.0, deck_color),
        );
    }

    // Status indicators at bottom-right
    let mut indicator_x = rect.right() - 4.0;
    let indicator_y = rect.bottom() - 10.0;
    let font = egui::FontId::monospace(8.0);
    if deck.loop_active {
        let txt = "LOOP";
        indicator_x -= 28.0;
        painter.text(
            egui::pos2(indicator_x, indicator_y),
            egui::Align2::LEFT_TOP,
            txt,
            font.clone(),
            Color32::from_rgb(0, 255, 128),
        );
    }
    if deck.quantize_enabled {
        indicator_x -= 8.0;
        painter.text(
            egui::pos2(indicator_x, indicator_y),
            egui::Align2::LEFT_TOP,
            "Q",
            font.clone(),
            Color32::from_rgb(255, 200, 0),
        );
    }
    if deck.key_lock {
        indicator_x -= 16.0;
        painter.text(
            egui::pos2(indicator_x, indicator_y),
            egui::Align2::LEFT_TOP,
            "KL",
            font.clone(),
            Color32::from_rgb(100, 200, 255),
        );
    }
    if deck.slip_enabled {
        indicator_x -= 20.0;
        painter.text(
            egui::pos2(indicator_x, indicator_y),
            egui::Align2::LEFT_TOP,
            "SLP",
            font,
            Color32::from_rgb(255, 100, 255),
        );
    }

    // Click to seek
    if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let click_frac = ((pos.x - rect.left()) / rect.width()) as f64;
            let seek_frac = view_start + click_frac * viewport;
            return Some(seek_frac.clamp(0.0, 1.0));
        }
    }

    None
}

/// Draw mini overview waveform (full track at a glance)
pub fn draw_overview(ui: &mut Ui, state: &GuiState, is_deck_a: bool) -> Option<f64> {
    let deck = if is_deck_a { &state.deck_a } else { &state.deck_b };
    let zoom = if is_deck_a { state.zoom_a } else { state.zoom_b };
    let deck_color = theme::CyberTheme::deck_color(is_deck_a);

    let desired_size = Vec2::new(ui.available_width(), 16.0);
    let (response, painter) = ui.allocate_painter(desired_size, Sense::click());
    let rect = response.rect;

    painter.rect_filled(rect, 0.0, Color32::from_rgb(15, 15, 20));

    let overview = &deck.waveform_overview;
    if overview.is_empty() {
        return None;
    }

    let width = rect.width() as usize;
    let height = rect.height();
    let center_y = rect.center().y;

    // Draw mini waveform
    let step = overview.len() as f32 / width as f32;
    for px in 0..width {
        let idx = ((px as f32 * step) as usize).min(overview.len().saturating_sub(1));
        let amp = overview[idx];
        let bar_h = amp * height * 0.4;
        let x = rect.left() + px as f32;
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(x, center_y - bar_h),
                egui::pos2(x + 1.0, center_y + bar_h),
            ),
            0.0,
            Color32::from_rgba_premultiplied(
                deck_color.r(), deck_color.g(), deck_color.b(), 60,
            ),
        );
    }

    // Draw phrase markers on overview
    if deck.duration > 0.0 && !deck.phrase_markers.is_empty() {
        // Draw energy curve as subtle line in top quarter
        if !deck.energy_curve.is_empty() {
            let curve = &deck.energy_curve;
            let curve_step = curve.len() as f32 / width as f32;
            let top_quarter = rect.top() + height * 0.25;
            for px in 0..width {
                let ci = ((px as f32 * curve_step) as usize).min(curve.len().saturating_sub(1));
                let e = curve[ci];
                let y = top_quarter - e * height * 0.2;
                let x = rect.left() + px as f32;
                painter.rect_filled(
                    Rect::from_min_max(egui::pos2(x, y), egui::pos2(x + 1.0, y + 1.0)),
                    0.0,
                    Color32::from_rgba_premultiplied(255, 255, 255, 30),
                );
            }
        }

        // Draw phrase boundary markers
        for marker in deck.phrase_markers.iter() {
            let frac = marker.position_secs / deck.duration;
            let mx = rect.left() + (frac * rect.width() as f64) as f32;
            if mx < rect.left() || mx > rect.right() {
                continue;
            }

            let (marker_color, label) = match marker.phrase_type {
                PhraseType::Intro => (Color32::from_rgb(0, 200, 200), "IN"),
                PhraseType::Build => (Color32::from_rgb(255, 200, 0), "BLD"),
                PhraseType::Drop => (Color32::from_rgb(255, 60, 100), "DRP"),
                PhraseType::Break => (Color32::from_rgb(160, 80, 255), "BRK"),
                PhraseType::Outro => (Color32::from_rgb(80, 120, 180), "OUT"),
            };

            // Vertical line
            painter.line_segment(
                [egui::pos2(mx, rect.top()), egui::pos2(mx, rect.bottom())],
                egui::Stroke::new(1.0, Color32::from_rgba_premultiplied(
                    marker_color.r(), marker_color.g(), marker_color.b(), 120,
                )),
            );

            // Label
            painter.text(
                egui::pos2(mx + 2.0, rect.top() + 1.0),
                egui::Align2::LEFT_TOP,
                label,
                egui::FontId::monospace(7.0),
                marker_color,
            );
        }
    }

    // Draw loop region on overview
    if let (Some(loop_in), Some(loop_out)) = (deck.loop_in, deck.loop_out) {
        if deck.duration > 0.0 {
            let li_x = rect.left() + (loop_in / deck.duration * rect.width() as f64) as f32;
            let lo_x = rect.left() + (loop_out / deck.duration * rect.width() as f64) as f32;
            let li_x = li_x.clamp(rect.left(), rect.right());
            let lo_x = lo_x.clamp(rect.left(), rect.right());
            if lo_x > li_x {
                let c = if deck.loop_active {
                    Color32::from_rgba_premultiplied(0, 255, 128, 40)
                } else {
                    Color32::from_rgba_premultiplied(128, 128, 128, 20)
                };
                painter.rect_filled(
                    Rect::from_min_max(egui::pos2(li_x, rect.top()), egui::pos2(lo_x, rect.bottom())),
                    0.0, c,
                );
            }
        }
    }

    // Draw viewport indicator
    let viewport = zoom.viewport_fraction();
    let position_frac = if deck.duration > 0.0 { deck.position / deck.duration } else { 0.0 };
    let half_view = viewport / 2.0;
    let vs = (position_frac - half_view).max(0.0);
    let ve = (vs + viewport).min(1.0);
    let vx_start = rect.left() + (vs * rect.width() as f64) as f32;
    let vx_end = rect.left() + (ve * rect.width() as f64) as f32;
    painter.rect_stroke(
        Rect::from_min_max(egui::pos2(vx_start, rect.top()), egui::pos2(vx_end, rect.bottom())),
        0.0,
        egui::Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 80)),
    );

    // Draw position indicator (glowing line)
    let pos_x = rect.left() + (position_frac * rect.width() as f64) as f32;
    if pos_x >= rect.left() && pos_x <= rect.right() {
        painter.line_segment(
            [egui::pos2(pos_x, rect.top()), egui::pos2(pos_x, rect.bottom())],
            egui::Stroke::new(1.5, deck_color),
        );
    }

    // Click to seek on overview
    if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let seek_frac = ((pos.x - rect.left()) / rect.width()) as f64;
            return Some(seek_frac.clamp(0.0, 1.0));
        }
    }

    None
}
