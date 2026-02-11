use egui::{Color32, Rect, Sense, Ui, Vec2};

use ole_analysis::FrequencyBand;
use crate::state::GuiState;
use crate::theme;

pub fn draw_waveform(ui: &mut Ui, state: &GuiState, is_deck_a: bool) -> Option<f64> {
    let deck = if is_deck_a { &state.deck_a } else { &state.deck_b };
    let zoom = if is_deck_a { state.zoom_a } else { state.zoom_b };
    let deck_color = theme::CyberTheme::deck_color(is_deck_a);

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

    // Calculate viewport bounds
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

        // Determine if this position is past the playhead
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

    // Draw playhead
    let playhead_x = rect.left()
        + ((position_frac - view_start) / viewport * rect.width() as f64) as f32;
    if playhead_x >= rect.left() && playhead_x <= rect.right() {
        painter.line_segment(
            [
                egui::pos2(playhead_x, rect.top()),
                egui::pos2(playhead_x, rect.bottom()),
            ],
            egui::Stroke::new(2.0, deck_color),
        );
    }

    // Draw beat markers
    if let Some(ref grid) = deck.beat_grid_info {
        if grid.has_grid && grid.bpm > 0.0 && deck.duration > 0.0 {
            let beat_dur = 60.0 / grid.bpm as f64;
            let mut beat_time = grid.first_beat_offset_secs;
            while beat_time < deck.duration {
                let frac = beat_time / deck.duration;
                if frac >= view_start && frac <= view_end {
                    let bx = rect.left()
                        + ((frac - view_start) / viewport * rect.width() as f64) as f32;
                    painter.line_segment(
                        [egui::pos2(bx, rect.top()), egui::pos2(bx, rect.top() + 4.0)],
                        egui::Stroke::new(1.0, theme::DIM),
                    );
                }
                beat_time += beat_dur;
            }
        }
    }

    // Draw cue points
    for (ci, cue) in deck.cue_points.iter().enumerate() {
        if let Some(cue_pos) = cue {
            if deck.duration > 0.0 {
                let frac = cue_pos / deck.duration;
                if frac >= view_start && frac <= view_end {
                    let cx = rect.left()
                        + ((frac - view_start) / viewport * rect.width() as f64) as f32;
                    let cue_color = Color32::from_rgb(0xff, 0x80, 0x00);
                    painter.line_segment(
                        [egui::pos2(cx, rect.top()), egui::pos2(cx, rect.bottom())],
                        egui::Stroke::new(1.0, cue_color),
                    );
                    painter.text(
                        egui::pos2(cx + 2.0, rect.top()),
                        egui::Align2::LEFT_TOP,
                        format!("{}", ci + 1),
                        egui::FontId::monospace(9.0),
                        cue_color,
                    );
                }
            }
        }
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
