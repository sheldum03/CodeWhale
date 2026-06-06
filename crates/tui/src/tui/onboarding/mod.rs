//! Onboarding flow rendering and helpers.

pub mod api_key;
pub mod language;
pub mod trust_directory;
pub mod welcome;

use std::path::{Path, PathBuf};

use ratatui::{
    buffer::Buffer,
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph, Wrap},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::palette::{self, UiTheme};
use crate::tui::app::{App, OnboardingState};

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().style(Style::default().bg(app.ui_theme.surface_bg));
    f.render_widget(block, area);

    const TOP_MARGIN: u16 = 2;
    let content_width = 76.min(area.width.saturating_sub(4));
    let content_height = 20.min(area.height.saturating_sub(TOP_MARGIN + 2));
    let content_area = Rect {
        x: (area.width.saturating_sub(content_width)) / 2,
        y: TOP_MARGIN,
        width: content_width,
        height: content_height,
    };

    let lines = match app.onboarding {
        OnboardingState::Welcome => welcome::lines(app.ui_theme),
        OnboardingState::Language => language::lines(app),
        OnboardingState::ApiKey => api_key::lines(app),
        OnboardingState::TrustDirectory => trust_directory::lines(app),
        OnboardingState::Tips => tips_lines(app),
        OnboardingState::None => Vec::new(),
    };

    if !lines.is_empty() {
        let inner = if palette::ascii_ui_enabled() {
            let footer = if app.onboarding_workspace_trust_gate {
                None
            } else {
                let (step, total) = onboarding_step(app);
                Some(format!(" Step {step}/{total} "))
            };
            render_ascii_onboarding_panel(
                content_area,
                f.buffer_mut(),
                " CodeWhale ",
                footer.as_deref(),
                app.ui_theme,
            )
        } else {
            let mut panel = Block::default()
                .title(Line::from(Span::styled(
                    " CodeWhale ",
                    Style::default()
                        .fg(app.ui_theme.accent_primary)
                        .add_modifier(Modifier::BOLD),
                )))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(app.ui_theme.border))
                .style(Style::default().bg(app.ui_theme.panel_bg))
                .padding(Padding::new(2, 2, 1, 1));
            if !app.onboarding_workspace_trust_gate {
                let (step, total) = onboarding_step(app);
                panel = panel.title_bottom(Line::from(Span::styled(
                    format!(" Step {step}/{total} "),
                    Style::default()
                        .fg(app.ui_theme.text_muted)
                        .add_modifier(Modifier::BOLD),
                )));
            }
            let inner = panel.inner(content_area);
            f.render_widget(panel, content_area);
            inner
        };
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        f.render_widget(paragraph, inner);
    }
}

fn render_ascii_onboarding_panel(
    area: Rect,
    buf: &mut Buffer,
    title: &str,
    footer: Option<&str>,
    ui_theme: UiTheme,
) -> Rect {
    if area.width == 0 || area.height == 0 {
        return Rect {
            x: area.x,
            y: area.y,
            width: 0,
            height: 0,
        };
    }

    let fill_style = Style::default().bg(ui_theme.panel_bg);
    let border_style = Style::default()
        .fg(ui_theme.border)
        .bg(ui_theme.panel_bg);
    let title_style = Style::default()
        .fg(ui_theme.accent_primary)
        .bg(ui_theme.panel_bg)
        .add_modifier(Modifier::BOLD);
    let footer_style = Style::default()
        .fg(ui_theme.text_muted)
        .bg(ui_theme.panel_bg)
        .add_modifier(Modifier::BOLD);

    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            buf[(x, y)].set_symbol(" ").set_style(fill_style);
        }
    }

    if area.width > 1 {
        let bottom = area.y + area.height.saturating_sub(1);
        for x in area.x..area.x.saturating_add(area.width) {
            buf[(x, area.y)].set_symbol("-").set_style(border_style);
            buf[(x, bottom)].set_symbol("-").set_style(border_style);
        }
    }

    if area.height > 1 {
        let right = area.x + area.width.saturating_sub(1);
        for y in area.y..area.y.saturating_add(area.height) {
            buf[(area.x, y)].set_symbol("|").set_style(border_style);
            buf[(right, y)].set_symbol("|").set_style(border_style);
        }
    }

    if area.width > 1 && area.height > 1 {
        let right = area.x + area.width.saturating_sub(1);
        let bottom = area.y + area.height.saturating_sub(1);
        for (x, y) in [
            (area.x, area.y),
            (right, area.y),
            (area.x, bottom),
            (right, bottom),
        ] {
            buf[(x, y)].set_symbol("+").set_style(border_style);
        }
    }

    if area.width > 4 {
        let title = ascii_prefix(title, area.width.saturating_sub(4) as usize);
        buf.set_string(area.x + 2, area.y, &title, title_style);
    }
    if let Some(footer) = footer {
        if area.width > 8 && area.height > 1 {
            let footer = ascii_prefix(footer, area.width.saturating_sub(4) as usize);
            buf.set_string(
                area.x + 2,
                area.y + area.height.saturating_sub(1),
                &footer,
                footer_style,
            );
        }
    }

    Rect {
        x: area.x.saturating_add(3),
        y: area.y.saturating_add(2),
        width: area.width.saturating_sub(6),
        height: area.height.saturating_sub(4),
    }
}

