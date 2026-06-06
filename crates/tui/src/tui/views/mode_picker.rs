//! `/mode` picker for Agent / Plan / YOLO.

use crossterm::event::{KeyCode, KeyEvent};
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
use crate::tui::app::AppMode;
use crate::tui::views::{ModalKind, ModalView, ViewAction, ViewEvent};

#[derive(Debug, Clone, Copy)]
struct ModeRow {
    mode: AppMode,
    number: char,
    name: &'static str,
    hint: &'static str,
}

const MODE_ROWS: &[ModeRow] = &[
    ModeRow {
        mode: AppMode::Agent,
        number: '1',
        name: "Agent",
        hint: "Normal execution with approvals",
    },
    ModeRow {
        mode: AppMode::Plan,
        number: '2',
        name: "Plan",
        hint: "Plan first before execution",
    },
    ModeRow {
        mode: AppMode::Yolo,
        number: '3',
        name: "YOLO",
        hint: "Auto-approve; shell enabled",
    },
];

pub struct ModePickerView {
    cursor: usize,
    ui_theme: UiTheme,
}

impl ModePickerView {
    #[must_use]
    pub fn new(current: AppMode) -> Self {
        let cursor = MODE_ROWS
            .iter()
            .position(|row| row.mode == current)
            .unwrap_or(0);
        Self {
            cursor,
            ui_theme: palette::UI_THEME,
        }
    }

    #[must_use]
    pub fn with_ui_theme(mut self, ui_theme: UiTheme) -> Self {
        self.ui_theme = ui_theme;
        self
    }

    fn selected_mode(&self) -> AppMode {
        MODE_ROWS
            .get(self.cursor)
            .map_or(AppMode::Agent, |row| row.mode)
    }

    fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    fn move_down(&mut self) {
        let max = MODE_ROWS.len().saturating_sub(1);
        if self.cursor < max {
            self.cursor += 1;
        }
    }

    fn select_by_number(&mut self, number: char) -> Option<ViewAction> {
        let idx = MODE_ROWS.iter().position(|row| row.number == number)?;
        self.cursor = idx;
        Some(ViewAction::EmitAndClose(ViewEvent::ModeSelected {
            mode: self.selected_mode(),
        }))
    }
}

impl ModalView for ModePickerView {
    fn kind(&self) -> ModalKind {
        ModalKind::ModePicker
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Esc => ViewAction::Close,
            KeyCode::Enter => ViewAction::EmitAndClose(ViewEvent::ModeSelected {
                mode: self.selected_mode(),
            }),
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_up();
                ViewAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_down();
                ViewAction::None
            }
            KeyCode::Char(number) => self.select_by_number(number).unwrap_or(ViewAction::None),
            _ => ViewAction::None,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let popup_width = 68.min(area.width.saturating_sub(4)).max(44);
        let popup_height = 9.min(area.height.saturating_sub(4)).max(7);
        let popup_area = Rect {
            x: area.x + (area.width.saturating_sub(popup_width)) / 2,
            y: area.y + (area.height.saturating_sub(popup_height)) / 2,
            width: popup_width,
            height: popup_height,
        };

        Clear.render(popup_area, buf);

        let inner = if palette::ascii_ui_enabled() {
            render_ascii_mode_picker_chrome(popup_area, buf, self.ui_theme)
        } else {
            let block = Block::default()
                .title(Line::from(Span::styled(
                    " Mode ",
                    Style::default()
                        .fg(self.ui_theme.accent_primary)
                        .add_modifier(Modifier::BOLD),
                )))
                .title_bottom(Line::from(vec![
                    Span::styled(" Up/Down ", Style::default().fg(self.ui_theme.text_muted)),
                    Span::raw("move "),
                    Span::styled(" Enter ", Style::default().fg(self.ui_theme.text_muted)),
                    Span::raw("select "),
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

        let mut lines = Vec::with_capacity(MODE_ROWS.len() + 1);
        lines.push(Line::from(Span::styled(
            "Choose how CodeWhale should operate:",
            Style::default().fg(self.ui_theme.text_muted),
        )));

        for (idx, row) in MODE_ROWS.iter().enumerate() {
            let is_cursor = idx == self.cursor;
            let row_style = if is_cursor {
                Style::default()
                    .fg(self.ui_theme.selection_text)
                    .bg(self.ui_theme.selection_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.ui_theme.text_body)
            };
            let hint_style = if is_cursor {
                Style::default()
                    .fg(self.ui_theme.selection_text)
                    .bg(self.ui_theme.selection_bg)
            } else {
                Style::default().fg(self.ui_theme.text_muted)
            };
            let pointer = if is_cursor { ">" } else { " " };

            lines.push(Line::from(vec![
                Span::styled(
                    format!("{pointer} {}. {:<7}", row.number, row.name),
                    row_style,
                ),
                Span::styled(row.hint, hint_style),
            ]));
        }

        Paragraph::new(lines).render(inner, buf);
    }
}

fn render_ascii_mode_picker_chrome(area: Rect, buf: &mut Buffer, theme: UiTheme) -> Rect {
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
        let title = ascii_prefix(" Mode ", area.width.saturating_sub(4) as usize);
        buf.set_string(area.x + 2, area.y, &title, title_style);
    }
    if area.width > 8 && area.height > 1 {
        let hint = ascii_prefix(
            " Up/Down move Enter select Esc cancel ",
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
    use crossterm::event::KeyModifiers;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn opens_on_current_mode() {
        let view = ModePickerView::new(AppMode::Plan);
        assert_eq!(view.selected_mode(), AppMode::Plan);
    }

    #[test]
    fn enter_emits_selected_mode() {
        let mut view = ModePickerView::new(AppMode::Agent);
        view.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let action = view.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        match action {
            ViewAction::EmitAndClose(ViewEvent::ModeSelected { mode }) => {
                assert_eq!(mode, AppMode::Plan);
            }
            other => panic!("expected ModeSelected, got {other:?}"),
        }
    }

    #[test]
    fn number_keys_select_modes() {
        let mut view = ModePickerView::new(AppMode::Agent);
        let action = view.handle_key(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE));
        match action {
            ViewAction::EmitAndClose(ViewEvent::ModeSelected { mode }) => {
                assert_eq!(mode, AppMode::Yolo);
            }
            other => panic!("expected ModeSelected, got {other:?}"),
        }
    }

    #[test]
    fn ascii_mode_picker_chrome_uses_plain_border_chars() {
        let area = Rect::new(1, 1, 20, 8);
        let mut buf = Buffer::empty(Rect::new(0, 0, 24, 12));
        let inner =
            render_ascii_mode_picker_chrome(area, &mut buf, palette::DEEPSEEK_SHELL_UI_THEME);

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
        assert_eq!(inner, Rect::new(area.x + 2, area.y + 2, 16, 4));
    }

    #[test]
    fn ascii_prefix_respects_cjk_display_width() {
        let prefix = ascii_prefix(" 模式选择 ", 8);

        assert!(
            UnicodeWidthStr::width(prefix.as_str()) <= 8,
            "prefix overflowed display width: {prefix:?}"
        );
        assert!(
            prefix.is_char_boundary(prefix.len()),
            "prefix must not split UTF-8 codepoints: {prefix:?}"
        );
    }
}
