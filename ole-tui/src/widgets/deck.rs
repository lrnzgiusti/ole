//! Deck widget - displays track info, waveform, and controls

use ole_audio::DeckState;
use ole_audio::PlaybackState;
use ole_audio::FilterType;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};
use crate::theme::Theme;

/// Characters for vertical bar rendering (8 levels + empty)
const BAR_CHARS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Widget for displaying a single DJ deck
pub struct DeckWidget<'a> {
    state: &'a DeckState,
    theme: &'a Theme,
    title: &'a str,
    is_focused: bool,
    filter_enabled: bool,
    filter_type: FilterType,
    filter_level: u8,
    delay_enabled: bool,
    delay_level: u8,
    reverb_enabled: bool,
    reverb_level: u8,
    frame_count: u64,  // For animation timing
}

impl<'a> DeckWidget<'a> {
    pub fn new(state: &'a DeckState, theme: &'a Theme, title: &'a str) -> Self {
        Self {
            state,
            theme,
            title,
            is_focused: false,
            filter_enabled: false,
            filter_type: FilterType::LowPass,
            filter_level: 0,
            delay_enabled: false,
            delay_level: 0,
            reverb_enabled: false,
            reverb_level: 0,
            frame_count: 0,
        }
    }

    pub fn frame_count(mut self, count: u64) -> Self {
        self.frame_count = count;
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.is_focused = focused;
        self
    }

    pub fn filter(mut self, enabled: bool, filter_type: FilterType, level: u8) -> Self {
        self.filter_enabled = enabled;
        self.filter_type = filter_type;
        self.filter_level = level;
        self
    }

    pub fn delay(mut self, enabled: bool, level: u8) -> Self {
        self.delay_enabled = enabled;
        self.delay_level = level;
        self
    }

    pub fn reverb(mut self, enabled: bool, level: u8) -> Self {
        self.reverb_enabled = enabled;
        self.reverb_level = level;
        self
    }

    fn format_time(secs: f64) -> String {
        let mins = (secs / 60.0) as u32;
        let secs = secs % 60.0;
        format!("{:02}:{:05.2}", mins, secs)
    }

