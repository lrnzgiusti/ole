//! Deck widget - displays track info, waveform, and controls

use ole_analysis::FrequencyBand;
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
use crate::app::WaveformZoom;
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
    sync_quality: f32, // Sync quality (0.0-1.0) for steady border glow when phase-locked
    /// UI-side peak hold for CRT VU meter effect (None = use audio-side peak_hold)
    crt_peak_hold: Option<f32>,
    /// Waveform zoom level
    zoom: WaveformZoom,
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
            sync_quality: 0.0,
            crt_peak_hold: None,
            zoom: WaveformZoom::default(),
        }
    }

    pub fn zoom(mut self, zoom: WaveformZoom) -> Self {
        self.zoom = zoom;
        self
    }

    /// Set UI-side peak hold for CRT VU meter effect
    pub fn crt_peak_hold(mut self, peak: f32) -> Self {
        self.crt_peak_hold = Some(peak);
        self
    }

    pub fn sync_quality(mut self, quality: f32) -> Self {
        self.sync_quality = quality;
        self
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

    /// Get the color for a frequency band
    fn band_style(&self, band: FrequencyBand, is_played: bool) -> Style {
        if is_played {
            match band {
                FrequencyBand::Bass => Style::default().fg(self.theme.deck_a), // Warm color for bass
                FrequencyBand::Mid => Style::default().fg(self.theme.accent),   // Accent for mids
                FrequencyBand::High => Style::default().fg(self.theme.deck_b),  // Cool color for highs
            }
        } else {
            self.theme.dim()
        }
    }

    /// Calculate viewport based on zoom level
    fn viewport(&self, progress: f64) -> (f64, f64) {
        let viewport_size = self.zoom.viewport_fraction();
        let half = viewport_size / 2.0;
        let start = (progress - half).clamp(0.0, 1.0 - viewport_size);
        let end = (start + viewport_size).min(1.0);
        (start, end)
    }

    fn render_waveform(&self, width: usize) -> Line<'a> {
        use ratatui::style::Modifier;

        // No track loaded - show empty line
        if self.state.duration <= 0.0 || self.state.waveform_overview.is_empty() {
            return Line::from(Span::styled("─".repeat(width), self.theme.dim()));
        }

        let progress = (self.state.position / self.state.duration).clamp(0.0, 1.0);
        let duration = self.state.duration;

        // Calculate viewport based on zoom level
        let (viewport_start, viewport_end) = self.viewport(progress);
        let viewport_range = viewport_end - viewport_start;

        // Calculate playhead position within viewport
        let playhead_in_viewport = if viewport_range > 0.0 {
            ((progress - viewport_start) / viewport_range).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let playhead_pos = (playhead_in_viewport * width as f64) as usize;

        let waveform = &self.state.waveform_overview;
        let enhanced = &self.state.enhanced_waveform;
        let use_enhanced = !enhanced.is_empty();

        // Calculate cue marker positions (index in waveform display)
        let cue_markers: Vec<(usize, usize)> = self.state.cue_points
            .iter()
            .enumerate()
            .filter_map(|(idx, opt)| {
                opt.map(|pos_secs| {
                    let cue_progress = (pos_secs / duration).clamp(0.0, 1.0);
                    // Map cue to viewport
                    if cue_progress >= viewport_start && cue_progress <= viewport_end {
                        let cue_in_viewport = (cue_progress - viewport_start) / viewport_range;
                        let cue_pos = (cue_in_viewport * width as f64) as usize;
                        Some((idx, cue_pos.min(width.saturating_sub(1))))
                    } else {
                        None
                    }
                })
            })
            .flatten()
            .collect();

        // Calculate beat marker positions from beat grid (within viewport)
        // Returns (position, is_downbeat) where downbeat = beat 1 of a 4-beat bar
        let beat_markers: Vec<(usize, bool)> = self.state.beat_grid_info
            .as_ref()
            .filter(|g| g.has_grid && g.bpm > 0.0)
            .map(|grid| {
                let seconds_per_beat = 60.0 / grid.bpm as f64;
                let first_beat = grid.first_beat_offset_secs;

                let mut beats = Vec::new();

                // Handle beats before first_beat (count backwards)
                if first_beat > seconds_per_beat {
                    let mut pre_beat = first_beat - seconds_per_beat;
                    let mut beat_num: i32 = -1; // Beat before first_beat
                    while pre_beat >= 0.0 {
                        let beat_progress = (pre_beat / duration).clamp(0.0, 1.0);
                        if beat_progress >= viewport_start && beat_progress <= viewport_end {
                            let beat_in_viewport = (beat_progress - viewport_start) / viewport_range;
                            let beat_pos = (beat_in_viewport * width as f64) as usize;
                            if beat_pos < width {
                                // Downbeat every 4 beats (0, 4, 8, ...)
                                let is_downbeat = beat_num.rem_euclid(4) == 0;
                                beats.push((beat_pos, is_downbeat));
                            }
                        }
                        pre_beat -= seconds_per_beat;
                        beat_num -= 1;
                    }
                }

                // Calculate beats from first_beat onwards
                let mut beat_time = first_beat;
                let mut beat_num: i32 = 0;
                while beat_time < duration {
                    let beat_progress = (beat_time / duration).clamp(0.0, 1.0);
                    if beat_progress >= viewport_start && beat_progress <= viewport_end {
                        let beat_in_viewport = (beat_progress - viewport_start) / viewport_range;
                        let beat_pos = (beat_in_viewport * width as f64) as usize;
                        if beat_pos < width {
                            // Downbeat every 4 beats (0, 4, 8, ...)
                            let is_downbeat = beat_num % 4 == 0;
                            beats.push((beat_pos, is_downbeat));
                        }
                    }
                    beat_time += seconds_per_beat;
                    beat_num += 1;
                }
                beats
            })
            .unwrap_or_default();

        let mut spans = Vec::with_capacity(width);

        for i in 0..width {
            // Calculate track position for this display position
            let track_progress = viewport_start + (i as f64 / width as f64) * viewport_range;

            // Check if this position has a cue marker (highest priority)
            let cue_at_pos = cue_markers.iter().find(|(_, pos)| *pos == i);

            if let Some((cue_num, _)) = cue_at_pos {
                // Draw numbered cue marker (1-8) with bold highlight
                let marker = ['1', '2', '3', '4', '5', '6', '7', '8'][*cue_num];
                let style = self.theme.highlight().add_modifier(Modifier::BOLD);
                spans.push(Span::styled(marker.to_string(), style));
            } else if i == playhead_pos {
                // Playhead position - always visible
                spans.push(Span::styled("│", self.theme.highlight()));
            } else if let Some((_, is_downbeat)) = beat_markers.iter().find(|(pos, _)| *pos == i) {
                // Beat marker - show volume bar with highlight color to indicate beat
                let (peak, _band) = if use_enhanced {
                    let idx = (track_progress * enhanced.len() as f64) as usize;
                    let point = enhanced.points.get(idx);
                    (
                        point.map(|p| p.amplitude).unwrap_or(0.0),
                        point.map(|p| p.band).unwrap_or(FrequencyBand::Mid)
                    )
                } else {
                    let idx = (track_progress * waveform.len() as f64) as usize;
                    (waveform.get(idx).copied().unwrap_or(0.0), FrequencyBand::Mid)
                };

                let char_idx = (peak.clamp(0.0, 1.0) * 8.0) as usize;
                let bar_char = BAR_CHARS[char_idx.min(8)];

                // Downbeats (beat 1 of bar) get bold highlight, regular beats get normal highlight
                let style = if *is_downbeat {
                    self.theme.highlight().add_modifier(Modifier::BOLD)
                } else {
                    self.theme.highlight()
                };
                spans.push(Span::styled(bar_char.to_string(), style));
            } else {
                // Normal waveform rendering with frequency coloring
                let (peak, band) = if use_enhanced {
                    let idx = (track_progress * enhanced.len() as f64) as usize;
                    let point = enhanced.points.get(idx);
                    (
                        point.map(|p| p.amplitude).unwrap_or(0.0),
                        point.map(|p| p.band).unwrap_or(FrequencyBand::Mid)
                    )
                } else {
                    let idx = (track_progress * waveform.len() as f64) as usize;
                    (waveform.get(idx).copied().unwrap_or(0.0), FrequencyBand::Mid)
                };

                let char_idx = (peak.clamp(0.0, 1.0) * 8.0) as usize;
                let bar_char = BAR_CHARS[char_idx.min(8)];

                let is_played = i < playhead_pos;
                let style = self.band_style(band, is_played);

                spans.push(Span::styled(bar_char.to_string(), style));
            }
        }

        Line::from(spans)
    }

    /// Render LED ladder-style meter with linear scale
    /// value/peak_hold: 0.0-2.0 where 1.0 = unity gain (100%), 2.0 = 200%
    fn render_meter(value: f32, peak_hold: f32, width: usize, theme: &Theme) -> Vec<Span<'a>> {
        // Linear scaling: gain range 0.0-2.0 maps to bar 0%-100%
        // This matches the percentage display (gain * 100%)
        let value_normalized = (value / 2.0).clamp(0.0, 1.0);
        let peak_normalized = (peak_hold / 2.0).clamp(0.0, 1.0);

        let filled = (value_normalized * width as f32) as usize;
        let peak_pos = (peak_normalized * width as f32) as usize;

        // Threshold positions for color zones (linear scale)
        // Yellow zone: 75% of bar = gain 1.5 (150%)
        // Red zone: 90% of bar = gain 1.8 (180%)
        let yellow_threshold = (width as f32 * 0.75) as usize;
        let red_threshold = (width as f32 * 0.90) as usize;

        (0..width)
            .map(|i| {
                // Determine color based on position in meter
                let segment_style = if i >= red_threshold {
                    // Red zone (+0dB to +6dB)
                    Style::default().fg(theme.danger)
                } else if i >= yellow_threshold {
                    // Yellow zone (-6dB to 0dB)
                    Style::default().fg(theme.warning)
                } else {
                    // Green zone (below -6dB)
                    Style::default().fg(theme.accent)
                };

                if i < filled {
                    // Lit LED segment
                    Span::styled("█", segment_style)
                } else if i == peak_pos && peak_pos > 0 && peak_pos < width {
                    // Peak hold marker - use the color for that position
                    Span::styled("│", segment_style)
                } else {
                    // Unlit LED segment (dim)
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
        use ratatui::style::Modifier;

        // Border style based on sync quality (steady glow when phase-locked)
        // >95% = green glow (locked), 80-95% = yellow (close), <80% = normal
        let border_style = if self.sync_quality > 0.95 {
            // Sync locked - steady green/accent glow
            Style::default().fg(self.theme.accent)
        } else if self.sync_quality > 0.80 {
            // Close to sync - steady yellow/warning glow
            Style::default().fg(self.theme.warning)
        } else if self.is_focused {
            self.theme.border_active()
        } else {
            self.theme.border()
        };

        // Add focus indicator to title (with sync quality glow)
        let title_text = if self.is_focused {
            format!(" ► {} ◄ ", self.title)
        } else {
            format!("   {}   ", self.title)
        };

        let title_style = if self.sync_quality > 0.95 {
            Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD)
        } else if self.sync_quality > 0.80 {
            Style::default().fg(self.theme.warning)
        } else if self.is_focused {
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

        // Row 3: Time / Remaining / KEY / BPM / Tempo / Beat Phase
        let remaining = (self.state.duration - self.state.position).max(0.0);
        let time_str = format!(
            "{} -{} ",
            Self::format_time(self.state.position),
            Self::format_time(remaining)
        );
        let key_str = self.state.key
            .as_ref()
            .map(|k| format!("KEY:{}", k))
            .unwrap_or_else(|| "KEY:---".to_string());
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
            Span::raw("│ "),
            Span::styled(key_str, Style::from(self.theme.accent)),
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
        // Use UI-side CRT peak hold for classic analog behavior (longer hold, slower decay)
        let peak_hold = self.crt_peak_hold.unwrap_or(self.state.peak_hold);
        let meter_width = (inner.width as usize).saturating_sub(12);
        let gain_spans = Self::render_meter(self.state.gain, peak_hold, meter_width, self.theme);
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
