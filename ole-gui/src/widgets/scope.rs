use egui::{Color32, Rect, Ui, Vec2};

use crate::state::{GuiState, ScopeMode, SPECTRUM_BANDS, WATERFALL_DEPTH};
use crate::theme;

pub struct ScopeWidget;

impl ScopeWidget {
    pub fn show(ui: &mut Ui, state: &GuiState) {
        let desired_size = Vec2::new(ui.available_width(), 80.0);
        let (response, painter) = ui.allocate_painter(desired_size, egui::Sense::hover());
        let rect = response.rect;

        // Background
        painter.rect_filled(rect, 0.0, theme::BG);

        match state.scope_mode {
            ScopeMode::TimeDomain => Self::draw_time_domain(&painter, rect, state),
            ScopeMode::Lissajous => Self::draw_lissajous(&painter, rect, state),
            ScopeMode::StereoField => Self::draw_stereo_field(&painter, rect, state),
            ScopeMode::Waterfall => Self::draw_waterfall(&painter, rect, state),
        }
    }

    fn draw_time_domain(painter: &egui::Painter, rect: Rect, state: &GuiState) {
        let center_y = rect.center().y;
        let half_height = rect.height() * 0.4;

        Self::draw_scope_line(
            painter,
            rect,
            state.deck_a.scope_samples.as_slice(),
            center_y,
            half_height,
            theme::DECK_A,
        );

        Self::draw_scope_line(
            painter,
            rect,
            state.deck_b.scope_samples.as_slice(),
            center_y,
            half_height,
            theme::DECK_B,
        );

        painter.line_segment(
            [
                egui::pos2(rect.left(), center_y),
                egui::pos2(rect.right(), center_y),
            ],
            egui::Stroke::new(0.5, theme::DIM),
        );

        painter.text(
            egui::pos2(rect.left() + 4.0, rect.top() + 2.0),
            egui::Align2::LEFT_TOP,
            "SCOPE",
            egui::FontId::monospace(10.0),
            theme::TEXT_DIM,
        );
    }

    fn draw_scope_line(
        painter: &egui::Painter,
        rect: Rect,
        samples: &[f32],
        center_y: f32,
        half_height: f32,
        color: Color32,
    ) {
        if samples.len() < 4 {
            return;
        }

        let step = samples.len() as f32 / 2.0 / rect.width();
        let mut points = Vec::new();
        let mut i = 0.0;
        let mut x = rect.left();
        while x < rect.right() && (i as usize * 2) < samples.len() {
            let idx = (i as usize * 2).min(samples.len() - 2);
            let sample = samples[idx];
            let y = center_y - sample * half_height;
            points.push(egui::pos2(x, y));
            x += 1.0;
            i += step;
        }

        if points.len() >= 2 {
            let stroke = egui::Stroke::new(1.0, color);
            for window in points.windows(2) {
                painter.line_segment([window[0], window[1]], stroke);
            }
        }
    }

    fn draw_lissajous(painter: &egui::Painter, rect: Rect, state: &GuiState) {
        let center = rect.center();
        let scale = rect.height().min(rect.width()) * 0.35;

        let decks: &[(&[f32], Color32)] = &[
            (
                state.deck_a.scope_samples.as_slice(),
                Color32::from_rgba_unmultiplied(0x00, 0xff, 0x41, 0x60),
            ),
            (
                state.deck_b.scope_samples.as_slice(),
                Color32::from_rgba_unmultiplied(0x00, 0xff, 0xcc, 0x40),
            ),
        ];

        for &(samples, color) in decks {
            if samples.len() < 4 {
                continue;
            }
            for i in (0..samples.len().saturating_sub(1)).step_by(2) {
                let l = samples[i];
                let r = samples[i + 1];
                let x = center.x + l * scale;
                let y = center.y - r * scale;
                painter.circle_filled(egui::pos2(x, y), 0.5, color);
            }
        }

        painter.text(
            egui::pos2(rect.left() + 4.0, rect.top() + 2.0),
            egui::Align2::LEFT_TOP,
            "LISSAJOUS",
            egui::FontId::monospace(10.0),
            theme::TEXT_DIM,
        );
    }

