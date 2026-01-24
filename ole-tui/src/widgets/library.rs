//! Library widget for displaying track collection
//!
//! Shows tracks sorted by BPM and grouped by Camelot key,
//! with harmonic compatibility highlighting.

use crate::theme::Theme;
use ole_analysis::CamelotKey;
use ole_library::CachedAnalysis;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{
        Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget,
        Widget,
    },
};

/// State for the library widget
#[derive(Debug, Clone, Default)]
pub struct LibraryState {
    /// All loaded tracks
    pub tracks: Vec<CachedAnalysis>,
    /// Currently selected track index
    pub selected_index: usize,
    /// Scroll offset for the list
    pub scroll_offset: usize,
    /// Filter by Camelot key (if set)
    pub filter_key: Option<String>,
    /// Current playing track's key (for compatibility highlighting)
    pub current_playing_key: Option<String>,
    /// Whether a scan is in progress
    pub is_scanning: bool,
    /// Scan progress (current, total)
    pub scan_progress: (usize, usize),
}

impl LibraryState {
    /// Create a new library state
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the track list
    pub fn set_tracks(&mut self, tracks: Vec<CachedAnalysis>) {
        self.tracks = tracks;
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        let count = self.filtered_tracks().len();
        if count > 0 && self.selected_index < count - 1 {
            self.selected_index += 1;
        }
    }

    /// Move selection up
    pub fn select_prev(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Move selection to first item
    pub fn select_first(&mut self) {
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    /// Move selection to last item
    pub fn select_last(&mut self) {
        let count = self.filtered_tracks().len();
        if count > 0 {
            self.selected_index = count - 1;
        }
    }

    /// Get filtered tracks based on current filter
    pub fn filtered_tracks(&self) -> Vec<&CachedAnalysis> {
        self.tracks
            .iter()
            .filter(|t| {
                if let Some(ref filter) = self.filter_key {
                    t.key.as_ref().map(|k| k == filter).unwrap_or(false)
                } else {
                    true
                }
            })
            .collect()
    }

    /// Get the currently selected track
    pub fn selected_track(&self) -> Option<&CachedAnalysis> {
        self.filtered_tracks().get(self.selected_index).copied()
    }

    /// Set filter to a specific key
    pub fn set_filter(&mut self, key: Option<String>) {
        self.filter_key = key;
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    /// Filter to harmonically compatible keys based on current playing track
    pub fn filter_compatible(&mut self) {
        if let Some(ref current_key) = self.current_playing_key {
            if let Some(camelot) = CamelotKey::parse(current_key) {
                // Get all compatible keys
                let compatible: Vec<String> = camelot
                    .compatible_keys()
                    .iter()
                    .map(|k| k.to_string())
                    .collect();

                // Store compatible keys as comma-separated for display, or use first one
                if !compatible.is_empty() {
                    // Just use first compatible key for simple filter
                    self.filter_key = Some(compatible[0].clone());
                    self.selected_index = 0;
                    self.scroll_offset = 0;
                }
            }
        }
    }

    /// Jump to first track matching a Camelot key (e.g., 8A, 3B)
    /// Returns true if a matching track was found
    pub fn jump_to_key(&mut self, position: u8, is_minor: bool) -> bool {
        let key_str = format!("{}{}", position, if is_minor { 'A' } else { 'B' });

        // Clear any existing filter first
        self.filter_key = None;

        // Find first track with this key
        for (i, track) in self.tracks.iter().enumerate() {
            if track.key.as_ref().map(|k| k == &key_str).unwrap_or(false) {
                self.selected_index = i;
                self.scroll_offset = i.saturating_sub(5); // Center it a bit
                return true;
            }
        }
        false
    }

    /// Jump to first track near a target BPM (within ±3 BPM)
    /// Returns true if a matching track was found
    pub fn jump_to_bpm(&mut self, target_bpm: u16) -> bool {
        // Clear any existing filter first
        self.filter_key = None;

        let target = target_bpm as f32;

        // Find first track within ±3 BPM
        for (i, track) in self.tracks.iter().enumerate() {
            if let Some(bpm) = track.bpm {
                if (bpm - target).abs() <= 3.0 {
                    self.selected_index = i;
                    self.scroll_offset = i.saturating_sub(5);
                    return true;
                }
            }
        }

        // If no exact match, find closest
        let mut closest_idx = 0;
        let mut closest_diff = f32::MAX;
        for (i, track) in self.tracks.iter().enumerate() {
            if let Some(bpm) = track.bpm {
                let diff = (bpm - target).abs();
                if diff < closest_diff {
                    closest_diff = diff;
                    closest_idx = i;
                }
            }
        }

        if closest_diff < f32::MAX {
            self.selected_index = closest_idx;
            self.scroll_offset = closest_idx.saturating_sub(5);
            true
        } else {
            false
        }
    }

    /// Update scroll offset to keep selection visible
    fn update_scroll(&mut self, visible_height: usize) {
        if visible_height == 0 {
            return;
        }

        // Scroll down if selection is below visible area
        if self.selected_index >= self.scroll_offset + visible_height {
            self.scroll_offset = self.selected_index - visible_height + 1;
        }

        // Scroll up if selection is above visible area
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        }
    }
}

/// Widget for displaying the track library
pub struct LibraryWidget<'a> {
    state: &'a mut LibraryState,
    theme: &'a Theme,
    is_focused: bool,
}

impl<'a> LibraryWidget<'a> {
    /// Create a new library widget
    pub fn new(state: &'a mut LibraryState, theme: &'a Theme) -> Self {
        Self {
            state,
            theme,
            is_focused: false,
        }
    }

