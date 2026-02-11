use egui::{Color32, FontFamily, FontId, Stroke, TextStyle, Visuals};

pub struct CyberTheme;

// Color palette
pub const BG: Color32 = Color32::from_rgb(0x0a, 0x0a, 0x0a);
pub const BG_PANEL: Color32 = Color32::from_rgb(0x12, 0x12, 0x12);
pub const PRIMARY: Color32 = Color32::from_rgb(0x00, 0xff, 0x41);
pub const ACCENT_CYAN: Color32 = Color32::from_rgb(0x00, 0xff, 0xcc);
pub const ACCENT_PINK: Color32 = Color32::from_rgb(0xff, 0x00, 0x66);
pub const _ACCENT_BLUE: Color32 = Color32::from_rgb(0x00, 0x66, 0xff);
pub const TEXT: Color32 = Color32::from_rgb(0xb4, 0xff, 0xb4);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x50, 0x80, 0x50);
pub const DIM: Color32 = Color32::from_rgb(0x28, 0x28, 0x28);
pub const WARNING: Color32 = Color32::from_rgb(0xff, 0xff, 0x00);
pub const DANGER: Color32 = Color32::from_rgb(0xff, 0x32, 0x32);
pub const DECK_A: Color32 = PRIMARY;
pub const DECK_B: Color32 = ACCENT_CYAN;

impl CyberTheme {
    pub fn apply(ctx: &egui::Context) {
        // Load bundled monospace font
        let fonts = egui::FontDefinitions::default();

        // Use bundled JetBrains Mono if available, otherwise fall back to system monospace
        // Use system monospace fonts for now
        // (JetBrains Mono can be bundled later via include_bytes!)

        ctx.set_fonts(fonts);

        // Configure text styles - all monospace
        let mut style = (*ctx.style()).clone();
        style.text_styles = [
            (TextStyle::Small, FontId::new(10.0, FontFamily::Monospace)),
            (TextStyle::Body, FontId::new(13.0, FontFamily::Monospace)),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
            (TextStyle::Button, FontId::new(13.0, FontFamily::Monospace)),
            (TextStyle::Heading, FontId::new(18.0, FontFamily::Monospace)),
        ]
        .into();

        // Widget spacing
        style.spacing.item_spacing = egui::vec2(4.0, 2.0);
        style.spacing.window_margin = egui::Margin::same(4.0);
        style.spacing.button_padding = egui::vec2(4.0, 2.0);

        // Set dark visuals
        let mut visuals = Visuals::dark();
        visuals.panel_fill = BG;
        visuals.window_fill = BG_PANEL;
        visuals.extreme_bg_color = BG;
        visuals.faint_bg_color = DIM;

        // Widget visuals
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        visuals.widgets.noninteractive.bg_fill = BG_PANEL;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, DIM);

        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        visuals.widgets.inactive.bg_fill = BG_PANEL;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, DIM);

        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, PRIMARY);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(0x15, 0x25, 0x15);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, PRIMARY);

        visuals.widgets.active.fg_stroke = Stroke::new(1.0, PRIMARY);
        visuals.widgets.active.bg_fill = Color32::from_rgb(0x0a, 0x30, 0x0a);
        visuals.widgets.active.bg_stroke = Stroke::new(1.5, PRIMARY);

        visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(0x00, 0xff, 0x41, 0x30);
        visuals.selection.stroke = Stroke::new(1.0, PRIMARY);

        visuals.window_stroke = Stroke::new(1.0, DIM);
        visuals.window_shadow = egui::epaint::Shadow::NONE;

        style.visuals = visuals;
        ctx.set_style(style);
    }

    pub fn deck_color(deck_a: bool) -> Color32 {
        if deck_a { DECK_A } else { DECK_B }
    }

    pub fn meter_color(level: f32) -> Color32 {
        if level > 0.9 {
            DANGER
        } else if level > 0.75 {
            WARNING
        } else {
            PRIMARY
        }
    }

    pub fn spectrum_color(band: usize, total: usize) -> Color32 {
        if total == 0 {
            return PRIMARY;
        }
        let ratio = band as f32 / total as f32;
        if ratio < 0.33 {
            ACCENT_PINK
        } else if ratio < 0.66 {
            PRIMARY
        } else {
            ACCENT_CYAN
        }
    }
}
