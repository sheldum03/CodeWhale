//! Right-click context menu for mouse-captured TUI sessions.

use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Widget},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::palette::{self, UiTheme};
use crate::tui::views::{ContextMenuAction, ModalKind, ModalView, ViewAction, ViewEvent};

#[derive(Debug, Clone)]
pub struct ContextMenuEntry {
    pub label: String,
    pub description: String,
    pub action: ContextMenuAction,
}

pub struct ContextMenuView {
    entries: Vec<ContextMenuEntry>,
    selected: usize,
    column: u16,
    row: u16,
    last_rect: Cell<Option<Rect>>,
    title: String,
    ui_theme: UiTheme,
}

impl ContextMenuView {
    pub fn new(entries: Vec<ContextMenuEntry>, column: u16, row: u16, title: String) -> Self {
        Self {
            entries,
            selected: 0,
            column,
            row,
            last_rect: Cell::new(None),
            title,
            ui_theme: palette::UI_THEME,
        }
    }

    pub fn with_ui_theme(mut self, ui_theme: UiTheme) -> Self {
        self.ui_theme = ui_theme;
        self
    }

    fn selected_action(&self) -> Option<ContextMenuAction> {
        self.entries
            .get(self.selected)
            .map(|entry| entry.action.clone())
    }

    fn move_selection(&mut self, delta: isize) {
        if self.entries.is_empty() {
            self.selected = 0;
            return;
        }
        let max = self.entries.len().saturating_sub(1) as isize;
        self.selected = (self.selected as isize + delta).clamp(0, max) as usize;
    }

    fn menu_width(&self, area_width: u16) -> u16 {
        let widest = self
            .entries
            .iter()
            .map(|entry| {
                UnicodeWidthStr::width(entry.label.as_str())
                    + UnicodeWidthStr::width(entry.description.as_str())
                    + 8
            })
            .max()
            .unwrap_or(20);
        let width = u16::try_from(widest.clamp(24, 64)).unwrap_or(64);
        width.min(area_width.max(1))
    }

    fn menu_rect(&self, area: Rect) -> Rect {
        let width = self.menu_width(area.width);
        let desired_height =
            u16::try_from(self.entries.len().saturating_add(2)).unwrap_or(u16::MAX);
        let height = desired_height.min(area.height.max(1));
        let max_x = area.right().saturating_sub(width).max(area.x);
        let max_y = area.bottom().saturating_sub(height).max(area.y);
        let x = self.column.max(area.x).min(max_x);
        let y = self.row.max(area.y).min(max_y);
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    fn clicked_entry(&self, mouse: MouseEvent) -> Option<usize> {
        let rect = self.last_rect.get()?;
        if mouse.column <= rect.x
            || mouse.column >= rect.right().saturating_sub(1)
            || mouse.row <= rect.y
            || mouse.row >= rect.bottom().saturating_sub(1)
        {
            return None;
        }
        let idx = mouse.row.saturating_sub(rect.y + 1) as usize;
        (idx < self.entries.len()).then_some(idx)
    }
}

impl ModalView for ContextMenuView {
    fn kind(&self) -> ModalKind {
        ModalKind::ContextMenu
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => ViewAction::Close,
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection(-1);
                ViewAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection(1);
                ViewAction::None
            }
            KeyCode::Enter => self.selected_action().map_or(ViewAction::Close, |action| {
                ViewAction::EmitAndClose(ViewEvent::ContextMenuSelected { action })
            }),
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let idx = c.to_digit(10).and_then(|digit| {
                    let digit = usize::try_from(digit).ok()?;
                    digit.checked_sub(1)
                });
                if let Some(idx) = idx.filter(|idx| *idx < self.entries.len()) {
                    self.selected = idx;
                    return self.selected_action().map_or(ViewAction::Close, |action| {
                        ViewAction::EmitAndClose(ViewEvent::ContextMenuSelected { action })
                    });
                }
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> ViewAction {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(idx) = self.clicked_entry(mouse) {
                    self.selected = idx;
                    return self.selected_action().map_or(ViewAction::Close, |action| {
                        ViewAction::EmitAndClose(ViewEvent::ContextMenuSelected { action })
                    });
                }
                ViewAction::Close
            }
            MouseEventKind::Down(MouseButton::Right) => ViewAction::Close,
            MouseEventKind::ScrollUp => {
                self.move_selection(-1);
                ViewAction::None
            }
            MouseEventKind::ScrollDown => {
                self.move_selection(1);
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let menu_area = self.menu_rect(area);
        self.last_rect.set(Some(menu_area));
        Clear.render(menu_area, buf);

        let inner_width = menu_area.width.saturating_sub(2) as usize;
        let lines = self
            .entries
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                let label = format!("{} {}", idx + 1, entry.label);
                let description = if entry.description.trim().is_empty() {
                    String::new()
                } else {
                    format!(" - {}", entry.description)
                };
                let text = trim_to_width(&format!("{label}{description}"), inner_width);
                let style = if idx == self.selected {
                    Style::default()
                        .fg(self.ui_theme.selection_text)
                        .bg(self.ui_theme.selection_bg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(self.ui_theme.text_soft)
                        .bg(self.ui_theme.elevated_bg)
                };
                Line::from(Span::styled(text, style))
            })
            .collect::<Vec<_>>();