    fn render_transport(&self) -> Span<'a> {
        let symbol = match self.state.playback {
            PlaybackState::Playing => "▶",
            PlaybackState::Paused => "⏸",
            PlaybackState::Stopped => "⏹",
        };
        Span::styled(
            format!(" {} ", symbol),
            if self.state.playback == PlaybackState::Playing {
                self.theme.highlight()
            } else {
                self.theme.dim()
            },
        )
    }

    fn render_waveform(&self, width: usize) -> Line<'a> {
        use ratatui::style::Modifier;

        // No track loaded - show empty line
        if self.state.duration <= 0.0 || self.state.waveform_overview.is_empty() {
            return Line::from(Span::styled("─".repeat(width), self.theme.dim()));
        }

        let progress = (self.state.position / self.state.duration).clamp(0.0, 1.0);
        let playhead_pos = (progress * width as f64) as usize;
        let waveform = &self.state.waveform_overview;
        let duration = self.state.duration;

        // Calculate cue marker positions (index in waveform display)
        let cue_markers: Vec<(usize, usize)> = self.state.cue_points
            .iter()
            .enumerate()
            .filter_map(|(idx, opt)| {
                opt.map(|pos_secs| {
                    let cue_progress = (pos_secs / duration).clamp(0.0, 1.0);
                    let cue_pos = (cue_progress * width as f64) as usize;
                    (idx, cue_pos.min(width.saturating_sub(1)))
                })
            })
            .collect();

        let mut spans = Vec::with_capacity(width);

        for i in 0..width {
            // Check if this position has a cue marker
            let cue_at_pos = cue_markers.iter().find(|(_, pos)| *pos == i);

            if let Some((cue_num, _)) = cue_at_pos {
                // Draw numbered cue marker (1-4) with bold highlight
                let marker = ['1', '2', '3', '4'][*cue_num];
                let style = self.theme.highlight().add_modifier(Modifier::BOLD);
                spans.push(Span::styled(marker.to_string(), style));
            } else {
                // Normal waveform rendering
                let waveform_idx = (i * waveform.len()) / width.max(1);
                let peak = waveform.get(waveform_idx).copied().unwrap_or(0.0);

                // Map peak (0.0-1.0) to bar character (0-8)
                let char_idx = (peak.clamp(0.0, 1.0) * 8.0) as usize;
                let bar_char = BAR_CHARS[char_idx.min(8)];

                // Choose style based on position relative to playhead
                let style = if i == playhead_pos {
                    // Playhead - use highlight/inverse style
                    self.theme.highlight()
                } else if i < playhead_pos {
                    // Played portion - brighter/accent color
                    Style::from(self.theme.accent)
                } else {
                    // Unplayed portion - dimmer
                    self.theme.dim()
                };

                // For playhead position, use a visible marker
                let ch = if i == playhead_pos { '│' } else { bar_char };
                spans.push(Span::styled(ch.to_string(), style));
            }
        }

        Line::from(spans)
    }

    fn render_meter(value: f32, peak_hold: f32, width: usize, theme: &Theme) -> Vec<Span<'a>> {
        let filled = ((value.clamp(0.0, 2.0) / 2.0) * width as f32) as usize;
        let peak_pos = ((peak_hold.clamp(0.0, 2.0) / 2.0) * width as f32) as usize;
        let style = theme.meter_style(value / 2.0);
        let peak_style = theme.meter_style(peak_hold / 2.0);

        (0..width)
            .map(|i| {
                if i < filled {
                    Span::styled("█", style)
                } else if i == peak_pos && peak_pos > 0 && peak_pos < width {
                    // Peak hold marker
                    Span::styled("│", peak_style)
                } else {
                    Span::styled("░", theme.dim())
                }
            })
            .collect()
    }

    /// Render beat phase indicator as 4-beat visual (●○○○ style) with pulsing
    fn render_beat_phase(&self, phase: f32) -> Vec<Span<'a>> {
        use ratatui::style::Modifier;

        // Convert phase (0.0-1.0) to beat position in a 4-beat bar
        let beat_in_bar = ((phase * 4.0) as usize) % 4;

        // Calculate how close we are to the beat boundary (for pulse effect)
        let beat_phase_in_beat = (phase * 4.0).fract();
        let is_on_beat = !(0.15..=0.85).contains(&beat_phase_in_beat);

        (0..4)
            .map(|i| {
                let ch = if i == beat_in_bar { '●' } else { '○' };
                let style = if i == beat_in_bar {
                    if is_on_beat && i == 0 {
                        // Downbeat pulse - extra bright and bold
                        self.theme.highlight().add_modifier(Modifier::BOLD)
                    } else if is_on_beat {
                        // Other beats near hit - bright
                        self.theme.highlight()
                    } else {
                        // Active beat but not on the hit
                        Style::from(self.theme.accent)
                    }
                } else {
                    self.theme.dim()
                };
                Span::styled(ch.to_string(), style)
            })
            .collect()
    }
}

