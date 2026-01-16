//! Enhanced waveform widget with zoom, frequency coloring, and overview+detail views

use crate::app::WaveformZoom;
use crate::theme::Theme;
use ole_analysis::{EnhancedWaveform, FrequencyBand};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use std::sync::Arc;

/// Characters for vertical bar rendering (8 levels + empty)
const BAR_CHARS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Enhanced waveform widget with zoom and frequency coloring
pub struct EnhancedWaveformWidget<'a> {
    theme: &'a Theme,
    waveform: &'a Arc<EnhancedWaveform>,
    position: f64, // Current position (0.0 - 1.0)
    duration: f64, // Track duration in seconds
    zoom: WaveformZoom,
    beat_markers: Vec<usize>, // Positions of beat markers (in display width units)
    cue_markers: Vec<(usize, usize)>, // (cue_number, position)
}

impl<'a> EnhancedWaveformWidget<'a> {
    pub fn new(theme: &'a Theme, waveform: &'a Arc<EnhancedWaveform>) -> Self {
        Self {
            theme,
            waveform,
            position: 0.0,
            duration: 0.0,
            zoom: WaveformZoom::default(),
            beat_markers: Vec::new(),
            cue_markers: Vec::new(),
        }
    }

    pub fn position(mut self, position: f64, duration: f64) -> Self {
        self.position = position;
        self.duration = duration;
        self
    }

    pub fn zoom(mut self, zoom: WaveformZoom) -> Self {
        self.zoom = zoom;
        self
    }

    pub fn beat_markers(mut self, markers: Vec<usize>) -> Self {
        self.beat_markers = markers;
        self
    }

    pub fn cue_markers(mut self, markers: Vec<(usize, usize)>) -> Self {
        self.cue_markers = markers;
        self
    }

    /// Get the color for a frequency band
    fn band_color(&self, band: FrequencyBand, is_played: bool) -> Style {
        let base_style = match band {
            FrequencyBand::Bass => Style::default().fg(self.theme.deck_a), // Warm color for bass
            FrequencyBand::Mid => Style::default().fg(self.theme.accent),  // Accent for mids
            FrequencyBand::High => Style::default().fg(self.theme.deck_b), // Cool color for highs
        };

        if is_played {
            base_style
        } else {
            // Dim unplayed portion
            self.theme.dim()
        }
    }

    /// Calculate viewport start/end based on zoom level and position
    fn viewport(&self, _width: usize) -> (f64, f64) {
        let viewport_size = self.zoom.viewport_fraction();
        let progress = if self.duration > 0.0 {
            self.position / self.duration
        } else {
            0.0
        };

        // Center viewport on playhead
        let half = viewport_size / 2.0;
        let start = (progress - half).clamp(0.0, 1.0 - viewport_size);
        let end = (start + viewport_size).min(1.0);

        (start, end)
    }

    /// Render a single waveform row with frequency coloring
    fn render_detail_row(&self, width: usize, start: f64, end: f64) -> Line<'a> {
        use ratatui::style::Modifier;

        if self.waveform.is_empty() || width == 0 {
            return Line::from(Span::styled("─".repeat(width), self.theme.dim()));
        }

        let progress = if self.duration > 0.0 {
            self.position / self.duration
        } else {
            0.0
        };
        let viewport_range = end - start;

        // Convert playhead to position within viewport
        let playhead_in_viewport = if viewport_range > 0.0 {
            ((progress - start) / viewport_range).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let playhead_pos = (playhead_in_viewport * width as f64) as usize;

        let mut spans = Vec::with_capacity(width);

        for i in 0..width {
            // Calculate position in track (0.0 - 1.0)
            let track_pos = start + (i as f64 / width as f64) * viewport_range;

            // Check for cue marker at this position
            let cue_at_pos = self.cue_markers.iter().find(|(_, pos)| {
                let cue_viewport_pos = if self.duration > 0.0 {
                    let cue_progress = *pos as f64 / width as f64; // Assume passed as display positions
                    ((cue_progress - start) / viewport_range * width as f64) as usize
                } else {
                    0
                };
                cue_viewport_pos == i
            });

            if let Some((cue_num, _)) = cue_at_pos {
                // Draw numbered cue marker
                let marker = ['1', '2', '3', '4'][*cue_num];
                let style = self.theme.highlight().add_modifier(Modifier::BOLD);
                spans.push(Span::styled(marker.to_string(), style));
            } else if i == playhead_pos {
                // Playhead
                spans.push(Span::styled("│", self.theme.highlight()));
            } else {
                // Get waveform point at this position
                let point = self
                    .waveform
                    .points
                    .get((track_pos * self.waveform.points.len() as f64) as usize);

                if let Some(point) = point {
                    let char_idx = (point.amplitude.clamp(0.0, 1.0) * 8.0) as usize;
                    let bar_char = BAR_CHARS[char_idx.min(8)];

                    let is_played = i < playhead_pos;
                    let style = self.band_color(point.band, is_played);

                    spans.push(Span::styled(bar_char.to_string(), style));
                } else {
                    spans.push(Span::styled(" ", self.theme.dim()));
                }
            }
        }

        Line::from(spans)
    }

