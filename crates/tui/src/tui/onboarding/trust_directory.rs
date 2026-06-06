//! Workspace trust prompt for onboarding.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::localization::MessageId;
use crate::tui::app::App;

pub fn lines(app: &App) -> Vec<Line<'static>> {
    let ui_theme = app.ui_theme;
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        app.tr(MessageId::OnboardTrustTitle).to_string(),
        Style::default()
            .fg(ui_theme.accent_primary)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        app.tr(MessageId::OnboardTrustQuestion).to_string(),
        Style::default().fg(ui_theme.text_body),
    )));
    lines.push(Line::from(Span::styled(
        format!(
            "{}{}",
            app.tr(MessageId::OnboardTrustLocationPrefix),
            crate::utils::display_path(&app.workspace)
        ),
        Style::default().fg(ui_theme.text_muted),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        app.tr(MessageId::OnboardTrustRiskHint).to_string(),
        Style::default().fg(ui_theme.text_muted),
    )));
    lines.push(Line::from(Span::styled(
        app.tr(MessageId::OnboardTrustEffectHint).to_string(),
        Style::default().fg(ui_theme.text_muted),
    )));
    if let Some(message) = app.status_message.as_deref() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            message.to_string(),
            Style::default().fg(ui_theme.warning),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            app.tr(MessageId::OnboardTrustFooterPrefix).to_string(),
            Style::default().fg(ui_theme.text_muted),
        ),
        Span::styled(
            "1/Y",
            Style::default()
                .fg(ui_theme.text_body)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            app.tr(MessageId::OnboardTrustFooterMiddle).to_string(),
            Style::default().fg(ui_theme.text_muted),
        ),
        Span::styled(
            "2/N",
            Style::default()
                .fg(ui_theme.text_body)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            app.tr(MessageId::OnboardTrustFooterSuffix).to_string(),
            Style::default().fg(ui_theme.text_muted),
        ),
    ]));
    lines
}