    /// Set whether the widget is focused
    pub fn focused(mut self, focused: bool) -> Self {
        self.is_focused = focused;
        self
    }

    /// Check if a track's key is compatible with the current playing track
    fn is_compatible_key(&self, track_key: Option<&str>) -> bool {
        let Some(ref current) = self.state.current_playing_key else {
            return false;
        };
        let Some(track) = track_key else {
            return false;
        };

        // Parse and check compatibility
        if let (Some(current_camelot), Some(track_camelot)) =
            (CamelotKey::parse(current), CamelotKey::parse(track))
        {
            current_camelot.is_compatible(&track_camelot)
        } else {
            false
        }
    }

    /// Format BPM for display
    fn format_bpm(bpm: Option<f32>) -> String {
        bpm.map(|b| format!("{:6.1}", b))
            .unwrap_or_else(|| "  --- ".to_string())
    }

    /// Format key for display
    fn format_key(key: Option<&str>) -> String {
        key.map(|k| format!("{:>3}", k))
            .unwrap_or_else(|| " ? ".to_string())
    }

    /// Format duration for display
    fn format_duration(secs: f64) -> String {
        let mins = (secs / 60.0) as u32;
        let secs = (secs % 60.0) as u32;
        format!("{:2}:{:02}", mins, secs)
    }
}

impl Widget for LibraryWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Build title
        let title = if self.state.is_scanning {
            format!(
                " LIBRARY [{}/{}] ",
                self.state.scan_progress.0, self.state.scan_progress.1
            )
        } else {
            let count = self.state.filtered_tracks().len();
            if self.state.filter_key.is_some() {
                format!(" LIBRARY [{} filtered] ", count)
            } else {
                format!(" LIBRARY [{}] ", count)
            }
        };

        let border_style = if self.is_focused {
            self.theme.border_active()
        } else {
            self.theme.border()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(title, self.theme.title()));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 2 || inner.width < 30 {
            return;
        }

        // Reserve space for scrollbar
        let list_width = inner.width.saturating_sub(1);

        // Header row
        let header = Line::from(vec![
            Span::styled("KEY", self.theme.dim()),
            Span::styled("    BPM", self.theme.dim()),
            Span::styled("  TIME", self.theme.dim()),
            Span::styled("  TITLE", self.theme.dim()),
        ]);

        let header_area = Rect::new(inner.x, inner.y, list_width, 1);
        Paragraph::new(header).render(header_area, buf);

        // Track list area (below header)
        let list_height = (inner.height - 1) as usize;
        let list_area = Rect::new(inner.x, inner.y + 1, list_width, inner.height - 1);

        // Update scroll position
        self.state.update_scroll(list_height);

        let filtered = self.state.filtered_tracks();
        let scroll_offset = self.state.scroll_offset;

