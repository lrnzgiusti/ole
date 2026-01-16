//! Status bar widget - mode indicator and command line

use crate::app::MessageType;
use crate::theme::Theme;
use ole_input::Mode;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

/// Widget for displaying the status bar with mode and command input
pub struct StatusBarWidget<'a> {
    mode: Mode,
    command_buffer: &'a str,
    message: Option<&'a str>,
    message_type: MessageType,
    theme: &'a Theme,
    effects_a: Option<String>, // Effect chain for deck A
    effects_b: Option<String>, // Effect chain for deck B
}

impl<'a> StatusBarWidget<'a> {
    pub fn new(mode: Mode, command_buffer: &'a str, theme: &'a Theme) -> Self {
        Self {
            mode,
            command_buffer,
            message: None,
            message_type: MessageType::Info,
            theme,
            effects_a: None,
            effects_b: None,
        }
    }

    pub fn message(mut self, msg: Option<&'a str>, msg_type: MessageType) -> Self {
        self.message = msg;
        self.message_type = msg_type;
        self
    }

    pub fn effects(mut self, effects_a: String, effects_b: String) -> Self {
        self.effects_a = if effects_a.is_empty() {
            None
        } else {
            Some(effects_a)
        };
        self.effects_b = if effects_b.is_empty() {
            None
        } else {
            Some(effects_b)
        };
        self
    }

    fn mode_string(&self) -> (&'static str, ratatui::style::Style) {
        match self.mode {
            Mode::Normal => ("NORMAL", self.theme.highlight()),
            Mode::Command => ("COMMAND", Style::from(self.theme.accent)),
            Mode::Effects => ("EFFECTS", Style::from(self.theme.warning)),
            Mode::Help => ("HELP", self.theme.highlight()),
            Mode::Browser => ("BROWSE", self.theme.deck_b_style()),
        }
    }
}

impl Widget for StatusBarWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 1 {
            return;
        }

        let chunks = Layout::horizontal([
            Constraint::Length(10), // Mode indicator
            Constraint::Min(20),    // Command/message area
            Constraint::Length(20), // Help hint
        ])
        .split(area);

        // Mode indicator
        let (mode_text, mode_style) = self.mode_string();
        let mode_line = Line::from(vec![
            Span::raw("["),
            Span::styled(mode_text, mode_style),
            Span::raw("]"),
        ]);
        Paragraph::new(mode_line).render(chunks[0], buf);

        // Command/message area
        let content = if self.mode == Mode::Command {
            Line::from(vec![
                Span::styled(":", Style::from(self.theme.accent)),
                Span::styled(self.command_buffer, self.theme.normal()),
                Span::styled("█", self.theme.highlight()), // Cursor
            ])
        } else if let Some(msg) = self.message {
            // Color message based on type
            let msg_style = match self.message_type {
                MessageType::Info => self.theme.dim(),
                MessageType::Success => Style::from(self.theme.accent),
                MessageType::Warning => Style::default().fg(self.theme.warning),
                MessageType::Error => Style::default().fg(self.theme.danger),
            };
            Line::from(Span::styled(msg, msg_style))
        } else if self.effects_a.is_some() || self.effects_b.is_some() {
            // Show effect chains when no message
            let mut spans = vec![];
            if let Some(ref fx_a) = self.effects_a {
                spans.push(Span::styled("A:", self.theme.dim()));
                spans.push(Span::styled(format!("[{}]", fx_a), self.theme.fx_enabled()));
            }
            if let Some(ref fx_b) = self.effects_b {
                if !spans.is_empty() {
                    spans.push(Span::raw("  "));
                }
                spans.push(Span::styled("B:", self.theme.dim()));
                spans.push(Span::styled(format!("[{}]", fx_b), self.theme.fx_enabled()));
            }
            Line::from(spans)
        } else {
            Line::from(Span::styled(
                "Ready. Press ? for help, : for commands",
                self.theme.dim(),
            ))
        };
        Paragraph::new(content).render(chunks[1], buf);

        // Help hint
        let help = match self.mode {
            Mode::Normal => "Tab:deck  e:fx  ?:help",
            Mode::Command => "Enter:run  Esc:cancel",
            Mode::Effects => "d/r/f+lvl  0:off  Esc",
            Mode::Browser => "j/k:nav  Enter:load",
            Mode::Help => "Esc:close help",
        };
        let help_line = Line::from(Span::styled(help, self.theme.dim()));
        Paragraph::new(help_line).render(chunks[2], buf);
    }
}