        let inner_area = if palette::ascii_ui_enabled() {
            render_ascii_context_menu_chrome(menu_area, buf, self.title.as_str(), self.ui_theme)
        } else {
            let title = trim_to_width(
                self.title.as_str(),
                menu_area.width.saturating_sub(4) as usize,
            );
            let block = Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.ui_theme.border))
                .style(Style::default().bg(self.ui_theme.elevated_bg))
                .padding(Padding::horizontal(0));
            let inner_area = block.inner(menu_area);
            block.render(menu_area, buf);
            inner_area
        };

        Paragraph::new(lines).render(inner_area, buf);
    }
}

fn render_ascii_context_menu_chrome(
    area: Rect,
    buf: &mut Buffer,
    title: &str,
    theme: UiTheme,
) -> Rect {
    if area.width == 0 || area.height == 0 {
        return Rect {
            x: area.x,
            y: area.y,
            width: 0,
            height: 0,
        };
    }

    let fill_style = Style::default().bg(theme.elevated_bg);
    let border_style = Style::default().fg(theme.border).bg(theme.elevated_bg);
    let title_style = Style::default()
        .fg(theme.text_soft)
        .bg(theme.elevated_bg)
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
        let title = trim_to_width(title, area.width.saturating_sub(4) as usize);
        buf.set_string(area.x + 2, area.y, &title, title_style);
    }

    Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