fn ascii_prefix(text: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }

    let mut width = 0usize;
    text.chars()
        .take_while(|ch| {
            let ch_width = UnicodeWidthChar::width(*ch).unwrap_or(0);
            if width + ch_width > max_width {
                false
            } else {
                width += ch_width;
                true
            }
        })
        .collect()
}

fn onboarding_step(app: &App) -> (usize, usize) {
    let needs_trust = !app.trust_mode && needs_trust(&app.workspace);
    // Welcome + Language + Tips are always shown.
    let mut total = 3;
    if app.onboarding_needs_api_key {
        total += 1;
    }
    if needs_trust {
        total += 1;
    }

    let step = match app.onboarding {
        OnboardingState::Welcome => 1,
        OnboardingState::Language => 2,
        OnboardingState::ApiKey => 3,
        OnboardingState::TrustDirectory => {
            // Welcome (1) + Language (2) + optional ApiKey
            if app.onboarding_needs_api_key { 4 } else { 3 }
        }
        OnboardingState::Tips => total,
        OnboardingState::None => total,
    };

    (step, total)
}

pub fn tips_lines(app: &App) -> Vec<ratatui::text::Line<'static>> {
    use crate::localization::MessageId;
    use ratatui::style::Modifier;
    use ratatui::text::{Line, Span};

    vec![
        Line::from(Span::styled(
            app.tr(MessageId::OnboardTipsTitle).to_string(),
            Style::default()
                .fg(app.ui_theme.accent_primary)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::raw(app.tr(MessageId::OnboardTipsLine1).to_string())),
        Line::from(Span::raw(app.tr(MessageId::OnboardTipsLine2).to_string())),
        Line::from(Span::raw(app.tr(MessageId::OnboardTipsLine3).to_string())),
        Line::from(Span::raw(app.tr(MessageId::OnboardTipsLine4).to_string())),
        Line::from(vec![
            Span::styled(
                app.tr(MessageId::OnboardTipsFooterEnter).to_string(),
                Style::default()
                    .fg(app.ui_theme.text_body)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                app.tr(MessageId::OnboardTipsFooterAction).to_string(),
                Style::default().fg(app.ui_theme.text_muted),
            ),
        ]),
    ]
}

pub fn default_marker_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| {
        let primary = home.join(".codewhale").join(".onboarded");
        if primary.exists() {
            return primary;
        }
        home.join(".deepseek").join(".onboarded")
    })
}

pub fn is_onboarded() -> bool {
    default_marker_path().is_some_and(|path| path.exists())
}

pub fn mark_onboarded() -> std::io::Result<PathBuf> {
    let path = default_marker_path().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "Home directory not found")
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, "")?;
    Ok(path)
}

pub fn needs_trust(workspace: &Path) -> bool {
    if crate::config::is_workspace_trusted(workspace) {
        return false;
    }

    let markers = [
        workspace.join(".deepseek").join("trusted"),
        workspace.join(".deepseek").join("trust.json"),
    ];
    !markers.iter().any(|path| path.exists())
}

pub fn mark_trusted(workspace: &Path) -> anyhow::Result<PathBuf> {
    crate::config::save_workspace_trust(workspace)
}

// ── API key validation and state-machine transitions ─────────────────

/// Result of inspecting an API-key string entered during onboarding.
///
/// `Accept` always lets the user proceed; the optional `warning` is shown
/// as a non-blocking status message (short keys, unusual formats, etc.).
/// `Reject` blocks the keystroke flow until the user fixes the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiKeyValidation {
    Accept { warning: Option<String> },
    Reject(String),
}