        // Render visible tracks
        for (i, track) in filtered
            .iter()
            .skip(scroll_offset)
            .take(list_height)
            .enumerate()
        {
            let y = list_area.y + i as u16;
            let global_idx = scroll_offset + i;
            let is_selected = global_idx == self.state.selected_index;
            let is_compatible = self.is_compatible_key(track.key.as_deref());

            // Build row content
            let key_str = Self::format_key(track.key.as_deref());
            let bpm_str = Self::format_bpm(track.bpm);
            let time_str = Self::format_duration(track.duration_secs);

            // Calculate available width for title
            // KEY(3) + space(1) + BPM(6) + space(1) + TIME(5) + space(2) = 18
            let title_width = (list_width as usize).saturating_sub(18);
            let title: String = track.title.chars().take(title_width).collect();

            // Determine styles
            let base_style = if is_selected {
                self.theme.highlight()
            } else if is_compatible {
                self.theme.normal().add_modifier(Modifier::BOLD)
            } else {
                self.theme.normal()
            };

            let key_style = if is_compatible && !is_selected {
                // Use explicit fg + bg to avoid white text on default background
                ratatui::style::Style::default()
                    .fg(self.theme.accent)
                    .bg(self.theme.bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                base_style
            };

            // Build the line
            let line = Line::from(vec![
                Span::styled(key_str, key_style),
                Span::styled(" ", base_style),
                Span::styled(bpm_str, base_style),
                Span::styled(" ", base_style),
                Span::styled(time_str, base_style),
                Span::styled("  ", base_style),
                Span::styled(title, base_style),
            ]);

            let row_area = Rect::new(list_area.x, y, list_width, 1);
            Paragraph::new(line).render(row_area, buf);
        }

        // Render scrollbar if needed
        if filtered.len() > list_height {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
            let mut scrollbar_state = ScrollbarState::new(filtered.len()).position(scroll_offset);

            let scrollbar_area =
                Rect::new(inner.x + inner.width - 1, inner.y + 1, 1, inner.height - 1);
            StatefulWidget::render(scrollbar, scrollbar_area, buf, &mut scrollbar_state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_track(title: &str, bpm: f32, key: &str) -> CachedAnalysis {
        CachedAnalysis {
            path: PathBuf::from(format!("/test/{}.mp3", title)),
            file_size: 1000000,
            modified_time: 1700000000,
            duration_secs: 180.0,
            bpm: Some(bpm),
            bpm_confidence: Some(0.9),
            key: Some(key.to_string()),
            key_confidence: Some(0.85),
            title: title.to_string(),
            artist: "Test Artist".to_string(),
        }
    }

    #[test]
    fn test_library_state_navigation() {
        let mut state = LibraryState::new();
        state.set_tracks(vec![
            make_track("Track 1", 128.0, "8A"),
            make_track("Track 2", 130.0, "8A"),
            make_track("Track 3", 125.0, "9A"),
        ]);

        assert_eq!(state.selected_index, 0);

        state.select_next();
        assert_eq!(state.selected_index, 1);

        state.select_next();
        assert_eq!(state.selected_index, 2);

        // Should not go past end
        state.select_next();
        assert_eq!(state.selected_index, 2);

        state.select_prev();
        assert_eq!(state.selected_index, 1);

        state.select_first();
        assert_eq!(state.selected_index, 0);

        state.select_last();
        assert_eq!(state.selected_index, 2);
    }

    #[test]
    fn test_library_state_filter() {
        let mut state = LibraryState::new();
        state.set_tracks(vec![
            make_track("Track 1", 128.0, "8A"),
            make_track("Track 2", 130.0, "8A"),
            make_track("Track 3", 125.0, "9A"),
        ]);

        assert_eq!(state.filtered_tracks().len(), 3);

        state.set_filter(Some("8A".to_string()));
        assert_eq!(state.filtered_tracks().len(), 2);

        state.set_filter(Some("9A".to_string()));
        assert_eq!(state.filtered_tracks().len(), 1);

        state.set_filter(None);
        assert_eq!(state.filtered_tracks().len(), 3);
    }

    #[test]
    fn test_selected_track() {
        let mut state = LibraryState::new();
        state.set_tracks(vec![
            make_track("Track 1", 128.0, "8A"),
            make_track("Track 2", 130.0, "9A"),
        ]);

        assert_eq!(state.selected_track().unwrap().title, "Track 1");

        state.select_next();
        assert_eq!(state.selected_track().unwrap().title, "Track 2");
    }

    #[test]
    fn test_format_bpm() {
        assert_eq!(LibraryWidget::format_bpm(Some(128.0)), " 128.0");
        assert_eq!(LibraryWidget::format_bpm(Some(99.5)), "  99.5");
        assert_eq!(LibraryWidget::format_bpm(None), "  --- ");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(LibraryWidget::format_duration(180.0), " 3:00");
        assert_eq!(LibraryWidget::format_duration(65.0), " 1:05");
        assert_eq!(LibraryWidget::format_duration(3599.0), "59:59");
    }
}