fn trim_to_width(text: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }
    if max_width <= 3 {
        return ".".repeat(max_width);
    }

    let limit = max_width.saturating_sub(3);
    let mut out = String::new();
    let mut width = 0usize;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > limit {
            break;
        }
        out.push(ch);
        width += ch_width;
    }
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;
    use ratatui::buffer::Buffer;

    use super::*;

    fn entry(label: &str, action: ContextMenuAction) -> ContextMenuEntry {
        ContextMenuEntry {
            label: label.to_string(),
            description: String::new(),
            action,
        }
    }

    #[test]
    fn enter_emits_selected_action() {
        let mut view = ContextMenuView::new(
            vec![
                entry("Paste", ContextMenuAction::Paste),
                entry("Help", ContextMenuAction::OpenHelp),
            ],
            5,
            5,
            " Right click ".to_string(),
        );

        view.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let action = view.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(matches!(
            action,
            ViewAction::EmitAndClose(ViewEvent::ContextMenuSelected {
                action: ContextMenuAction::OpenHelp
            })
        ));
    }

    #[test]
    fn menu_clamps_to_render_area() {
        let view = ContextMenuView::new(
            vec![entry("Paste", ContextMenuAction::Paste)],
            200,
            80,
            " Right click ".to_string(),
        );

        let rect = view.menu_rect(Rect {
            x: 0,
            y: 0,
            width: 40,
            height: 10,
        });

        assert!(rect.right() <= 40);
        assert!(rect.bottom() <= 10);
    }

    #[test]
    fn left_click_selects_rendered_entry() {
        let mut view = ContextMenuView::new(
            vec![
                entry("Paste", ContextMenuAction::Paste),
                entry("Help", ContextMenuAction::OpenHelp),
            ],
            2,
            2,
            " Right click ".to_string(),
        );
        let area = Rect {
            x: 0,
            y: 0,
            width: 40,
            height: 10,
        };
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        let action = view.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 4,
            row: 4,
            modifiers: KeyModifiers::NONE,
        });

        assert!(matches!(
            action,
            ViewAction::EmitAndClose(ViewEvent::ContextMenuSelected {
                action: ContextMenuAction::OpenHelp
            })
        ));
    }

    #[test]
    fn selected_entry_uses_injected_theme_colors() {
        let mut theme = palette::DEEPSEEK_SHELL_UI_THEME;
        theme.selection_bg = ratatui::style::Color::Indexed(24);
        theme.selection_text = ratatui::style::Color::Indexed(255);
        let view = ContextMenuView::new(
            vec![
                entry("Paste", ContextMenuAction::Paste),
                entry("Help", ContextMenuAction::OpenHelp),
            ],
            2,
            2,
            " Right click ".to_string(),
        )
        .with_ui_theme(theme);
        let area = Rect {
            x: 0,
            y: 0,
            width: 40,
            height: 10,
        };
        let mut buf = Buffer::empty(area);

        view.render(area, &mut buf);

        let selected_cell = buf
            .content()
            .iter()
            .find(|cell| cell.symbol() == "1")
            .expect("selected entry should render");
        assert_eq!(selected_cell.bg, theme.selection_bg);
        assert_eq!(selected_cell.fg, theme.selection_text);
    }

    #[test]
    fn context_menu_title_truncates_to_menu_width() {
        let title = format!(" {} ", "\u{4f1a}\u{8bdd}\u{5217}\u{8868}".repeat(12));
        let view = ContextMenuView::new(
            vec![entry("Paste", ContextMenuAction::Paste)],
            1,
            1,
            title,
        )
        .with_ui_theme(palette::DEEPSEEK_SHELL_UI_THEME);
        let area = Rect {
            x: 0,
            y: 0,
            width: 28,
            height: 8,
        };
        let mut buf = Buffer::empty(area);

        view.render(area, &mut buf);
        let menu = view.last_rect.get().expect("menu rect should be tracked");
        let mut title_row = String::new();
        for x in menu.left()..menu.right() {
            title_row.push_str(buf[(x, menu.y)].symbol());
        }

        assert!(UnicodeWidthStr::width(title_row.as_str()) <= usize::from(menu.width));
        assert!(
            title_row.is_char_boundary(title_row.len()),
            "context menu title must not split UTF-8 codepoints: {title_row:?}"
        );
    }

    #[test]
    fn ascii_context_menu_chrome_uses_plain_border_chars() {
        let area = Rect {
            x: 1,
            y: 1,
            width: 18,
            height: 6,
        };
        let mut buf = Buffer::empty(Rect {
            x: 0,
            y: 0,
            width: 22,
            height: 10,
        });
        let inner = render_ascii_context_menu_chrome(
            area,
            &mut buf,
            " Right click ",
            palette::DEEPSEEK_SHELL_UI_THEME,
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
        assert_eq!(
            inner,
            Rect {
                x: area.x + 1,
                y: area.y + 1,
                width: area.width - 2,
                height: area.height - 2,
            }
        );
    }

    #[test]
    fn trim_to_width_respects_cjk_display_width_for_tiny_widths() {
        for max_width in 0..=3 {
            let trimmed = trim_to_width("\u{4f1a}\u{8bdd}\u{5217}\u{8868}", max_width);

            assert!(
                unicode_width::UnicodeWidthStr::width(trimmed.as_str()) <= max_width,
                "trimmed text overflowed {max_width} columns: {trimmed:?}"
            );
            assert!(
                trimmed.is_char_boundary(trimmed.len()),
                "trimmed text must not split UTF-8 codepoints: {trimmed:?}"
            );
        }
    }
}