    /// Render the overview bar showing full track with viewport indicator
    fn render_overview(&self, width: usize) -> Line<'a> {
        if self.waveform.is_empty() || width == 0 {
            return Line::from(Span::styled("─".repeat(width), self.theme.dim()));
        }

        let progress = if self.duration > 0.0 {
            self.position / self.duration
        } else {
            0.0
        };
        let playhead_pos = (progress * width as f64) as usize;

        let (viewport_start, viewport_end) = self.viewport(width);
        let viewport_start_pos = (viewport_start * width as f64) as usize;
        let viewport_end_pos = (viewport_end * width as f64) as usize;

        let mut spans = Vec::with_capacity(width);

        for i in 0..width {
            let track_pos = i as f64 / width as f64;
            let is_viewport_edge =
                i == viewport_start_pos || (i > 0 && i == viewport_end_pos.saturating_sub(1));
            let is_in_viewport = i >= viewport_start_pos && i < viewport_end_pos;

            if i == playhead_pos {
                // Playhead always visible
                spans.push(Span::styled("│", self.theme.highlight()));
            } else if is_viewport_edge && self.zoom != WaveformZoom::X1 {
                // Viewport bracket indicators (only show when zoomed)
                let bracket = if i == viewport_start_pos { "[" } else { "]" };
                spans.push(Span::styled(bracket, self.theme.highlight()));
            } else {
                // Waveform bar (simplified - just amplitude, no frequency coloring in overview)
                let point = self
                    .waveform
                    .points
                    .get((track_pos * self.waveform.points.len() as f64) as usize);

                if let Some(point) = point {
                    // Use half-height characters for compact overview
                    let char_idx = (point.amplitude.clamp(0.0, 1.0) * 4.0) as usize;
                    let compact_chars = [' ', '▁', '▂', '▃', '▄'];
                    let bar_char = compact_chars[char_idx.min(4)];

                    let style = if i < playhead_pos {
                        Style::default().fg(self.theme.accent)
                    } else if is_in_viewport {
                        self.theme.normal()
                    } else {
                        self.theme.dim()
                    };

                    spans.push(Span::styled(bar_char.to_string(), style));
                } else {
                    spans.push(Span::styled(" ", self.theme.dim()));
                }
            }
        }

        Line::from(spans)
    }
}

impl Widget for EnhancedWaveformWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let width = area.width as usize;
        let (viewport_start, viewport_end) = self.viewport(width);

        if area.height == 1 {
            // Single row: just detail view
            let line = self.render_detail_row(width, viewport_start, viewport_end);
            Paragraph::new(line).render(area, buf);
        } else if area.height == 2 {
            // Two rows: overview + detail
            let overview = self.render_overview(width);
            let detail = self.render_detail_row(width, viewport_start, viewport_end);

            let overview_area = Rect { height: 1, ..area };
            let detail_area = Rect {
                y: area.y + 1,
                height: 1,
                ..area
            };

            Paragraph::new(overview).render(overview_area, buf);
            Paragraph::new(detail).render(detail_area, buf);
        } else {
            // Three+ rows: overview + 2 detail rows
            let overview = self.render_overview(width);
            let detail1 = self.render_detail_row(width, viewport_start, viewport_end);
            let detail2 = self.render_detail_row(width, viewport_start, viewport_end);

            let overview_area = Rect { height: 1, ..area };
            let detail1_area = Rect {
                y: area.y + 1,
                height: 1,
                ..area
            };
            let detail2_area = Rect {
                y: area.y + 2,
                height: 1,
                ..area
            };

            Paragraph::new(overview).render(overview_area, buf);
            Paragraph::new(detail1).render(detail1_area, buf);
            Paragraph::new(detail2).render(detail2_area, buf);
        }
    }
}