    /// Stereo butterfly display: L channel goes up, R channel goes down
    /// Shows stereo width and imaging in real-time
    fn draw_stereo_field(painter: &egui::Painter, rect: Rect, state: &GuiState) {
        let center_y = rect.center().y;
        let quarter_height = rect.height() * 0.45;

        // Divider line
        painter.line_segment(
            [
                egui::pos2(rect.left(), center_y),
                egui::pos2(rect.right(), center_y),
            ],
            egui::Stroke::new(0.5, theme::DIM),
        );

        // Draw both decks as butterfly (L up, R down)
        let decks: &[(&[f32], Color32, Color32)] = &[
            (
                state.deck_a.scope_samples.as_slice(),
                Color32::from_rgba_unmultiplied(0x00, 0xff, 0x41, 0xC0), // L green
                Color32::from_rgba_unmultiplied(0xff, 0x00, 0x66, 0xA0), // R pink
            ),
            (
                state.deck_b.scope_samples.as_slice(),
                Color32::from_rgba_unmultiplied(0x00, 0xff, 0xcc, 0x80), // L cyan
                Color32::from_rgba_unmultiplied(0x00, 0x66, 0xff, 0x60), // R blue
            ),
        ];

        for &(samples, color_l, color_r) in decks {
            if samples.len() < 4 {
                continue;
            }

            let step = samples.len() as f32 / 2.0 / rect.width();
            let mut points_l = Vec::new();
            let mut points_r = Vec::new();
            let mut i = 0.0;
            let mut x = rect.left();

            while x < rect.right() && (i as usize * 2 + 1) < samples.len() {
                let idx = i as usize * 2;
                let l = samples[idx.min(samples.len() - 2)];
                let r = samples[(idx + 1).min(samples.len() - 1)];

                // L goes UP from center
                points_l.push(egui::pos2(x, center_y - l.abs() * quarter_height));
                // R goes DOWN from center
                points_r.push(egui::pos2(x, center_y + r.abs() * quarter_height));

                x += 1.0;
                i += step;
            }

            // Draw filled area for L channel (center to top)
            for window in points_l.windows(2) {
                painter.line_segment([window[0], window[1]], egui::Stroke::new(1.0, color_l));
            }

            // Draw filled area for R channel (center to bottom)
            for window in points_r.windows(2) {
                painter.line_segment([window[0], window[1]], egui::Stroke::new(1.0, color_r));
            }

            // Draw stereo width indicator: difference between L and R as a filled bar
            if !points_l.is_empty() && !points_r.is_empty() {
                let step_vis = (rect.width() / 4.0) as usize;
                for chunk_start in (0..points_l.len()).step_by(step_vis.max(1)) {
                    let chunk_end = (chunk_start + step_vis).min(points_l.len());
                    if chunk_end <= chunk_start {
                        continue;
                    }

                    // Calculate stereo width for this chunk
                    let mut width_sum = 0.0f32;
                    let mut count = 0;
                    let samp_step = samples.len() as f32 / 2.0 / rect.width();
                    for j in chunk_start..chunk_end {
                        let idx = (j as f32 * samp_step) as usize * 2;
                        if idx + 1 < samples.len() {
                            let l = samples[idx];
                            let r = samples[idx + 1];
                            let diff = (l - r).abs();
                            width_sum += diff;
                            count += 1;
                        }
                    }

                    if count > 0 {
                        let avg_width = width_sum / count as f32;
                        let bar_x = rect.left() + chunk_start as f32;
                        let bar_w = (chunk_end - chunk_start) as f32;
                        let bar_h = avg_width * quarter_height * 0.3;

                        // Small stereo width indicator at the center
                        let width_color =
                            Color32::from_rgba_unmultiplied(0xff, 0xff, 0x00, (avg_width * 200.0).min(60.0) as u8);
                        painter.rect_filled(
                            Rect::from_min_max(
                                egui::pos2(bar_x, center_y - bar_h),
                                egui::pos2(bar_x + bar_w, center_y + bar_h),
                            ),
                            0.0,
                            width_color,
                        );
                    }
                }
            }
        }

        // Labels
        painter.text(
            egui::pos2(rect.left() + 4.0, rect.top() + 2.0),
            egui::Align2::LEFT_TOP,
            "STEREO FIELD",
            egui::FontId::monospace(10.0),
            theme::TEXT_DIM,
        );
        painter.text(
            egui::pos2(rect.right() - 4.0, rect.top() + 2.0),
            egui::Align2::RIGHT_TOP,
            "L",
            egui::FontId::monospace(9.0),
            Color32::from_rgba_unmultiplied(0x00, 0xff, 0x41, 0x80),
        );
        painter.text(
            egui::pos2(rect.right() - 4.0, rect.bottom() - 12.0),
            egui::Align2::RIGHT_TOP,
            "R",
            egui::FontId::monospace(9.0),
            Color32::from_rgba_unmultiplied(0xff, 0x00, 0x66, 0x80),
        );
    }

