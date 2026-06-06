//! `/feedback` picker for GitHub feedback destinations.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Widget},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::palette;
use crate::palette::UiTheme;
use crate::tui::views::{CommandPaletteAction, ModalKind, ModalView, ViewAction, ViewEvent};

#[derive(Debug, Clone, Copy)]
struct FeedbackOption {
    number: char,
    label: &'static str,
    description: &'static str,
    command: &'static str,
}

const OPTIONS: &[FeedbackOption] = &[
    FeedbackOption {
        number: '1',
        label: "Bug report",
        description: "Report a problem or regression",
        command: "/feedback bug",
    },
    FeedbackOption {
        number: '2',
        label: "Feature request",
        description: "Suggest an idea or improvement",
        command: "/feedback feature",
    },
    FeedbackOption {
        number: '3',
        label: "Security vulnerability",
        description: "Review the security policy before reporting",
        command: "/feedback security",
    },
];

pub struct FeedbackPickerView {
    selected: usize,
    ui_theme: UiTheme,
}

impl FeedbackPickerView {
    #[must_use]
    pub fn new() -> Self {
        Self {
            selected: 0,
            ui_theme: palette::UI_THEME,
        }
    }

    #[must_use]
    pub fn with_ui_theme(mut self, ui_theme: UiTheme) -> Self {
        self.ui_theme = ui_theme;
        self
    }

    fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    fn move_down(&mut self) {
        let max = OPTIONS.len().saturating_sub(1);
        if self.selected < max {
            self.selected += 1;
        }
    }

    fn select_number(&mut self, number: char) -> Option<ViewAction> {
        let idx = OPTIONS.iter().position(|option| option.number == number)?;
        self.selected = idx;
        Some(self.selected_action())
    }

    fn selected_action(&self) -> ViewAction {
        let command = OPTIONS
            .get(self.selected)
            .map(|option| option.command)
            .unwrap_or(OPTIONS[0].command)
            .to_string();
        ViewAction::EmitAndClose(ViewEvent::CommandPaletteSelected {
            action: CommandPaletteAction::ExecuteCommand { command },
        })
    }
}

impl Default for FeedbackPickerView {
    fn default() -> Self {
        Self::new()
    }
}

impl ModalView for FeedbackPickerView {
    fn kind(&self) -> ModalKind {
        ModalKind::FeedbackPicker
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Esc => ViewAction::Close,
            KeyCode::Enter => self.selected_action(),
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_up();
                ViewAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_down();
                ViewAction::None
            }
            KeyCode::Char(number)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && OPTIONS.iter().any(|option| option.number == number) =>
            {
                self.select_number(number).unwrap_or(ViewAction::None)
            }
            _ => ViewAction::None,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let popup_width = 78.min(area.width.saturating_sub(4)).max(44);
        let needed_height = (OPTIONS.len() as u16).saturating_add(7);
        let popup_height = needed_height.min(area.height.saturating_sub(4)).max(8);

        let popup_area = Rect {
            x: area.x + (area.width.saturating_sub(popup_width)) / 2,
            y: area.y + (area.height.saturating_sub(popup_height)) / 2,
            width: popup_width,
            height: popup_height,
        };

        Clear.render(popup_area, buf);

        let inner = if palette::ascii_ui_enabled() {
            render_ascii_feedback_picker_chrome(popup_area, buf, self.ui_theme)
        } else {
            let block = Block::default()
                .title(Line::from(Span::styled(
                    " Feedback ",
                    Style::default()
                        .fg(self.ui_theme.accent_primary)
                        .add_modifier(Modifier::BOLD),
                )))
                .title_bottom(Line::from(vec![
                    Span::styled(" Up/Down ", Style::default().fg(self.ui_theme.text_muted)),
                    Span::raw("move "),
                    Span::styled(" Enter ", Style::default().fg(self.ui_theme.text_muted)),
                    Span::raw("open "),
                    Span::styled(" Esc ", Style::default().fg(self.ui_theme.text_muted)),
                    Span::raw("cancel "),
                ]))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.ui_theme.border))
                .style(Style::default().bg(self.ui_theme.surface_bg))
                .padding(Padding::uniform(1));

            let inner = block.inner(popup_area);
            block.render(popup_area, buf);
            inner
        };

        let mut lines = Vec::with_capacity(OPTIONS.len() + 2);
        lines.push(Line::from(Span::styled(
            "Choose where to send feedback:",
            Style::default().fg(self.ui_theme.text_muted),
        )));
        lines.push(Line::from(""));

        for (idx, option) in OPTIONS.iter().enumerate() {
            let is_selected = idx == self.selected;
            let row_style = if is_selected {
                Style::default()
                    .fg(self.ui_theme.selection_text)
                    .bg(self.ui_theme.selection_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.ui_theme.text_body)
            };
            let desc_style = if is_selected {
                Style::default()
                    .fg(self.ui_theme.selection_text)
                    .bg(self.ui_theme.selection_bg)
            } else {
                Style::default().fg(self.ui_theme.text_muted)
            };
            let pointer = if is_selected { ">" } else { " " };

            lines.push(Line::from(vec![
                Span::styled(format!(" {pointer} {}. ", option.number), row_style),
                Span::styled(option.label, row_style),
                Span::raw("    "),
                Span::styled(option.description, desc_style),
            ]));
        }

        Paragraph::new(lines).render(inner, buf);
    }
}