impl Widget for DeckWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_style = if self.is_focused {
            self.theme.border_active()
        } else {
            self.theme.border()
        };

        // Add focus indicator to title
        let title_text = if self.is_focused {
            format!(" ► {} ◄ ", self.title)
        } else {
            format!("   {}   ", self.title)
        };

        let title_style = if self.is_focused {
            self.theme.highlight()
        } else {
            self.theme.title()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(title_text, title_style));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 5 || inner.width < 20 {
            return;
        }

        // Layout: track name, waveform, time/bpm, gain, effects
        let chunks = Layout::vertical([
            Constraint::Length(1), // Track name + transport
            Constraint::Length(1), // Waveform
            Constraint::Length(1), // Time / BPM / Tempo
            Constraint::Length(1), // Gain meter
            Constraint::Length(1), // Effects
        ])
        .split(inner);

        // Row 1: Track name and transport
        let track_name = self.state.track_name.as_deref().unwrap_or("No track loaded");
        let transport = self.render_transport();
        let name_width = (inner.width as usize).saturating_sub(4);
        let truncated_name: String = track_name.chars().take(name_width).collect();

        let line = Line::from(vec![
            transport,
            Span::styled(truncated_name, self.theme.normal()),
        ]);
        Paragraph::new(line).render(chunks[0], buf);

        // Row 2: Waveform
        let waveform = self.render_waveform(inner.width as usize);
        Paragraph::new(waveform).render(chunks[1], buf);

        // Row 3: Time / Remaining / BPM / Tempo / Beat Phase
        let remaining = (self.state.duration - self.state.position).max(0.0);
        let time_str = format!(
            "{} -{} ",
            Self::format_time(self.state.position),
            Self::format_time(remaining)
        );
        let bpm_str = self.state.bpm
            .map(|b| format!("BPM:{:.1}", b))
            .unwrap_or_else(|| "BPM:---".to_string());
        let tempo_str = format!("×{:.2}", self.state.tempo);

        // Beat phase indicator - only show if we have a beat grid
        let beat_phase_spans: Vec<Span> = if self.state.beat_grid_info.is_some() {
            self.render_beat_phase(self.state.beat_phase)
        } else {
            vec![Span::styled("----", self.theme.dim())]
        };

        let mut line_spans = vec![
            Span::styled(time_str, self.theme.normal()),
            Span::raw(" │ "),
            Span::styled(bpm_str, Style::from(self.theme.accent)),
            Span::raw(" │ "),
            Span::styled(tempo_str, self.theme.normal()),
            Span::raw(" │ "),
        ];
        line_spans.extend(beat_phase_spans);
        let line = Line::from(line_spans);
        Paragraph::new(line).render(chunks[2], buf);

        // Row 4: Gain meter with peak hold and clip indicator
        let meter_width = (inner.width as usize).saturating_sub(12);
        let gain_spans = Self::render_meter(self.state.gain, self.state.peak_hold, meter_width, self.theme);
        let gain_pct = (self.state.gain * 100.0) as u32;
        // Blinking CLIP indicator - blinks every 4 frames (~8Hz at 30fps)
        let clip_indicator = if self.state.is_clipping {
            let visible = (self.frame_count / 4).is_multiple_of(2);
            if visible {
                Span::styled(" ▲CLIP", self.theme.meter_style(1.0)) // Red/warning
            } else {
                Span::styled("      ", self.theme.dim())
            }
        } else {
            Span::styled("      ", self.theme.dim())
        };
        let mut line_spans = vec![Span::styled("GAIN:", self.theme.dim())];
        line_spans.extend(gain_spans);
        line_spans.push(Span::styled(format!("{:3}%", gain_pct), self.theme.normal()));
        line_spans.push(clip_indicator);
        let line = Line::from(line_spans);
        Paragraph::new(line).render(chunks[3], buf);

        // Row 5: Effects - show [FILT:L5] [DELAY:3] [REVERB:2] style
        let filter_style = if self.filter_enabled {
            self.theme.fx_enabled()
        } else {
            self.theme.fx_disabled()
        };
        let delay_style = if self.delay_enabled {
            self.theme.fx_enabled()
        } else {
            self.theme.fx_disabled()
        };
        let reverb_style = if self.reverb_enabled {
            self.theme.fx_enabled()
        } else {
            self.theme.fx_disabled()
        };

        // Filter type shorthand: L=LowPass, B=BandPass, H=HighPass
        let filter_type_char = match self.filter_type {
            FilterType::LowPass => 'L',
            FilterType::BandPass => 'B',
            FilterType::HighPass => 'H',
        };

        let filter_text = if self.filter_enabled && self.filter_level > 0 {
            format!("[FILT:{}{:X}]", filter_type_char, self.filter_level)
        } else {
            "[FILT:--]".to_string()
        };

        let delay_text = if self.delay_enabled && self.delay_level > 0 {
            format!("[DELAY:{}]", self.delay_level)
        } else {
            "[DELAY:-]".to_string()
        };

        let reverb_text = if self.reverb_enabled && self.reverb_level > 0 {
            format!("[VERB:{}]", self.reverb_level)
        } else {
            "[VERB:-]".to_string()
        };

        let line = Line::from(vec![
            Span::styled(filter_text, filter_style),
            Span::raw(" "),
            Span::styled(delay_text, delay_style),
            Span::raw(" "),
            Span::styled(reverb_text, reverb_style),
        ]);
        Paragraph::new(line).render(chunks[4], buf);
    }
}