/// Help overlay widget
pub struct HelpWidget<'a> {
    theme: &'a Theme,
}

impl<'a> HelpWidget<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self { theme }
    }
}

impl Widget for HelpWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Clear background
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                buf[(x, y)].set_char(' ').set_style(self.theme.normal());
            }
        }

        let help_text = vec![
            "╔══════════════════════════════════════════════════════════╗",
            "║           OLE - Open Live Engine v0.1                    ║",
            "╠══════════════════════════════════════════════════════════╣",
            "║ TRANSPORT              DECK A      DECK B                ║",
            "║   Play/Pause             a           A                   ║",
            "║   Pause                  s           S                   ║",
            "║   Stop                   z           Z                   ║",
            "║   Nudge ±                x/c         X/C                 ║",
            "║   Tempo ±0.01           [/]         ,/.                  ║",
            "║   Tempo ±0.1            {/}         </>                  ║",
            "║   Gain ±                -/=         _/+                  ║",
            "╠──────────────────────────────────────────────────────────╣",
            "║ NAVIGATION & MIXING                                      ║",
            "║   Tab         Switch focus between decks                 ║",
            "║   ↑ / ↓       Beatjump +/- 4 beats (focused deck)        ║",
            "║   1-4         Jump to cue point (Shift+1-4 to set)       ║",
            "║   h / l       Crossfader left / right                    ║",
            "║   \\           Center crossfader                          ║",
            "║   b / B       Sync B→A / A→B (tempo + phase)             ║",
            "╠──────────────────────────────────────────────────────────╣",
            "║ EFFECTS (press 'e' to enter FX mode on focused deck)     ║",
            "║   d + 0-5     Delay: 0=off, 1=100ms ... 5=500ms          ║",
            "║   r + 0-5     Reverb: 0=off, 1=subtle ... 5=cathedral    ║",
            "║   f + 0       Filter off                                 ║",
            "║   f + l/b/h + 1-0   Filter: l=low b=band h=high +level   ║",
            "║   Esc         Exit effects mode                          ║",
            "╠──────────────────────────────────────────────────────────╣",
            "║ COMMANDS (:)                                             ║",
            "║   :load a <path>    Load track to deck A                 ║",
            "║   :load b <path>    Load track to deck B                 ║",
            "║   :theme <name>     green / amber / cyber                ║",
            "║   :q                Quit                                 ║",
            "╠──────────────────────────────────────────────────────────╣",
            "║              Esc or ? to close  │  Ctrl-Q to quit        ║",
            "╚══════════════════════════════════════════════════════════╝",
        ];

        let start_y = area.y + area.height.saturating_sub(help_text.len() as u16) / 2;
        let start_x = area.x + area.width.saturating_sub(62) / 2;

        for (i, line) in help_text.iter().enumerate() {
            let y = start_y + i as u16;
            if y >= area.y + area.height {
                break;
            }

            for (j, ch) in line.chars().enumerate() {
                let x = start_x + j as u16;
                if x >= area.x + area.width {
                    break;
                }

                let style = if ch == '║'
                    || ch == '╔'
                    || ch == '╗'
                    || ch == '╚'
                    || ch == '╝'
                    || ch == '═'
                    || ch == '╠'
                    || ch == '╣'
                    || ch == '─'
                    || ch == '│'
                {
                    self.theme.border()
                } else {
                    self.theme.normal()
                };

                buf[(x, y)].set_char(ch).set_style(style);
            }
        }
    }
}