fn render_ascii_feedback_picker_chrome(area: Rect, buf: &mut Buffer, theme: UiTheme) -> Rect {
    if area.width == 0 || area.height == 0 {
        return Rect {
            x: area.x,
            y: area.y,
            width: 0,
            height: 0,
        };
    }

    let fill_style = Style::default().bg(theme.surface_bg);
    let border_style = Style::default().fg(theme.border).bg(theme.surface_bg);
    let title_style = Style::default()
        .fg(theme.accent_primary)
        .bg(theme.surface_bg)
        .add_modifier(Modifier::BOLD);
    let hint_style = Style::default()
        .fg(theme.text_muted)
        .bg(theme.surface_bg);

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
        let title = ascii_prefix(" Feedback ", area.width.saturating_sub(4) as usize);
        buf.set_string(area.x + 2, area.y, &title, title_style);
    }
    if area.width > 8 && area.height > 1 {
        let hint = ascii_prefix(
            " Up/Down move Enter open Esc cancel ",
            area.width.saturating_sub(4) as usize,
        );
        buf.set_string(
            area.x + 2,
            area.y + area.height.saturating_sub(1),
            &hint,
            hint_style,
        );
    }

    Rect {
        x: area.x.saturating_add(2),
        y: area.y.saturating_add(2),
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(4),
    }
}

fn ascii_prefix(text: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }

    let mut out = String::new();
    let mut width = 0usize;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > max_width {
            break;
        }
        out.push(ch);
        width += ch_width;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    fn emitted_command(action: ViewAction) -> String {
        match action {
            ViewAction::EmitAndClose(ViewEvent::CommandPaletteSelected {
                action: CommandPaletteAction::ExecuteCommand { command },
            }) => command,
            other => panic!("expected feedback command emit, got {other:?}"),
        }
    }

    #[test]
    fn enter_emits_selected_feedback_command() {
        let mut view = FeedbackPickerView::new();
        let command =
            emitted_command(view.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert_eq!(command, "/feedback bug");
    }

    #[test]
    fn arrow_down_selects_feature_command() {
        let mut view = FeedbackPickerView::new();
        view.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let command =
            emitted_command(view.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert_eq!(command, "/feedback feature");
    }

    #[test]
    fn digit_selects_security_command() {
        let mut view = FeedbackPickerView::new();
        let command =
            emitted_command(view.handle_key(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE)));
        assert_eq!(command, "/feedback security");
    }

    #[test]
    fn esc_closes_picker() {
        let mut view = FeedbackPickerView::new();
        assert!(matches!(
            view.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            ViewAction::Close
        ));
    }

    #[test]
    fn ascii_feedback_picker_chrome_uses_plain_border_chars() {
        let area = Rect::new(1, 1, 24, 8);
        let mut buf = Buffer::empty(Rect::new(0, 0, 28, 12));
        let inner =
            render_ascii_feedback_picker_chrome(area, &mut buf, palette::DEEPSEEK_SHELL_UI_THEME);

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
        assert_eq!(inner, Rect::new(area.x + 2, area.y + 2, 20, 4));
    }

    #[test]
    fn ascii_prefix_respects_cjk_display_width() {
        let prefix = ascii_prefix(" 反馈选择器 ", 8);

        assert!(
            UnicodeWidthStr::width(prefix.as_str()) <= 8,
            "prefix overflowed display width: {prefix:?}"
        );
        assert!(
            prefix.is_char_boundary(prefix.len()),
            "prefix must not split UTF-8 codepoints: {prefix:?}"
        );
    }

    #[test]
    fn selected_feedback_row_uses_injected_theme_selection_colors() {
        let mut theme = palette::UI_THEME;
        theme.selection_bg = ratatui::style::Color::Indexed(24);
        theme.selection_text = ratatui::style::Color::Indexed(255);
        let view = FeedbackPickerView::new().with_ui_theme(theme);
        let area = Rect::new(0, 0, 90, 16);
        let mut buf = Buffer::empty(area);

        view.render(area, &mut buf);

        let mut selected_row_uses_theme = false;
        for y in area.top()..area.bottom() {
            let mut row = String::new();
            let mut row_has_selection = false;
            for x in area.left()..area.right() {
                let cell = &buf[(x, y)];
                row.push_str(cell.symbol());
                row_has_selection |=
                    cell.bg == theme.selection_bg && cell.fg == theme.selection_text;
            }
            if row.contains("Bug report") && row_has_selection {
                selected_row_uses_theme = true;
                break;
            }
        }

        assert!(
            selected_row_uses_theme,
            "selected feedback row should use injected selection colors"
        );
    }
}