/// Validate an API key entered during onboarding. Whitespace-only or
/// whitespace-containing keys are rejected; short or hyphen-less keys
/// are accepted with a warning so unusual provider key formats still
/// work.
#[must_use]
pub fn validate_api_key_for_onboarding(api_key: &str) -> ApiKeyValidation {
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        return ApiKeyValidation::Reject("API key cannot be empty.".to_string());
    }
    if trimmed.contains(char::is_whitespace) {
        return ApiKeyValidation::Reject(
            "API key appears malformed (contains whitespace).".to_string(),
        );
    }
    if trimmed.len() < 16 {
        return ApiKeyValidation::Accept {
            warning: Some(
                "API key looks short. Double-check it, but unusual formats are allowed."
                    .to_string(),
            ),
        };
    }
    if !trimmed.contains('-') {
        return ApiKeyValidation::Accept {
            warning: Some(
                "API key format looks unusual. Check that the full key was copied.".to_string(),
            ),
        };
    }
    ApiKeyValidation::Accept { warning: None }
}

/// Welcome → Language transition. Clears the status message bar.
pub fn advance_onboarding_from_welcome(app: &mut App) {
    app.status_message = None;
    app.onboarding = OnboardingState::Language;
}

/// Language → next step. Routes to ApiKey when the session lacks a key,
/// to TrustDirectory when the workspace is untrusted, otherwise to Tips.
pub fn advance_onboarding_after_language(app: &mut App) {
    app.status_message = None;
    if app.onboarding_needs_api_key {
        app.onboarding = OnboardingState::ApiKey;
    } else if !app.trust_mode && needs_trust(&app.workspace) {
        app.onboarding = OnboardingState::TrustDirectory;
    } else {
        app.onboarding = OnboardingState::Tips;
    }
}

/// Re-validate the current `api_key_input` and project the result onto
/// `app.status_message`. `show_empty_error` reports the "cannot be empty"
/// message even when the input has not been touched yet (used right
/// before submission); otherwise an empty input clears the status bar.
pub fn sync_api_key_validation_status(app: &mut App, show_empty_error: bool) {
    if app.api_key_input.trim().is_empty() && !show_empty_error {
        app.status_message = None;
        return;
    }

    match validate_api_key_for_onboarding(&app.api_key_input) {
        ApiKeyValidation::Accept { warning } => {
            app.status_message = warning;
        }
        ApiKeyValidation::Reject(message) => {
            app.status_message = Some(message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_onboarding_panel_uses_plain_border_chars_and_padding() {
        let area = Rect::new(1, 1, 24, 8);
        let mut buf = Buffer::empty(Rect::new(0, 0, 28, 12));
        let inner = render_ascii_onboarding_panel(
            area,
            &mut buf,
            " CodeWhale ",
            Some(" Step 1/3 "),
            crate::palette::DEEPSEEK_SHELL_UI_THEME,
        );

        assert_eq!(buf[(area.x, area.y)].symbol(), "+");
        assert_eq!(buf[(area.x + 1, area.y)].symbol(), "-");
        assert_eq!(buf[(area.x, area.y + 1)].symbol(), "|");
        assert_eq!(
            buf[(
                area.x + area.width.saturating_sub(1),
                area.y + area.height.saturating_sub(1)
            )]
                .symbol(),
            "+"
        );
        assert_eq!(inner, Rect::new(area.x + 3, area.y + 2, 18, 4));
    }

    #[test]
    fn ascii_prefix_respects_cjk_display_width() {
        let prefix = ascii_prefix(" 欢迎向导 ", 8);

        assert!(
            UnicodeWidthStr::width(prefix.as_str()) <= 8,
            "prefix overflowed display width: {prefix:?}"
        );
        assert!(
            prefix.is_char_boundary(prefix.len()),
            "prefix should end on a valid char boundary: {prefix:?}"
        );
    }

    #[test]
    fn validate_rejects_empty_or_whitespace() {
        assert!(matches!(
            validate_api_key_for_onboarding(""),
            ApiKeyValidation::Reject(_)
        ));
        assert!(matches!(
            validate_api_key_for_onboarding("   "),
            ApiKeyValidation::Reject(_)
        ));
        assert!(matches!(
            validate_api_key_for_onboarding("sk live abc"),
            ApiKeyValidation::Reject(_)
        ));
    }

    #[test]
    fn validate_warns_on_short_or_no_hyphen_keys_but_accepts() {
        match validate_api_key_for_onboarding("abc123") {
            ApiKeyValidation::Accept { warning: Some(_) } => {}
            _ => panic!("expected accept-with-warning"),
        }
        match validate_api_key_for_onboarding("abcdefghijklmnop") {
            ApiKeyValidation::Accept { warning: Some(_) } => {}
            _ => panic!("expected accept-with-warning"),
        }
    }

    #[test]
    fn validate_accepts_well_formed_key() {
        assert_eq!(
            validate_api_key_for_onboarding("sk-1234567890abcdef"),
            ApiKeyValidation::Accept { warning: None }
        );
    }
}
