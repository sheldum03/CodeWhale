//! Language picker for first-run onboarding (#566).
//!
//! Surfaces every locale the TUI ships translations for, plus an `auto`
//! option that defers to `LC_ALL` / `LANG`. Selection persists via
//! `Settings::save` immediately so the rest of onboarding (and every
//! subsequent session) reads the chosen tag.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::localization::MessageId;
use crate::palette;
use crate::tui::app::App;

/// Locale options shown in the picker. Order matches the keyboard hotkeys.
/// Each entry is `(hotkey, settings_tag, native_name, english_label)`.
/// `settings_tag` is what `Settings::set("locale", …)` accepts and what
/// `localization::Locale` resolves on next read.
pub const LANGUAGE_OPTIONS: &[(char, &str, &str, &str)] = &[
    ('1', "auto", "Auto-detect", "(LC_ALL / LANG)"),
    ('2', "en", "English", ""),
    ('3', "ja", "日本語", "(Japanese)"),
    ('4', "zh-Hans", "简体中文", "(Simplified Chinese)"),
    ('5', "zh-Hant", "繁體中文", "(Traditional Chinese)"),
    ('6', "pt-BR", "Português (Brasil)", "(Brazilian Portuguese)"),
];

pub fn lines(app: &App) -> Vec<Line<'static>> {
    let ui_theme = app.ui_theme;
    let current_owned = app.current_locale_tag();
    let current = current_owned.as_str();

    let mut out: Vec<Line<'static>> = vec![
        Line::from(Span::styled(
            app.tr(MessageId::OnboardLanguageTitle).to_string(),
            Style::default()
                .fg(ui_theme.accent_primary)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            app.tr(MessageId::OnboardLanguageBlurb).to_string(),
            Style::default().fg(ui_theme.text_muted),
        )),
        Line::from(""),
    ];

    for (hotkey, tag, native, english) in LANGUAGE_OPTIONS {
        let is_current = current == *tag;
        let bullet = language_bullet(is_current);
        let bullet_color = if is_current {
            ui_theme.accent_primary
        } else {
            ui_theme.text_muted
        };
        let mut spans: Vec<Span<'static>> = vec![
            Span::styled(format!("  {bullet}  "), Style::default().fg(bullet_color)),
            Span::styled(
                format!("[{hotkey}] "),
                Style::default()
                    .fg(ui_theme.text_body)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                native.to_string(),
                Style::default().fg(ui_theme.text_body),
            ),
        ];
        if !english.is_empty() {
            spans.push(Span::styled(
                format!(" {english}"),
                Style::default().fg(ui_theme.text_muted),
            ));
        }
        out.push(Line::from(spans));
    }

    out.push(Line::from(""));
    out.push(Line::from(Span::styled(
        app.tr(MessageId::OnboardLanguageFooter).to_string(),
        Style::default().fg(ui_theme.text_muted),
    )));

    out
}

fn language_bullet(selected: bool) -> &'static str {
    if palette::ascii_ui_enabled() {
        if selected { "*" } else { "o" }
    } else if selected {
        "\u{25CF}"
    } else {
        "\u{25CB}"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_bullet_has_ascii_fallback() {
        let selected = language_bullet(true);
        let unselected = language_bullet(false);
        if palette::ascii_ui_enabled() {
            assert_eq!((selected, unselected), ("*", "o"));
        } else {
            assert_eq!((selected, unselected), ("\u{25CF}", "\u{25CB}"));
        }
    }
}
