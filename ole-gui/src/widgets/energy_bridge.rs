use egui::{Color32, Ui, Vec2};

use crate::state::GuiState;
use crate::theme;

pub struct EnergyBridge;

impl EnergyBridge {
    /// Draw an animated energy arc between the two decks
    /// Shows energy flow based on crossfader position, beat sync, and audio levels
    pub fn show(ui: &mut Ui, state: &GuiState) {
        let desired_size = Vec2::new(ui.available_width(), 24.0);
        let (response, painter) = ui.allocate_painter(desired_size, egui::Sense::hover());
        let rect = response.rect;

        let center_y = rect.center().y;
        let crossfader_norm = (state.crossfader + 1.0) / 2.0; // -1..1 → 0..1
        let energy_a = state.deck_a.peak_level.min(1.0);
        let energy_b = state.deck_b.peak_level.min(1.0);
        let total_energy = (energy_a + energy_b) * 0.5;
        let sync = state.sync_quality;
        let frame = state.frame_count;

        // Draw the bridge backbone: a flowing sine wave between A and B
        let segments = 80;
        let bridge_left = rect.left() + 20.0;
        let bridge_right = rect.right() - 20.0;
        let bridge_width = bridge_right - bridge_left;

        if bridge_width < 10.0 {
            return;
        }

        // Deck labels at the edges
        painter.text(
            egui::pos2(rect.left() + 2.0, center_y),
            egui::Align2::LEFT_CENTER,
            "A",
            egui::FontId::monospace(10.0),
            Color32::from_rgba_unmultiplied(
                theme::DECK_A.r(),
                theme::DECK_A.g(),
                theme::DECK_A.b(),
                (energy_a * 200.0 + 55.0).min(255.0) as u8,
            ),
        );
        painter.text(
            egui::pos2(rect.right() - 2.0, center_y),
            egui::Align2::RIGHT_CENTER,
            "B",
            egui::FontId::monospace(10.0),
            Color32::from_rgba_unmultiplied(
                theme::DECK_B.r(),
                theme::DECK_B.g(),
                theme::DECK_B.b(),
                (energy_b * 200.0 + 55.0).min(255.0) as u8,
            ),
        );

        // Draw the bridge backbone as a flowing wave
        let time = frame as f32 * 0.05;
        let wave_amp = rect.height() * 0.2 * (0.5 + total_energy * 0.5);

        for seg in 0..segments {
            let t0 = seg as f32 / segments as f32;
            let t1 = (seg + 1) as f32 / segments as f32;

            let x0 = bridge_left + t0 * bridge_width;
            let x1 = bridge_left + t1 * bridge_width;

            // Sine wave with traveling motion
            let y0 = center_y + (t0 * 6.0 + time).sin() * wave_amp;
            let y1 = center_y + (t1 * 6.0 + time).sin() * wave_amp;

            // Color blends from Deck A (green) to Deck B (cyan) across the bridge
            let color = blend_deck_colors(t0, crossfader_norm, total_energy, sync);

            // Width varies with energy and sync
            let width = 1.0 + total_energy * 1.5 + sync * 0.5;

            painter.line_segment(
                [egui::pos2(x0, y0), egui::pos2(x1, y1)],
                egui::Stroke::new(width, color),
            );
        }

        // Crossfader position indicator: a bright point on the bridge
        let cf_x = bridge_left + crossfader_norm * bridge_width;
        let cf_y = center_y + (crossfader_norm * 6.0 + time).sin() * wave_amp;
        let cf_pulse = 2.0 + (state.beat_pulse_a.max(state.beat_pulse_b)) * 4.0;

        // Glow around crossfader point
        let glow_color = Color32::from_rgba_unmultiplied(0xff, 0xff, 0xff, 30);
        painter.circle_filled(egui::pos2(cf_x, cf_y), cf_pulse * 2.0, glow_color);
        painter.circle_filled(
            egui::pos2(cf_x, cf_y),
            cf_pulse,
            Color32::from_rgba_unmultiplied(0xff, 0xff, 0xff, 180),
        );

        // Draw energy particles
        for particle in &state.energy_particles {
            let px = bridge_left + particle.pos * bridge_width;
            let py = center_y
                + (particle.pos * 6.0 + time).sin() * wave_amp
                + particle.wave_offset * rect.height() * 0.3;

            if px < bridge_left || px > bridge_right {
                continue;
            }

            let alpha = (particle.brightness * 255.0).min(255.0) as u8;

            // Particle color depends on position (A side = green, B side = cyan)
            let pr = if particle.pos < 0.5 {
                let t = particle.pos * 2.0;
                lerp_u8(theme::DECK_A.r(), theme::ACCENT_PINK.r(), t)
            } else {
                let t = (particle.pos - 0.5) * 2.0;
                lerp_u8(theme::ACCENT_PINK.r(), theme::DECK_B.r(), t)
            };
            let pg = if particle.pos < 0.5 {
                let t = particle.pos * 2.0;
                lerp_u8(theme::DECK_A.g(), theme::ACCENT_PINK.g(), t)
            } else {
                let t = (particle.pos - 0.5) * 2.0;
                lerp_u8(theme::ACCENT_PINK.g(), theme::DECK_B.g(), t)
            };
            let pb = if particle.pos < 0.5 {
                let t = particle.pos * 2.0;
                lerp_u8(theme::DECK_A.b(), theme::ACCENT_PINK.b(), t)
            } else {
                let t = (particle.pos - 0.5) * 2.0;
                lerp_u8(theme::ACCENT_PINK.b(), theme::DECK_B.b(), t)
            };

            let color = Color32::from_rgba_unmultiplied(pr, pg, pb, alpha);

            // Particle glow
            if particle.brightness > 0.5 {
                let glow_alpha = ((particle.brightness - 0.5) * 60.0).min(30.0) as u8;
                let glow = Color32::from_rgba_unmultiplied(pr, pg, pb, glow_alpha);
                painter.circle_filled(egui::pos2(px, py), particle.size * 3.0, glow);
            }

            painter.circle_filled(egui::pos2(px, py), particle.size, color);
        }

        // Sync quality indicator at center
        if sync > 0.1 {
            let sync_text = format!("{:.0}%", sync * 100.0);
            let sync_color = if sync > 0.9 {
                theme::PRIMARY
            } else if sync > 0.5 {
                theme::WARNING
            } else {
                theme::DANGER
            };
            painter.text(
                egui::pos2(rect.center().x, rect.top() + 1.0),
                egui::Align2::CENTER_TOP,
                sync_text,
                egui::FontId::monospace(8.0),
                sync_color,
            );
        }
    }
}

/// Blend deck colors across the bridge based on crossfader position
fn blend_deck_colors(t: f32, crossfader: f32, energy: f32, sync: f32) -> Color32 {
    // Base gradient from A to B
    let r_a = theme::DECK_A.r() as f32;
    let g_a = theme::DECK_A.g() as f32;
    let b_a = theme::DECK_A.b() as f32;
    let r_b = theme::DECK_B.r() as f32;
    let g_b = theme::DECK_B.g() as f32;
    let b_b = theme::DECK_B.b() as f32;

    let r = r_a + (r_b - r_a) * t;
    let g = g_a + (g_b - g_a) * t;
    let b = b_a + (b_b - b_a) * t;

    // Brightness based on energy and proximity to crossfader
    let dist_to_cf = (t - crossfader).abs();
    let proximity_boost = (1.0 - dist_to_cf * 2.0).max(0.0);
    let base_alpha = 20.0 + energy * 80.0 + proximity_boost * 100.0 + sync * 30.0;
    let alpha = base_alpha.min(255.0) as u8;

    Color32::from_rgba_unmultiplied(r as u8, g as u8, b as u8, alpha)
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    (a as f32 + (b as f32 - a as f32) * t) as u8
}