    /// Scrolling spectrogram (waterfall): time on X-axis, frequency on Y-axis
    /// Color-coded by intensity with cyberpunk palette
    fn draw_waterfall(painter: &egui::Painter, rect: Rect, state: &GuiState) {
        let width = rect.width();
        let height = rect.height();

        // How many frames fit in the display width
        let frames_to_show = (width as usize).min(WATERFALL_DEPTH);
        let col_width = width / frames_to_show as f32;
        let row_height = height / SPECTRUM_BANDS as f32;

        for frame_offset in 0..frames_to_show {
            // Read from circular buffer: oldest first (left), newest at right
            let buf_idx = (state.waterfall_idx + WATERFALL_DEPTH - frames_to_show + frame_offset)
                % WATERFALL_DEPTH;

            let x = rect.left() + frame_offset as f32 * col_width;

            for band in 0..SPECTRUM_BANDS {
                // Combine both decks
                let val_a = state.waterfall_a[buf_idx][band];
                let val_b = state.waterfall_b[buf_idx][band];
                let val = (val_a + val_b).min(1.0);

                if val < 0.01 {
                    continue;
                }

                // Map frequency band to Y: low frequencies at bottom
                let y = rect.bottom() - (band as f32 + 1.0) * row_height;

                let color = waterfall_color(val, band, SPECTRUM_BANDS);
                painter.rect_filled(
                    Rect::from_min_max(
                        egui::pos2(x, y),
                        egui::pos2(x + col_width + 0.5, y + row_height + 0.5),
                    ),
                    0.0,
                    color,
                );
            }
        }

        // Frequency scale labels
        painter.text(
            egui::pos2(rect.left() + 4.0, rect.top() + 2.0),
            egui::Align2::LEFT_TOP,
            "SPECTROGRAM",
            egui::FontId::monospace(10.0),
            theme::TEXT_DIM,
        );
        painter.text(
            egui::pos2(rect.right() - 4.0, rect.bottom() - 12.0),
            egui::Align2::RIGHT_TOP,
            "20Hz",
            egui::FontId::monospace(8.0),
            theme::TEXT_DIM,
        );
        painter.text(
            egui::pos2(rect.right() - 4.0, rect.top() + 2.0),
            egui::Align2::RIGHT_TOP,
            "20kHz",
            egui::FontId::monospace(8.0),
            theme::TEXT_DIM,
        );
    }
}

/// Map waterfall intensity + frequency band to a cyberpunk color
fn waterfall_color(intensity: f32, band: usize, total_bands: usize) -> Color32 {
    let i = intensity.clamp(0.0, 1.0);
    let freq_ratio = band as f32 / total_bands.max(1) as f32;

    // Base hue shifts with frequency: purple (low) → pink (mid) → cyan (high) → white (very loud)
    let (r, g, b) = if freq_ratio < 0.33 {
        // Low frequencies: deep purple → magenta
        let t = freq_ratio / 0.33;
        lerp_rgb((0.3, 0.0, 0.5), (0.8, 0.0, 0.4), t)
    } else if freq_ratio < 0.66 {
        // Mid frequencies: magenta → green
        let t = (freq_ratio - 0.33) / 0.33;
        lerp_rgb((0.8, 0.0, 0.4), (0.0, 1.0, 0.25), t)
    } else {
        // High frequencies: green → cyan
        let t = (freq_ratio - 0.66) / 0.34;
        lerp_rgb((0.0, 1.0, 0.25), (0.0, 1.0, 0.8), t)
    };

    // Intensity brightens the color toward white
    let boost = i * i; // quadratic for more punch at high values
    let r_final = (r * i + boost * 0.3).min(1.0);
    let g_final = (g * i + boost * 0.3).min(1.0);
    let b_final = (b * i + boost * 0.3).min(1.0);
    let alpha = (i * 255.0).min(255.0) as u8;

    Color32::from_rgba_unmultiplied(
        (r_final * 255.0) as u8,
        (g_final * 255.0) as u8,
        (b_final * 255.0) as u8,
        alpha,
    )
}

fn lerp_rgb(a: (f32, f32, f32), b: (f32, f32, f32), t: f32) -> (f32, f32, f32) {
    (
        a.0 + (b.0 - a.0) * t,
        a.1 + (b.1 - a.1) * t,
        a.2 + (b.2 - a.2) * t,
    )
}
