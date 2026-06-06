//! Welcome screen content for onboarding.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::palette::UiTheme;

pub fn lines(ui_theme: UiTheme) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            "codewhale",
            Style::default()
                .fg(ui_theme.accent_primary)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("Version {}", env!("CARGO_PKG_VERSION")),
            Style::default().fg(ui_theme.text_muted),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "A focused terminal workspace for longer model sessions.",
            Style::default().fg(ui_theme.text_body),
        )),
        Line::from(Span::styled(
            "You'll add an API key, review trust for this directory, and then land in the chat.",
            Style::default().fg(ui_theme.text_muted),
        )),
        Line::from(Span::styled(
            "The main composer is multi-line, so you can write full prompts instead of squeezing everything into one line.",
            Style::default().fg(ui_theme.text_muted),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press Enter to continue.",
            Style::default().fg(ui_theme.text_body),
        )),
        Line::from(Span::styled(
            "Ctrl+C exits at any point.",
            Style::default().fg(ui_theme.text_muted),
        )),
    ]
}
