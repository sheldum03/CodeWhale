//! `/statusline` multi-select picker.
//!
//! Mirrors codex-rs's `bottom_pane::status_line_setup` ergonomically: a
//! checklist of footer items the user can toggle on/off with Space (or
//! Enter), reordered by ↑/↓, applied immediately so the live footer
//! reflects every change. Enter saves to `~/.deepseek/config.toml` under
//! `tui.status_items`; Esc reverts to the snapshot taken on open.
//!
//! The picker enumerates [`StatusItem::all`] so adding a new variant in
//! `crates/tui/src/config.rs` automatically surfaces a new row here.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Widget},
};

use crate::config::{ApiProvider, StatusItem};
use crate::localization::truncate_to_width;
use crate::palette;
use crate::palette::UiTheme;
use crate::tui::views::{ModalKind, ModalView, ViewAction, ViewEvent};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Picker state. We hold both the user's working selection AND the original
/// snapshot so Esc can perfectly revert the live preview.
pub struct StatusPickerView {
    /// Every available item, in the order shown to the user. We keep this
    /// list ordered so toggles produce a stable on-screen layout that
    /// doesn't shuffle as items flip.
    rows: Vec<StatusItem>,
    /// Indices in `rows` currently checked on (the user's working set).
    selected: Vec<bool>,
    /// Highlighted row.
    cursor: usize,
    /// Snapshot of `app.status_items` at open time so Esc reverts cleanly.
    original: Vec<StatusItem>,
    ui_theme: UiTheme,
}

impl StatusPickerView {
    #[must_use]
    pub fn new(active: &[StatusItem], provider: ApiProvider) -> Self {
        let rows: Vec<StatusItem> = StatusItem::all()
            .iter()
            .filter(|item| item.is_available_for(provider))
            .copied()
            .collect();
        let selected: Vec<bool> = rows.iter().map(|item| active.contains(item)).collect();
        Self {
            rows,
            selected,
            cursor: 0,
            original: active.to_vec(),
            ui_theme: palette::UI_THEME,
        }
    }

    #[must_use]
    pub fn with_ui_theme(mut self, ui_theme: UiTheme) -> Self {
        self.ui_theme = ui_theme;
        self
    }

    /// Build the current selection in the same order the user sees it.
    /// Preserves `StatusItem::all()` order so toggling produces deterministic
    /// `tui.status_items` output (no churn-induced diffs in config.toml).
    fn current_selection(&self) -> Vec<StatusItem> {
        self.rows
            .iter()
            .zip(self.selected.iter())
            .filter_map(|(item, on)| if *on { Some(*item) } else { None })
            .collect()
    }

    fn move_up(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        if self.cursor == 0 {
            self.cursor = self.rows.len() - 1;
        } else {
            self.cursor -= 1;
        }
    }

    fn move_down(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        self.cursor = (self.cursor + 1) % self.rows.len();
    }

    fn toggle_current(&mut self) {
        if let Some(slot) = self.selected.get_mut(self.cursor) {
            *slot = !*slot;
        }
    }

    fn live_preview_event(&self) -> ViewEvent {
        ViewEvent::StatusItemsUpdated {
            items: self.current_selection(),
            final_save: false,
        }
    }

    fn final_event(&self) -> ViewEvent {
        ViewEvent::StatusItemsUpdated {
            items: self.current_selection(),
            final_save: true,
        }
    }

    fn revert_event(&self) -> ViewEvent {
        ViewEvent::StatusItemsUpdated {
            items: self.original.clone(),
            final_save: false,
        }
    }
}

impl ModalView for StatusPickerView {
    fn kind(&self) -> ModalKind {
        ModalKind::StatusPicker
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Esc => {
                // Roll the live preview back to the snapshot so Esc means
                // "take me back to where I was."
                ViewAction::EmitAndClose(self.revert_event())
            }
            KeyCode::Enter => ViewAction::EmitAndClose(self.final_event()),
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_up();
                ViewAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_down();
                ViewAction::None
            }
            KeyCode::Char(' ') | KeyCode::Char('x') | KeyCode::Char('X') => {
                self.toggle_current();
                ViewAction::Emit(self.live_preview_event())
            }
            KeyCode::Char('a') | KeyCode::Char('A')
                if !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                // Quality-of-life: 'a' selects all so the user can quickly
                // see every chip available before paring back.
                for slot in &mut self.selected {
                    *slot = true;
                }
                ViewAction::Emit(self.live_preview_event())
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                // 'n' clears all so the user can build up from scratch.
                for slot in &mut self.selected {
                    *slot = false;
                }
                ViewAction::Emit(self.live_preview_event())
            }
            _ => ViewAction::None,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let popup_width = 64.min(area.width.saturating_sub(4)).max(40);
        // Two header lines + one row per StatusItem + one footer hint line.
        // When the full list is taller than the screen, cap the popup so it
        // stays on-screen and let the scroll offset handle overflow.
        let needed_height = (self.rows.len() as u16).saturating_add(4);
        let max_fit = area.height.saturating_sub(4).max(8);
        let popup_height = needed_height.min(max_fit);

        let popup_area = Rect {
            x: area.x + (area.width.saturating_sub(popup_width)) / 2,
            y: area.y + (area.height.saturating_sub(popup_height)) / 2,
            width: popup_width,
            height: popup_height,
        };

        Clear.render(popup_area, buf);

        let inner = if palette::ascii_ui_enabled() {
            render_ascii_status_picker_chrome(popup_area, buf, self.ui_theme)
        } else {
            let block = Block::default()
                .title(Line::from(Span::styled(
                    " Status line ",
                    Style::default()
                        .fg(self.ui_theme.accent_primary)
                        .add_modifier(Modifier::BOLD),
                )))
                .title_bottom(status_picker_footer_line(
                    " Space toggle a all n none Enter save Esc cancel ",
                    popup_area.width.saturating_sub(4) as usize,
                    self.ui_theme,
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.ui_theme.border))
                .style(Style::default().bg(self.ui_theme.surface_bg))
                .padding(Padding::uniform(1));

            let inner = block.inner(popup_area);
            block.render(popup_area, buf);
            inner
        };

        let visible_rows = inner.height.saturating_sub(2) as usize;
        let row_start = visible_row_start(self.rows.len(), self.cursor, visible_rows);

        let mut lines: Vec<Line> = Vec::with_capacity(visible_rows + 2);
        lines.push(Line::from(Span::styled(
            "Pick the chips you want in the footer:",
            Style::default().fg(self.ui_theme.text_muted),
        )));
        lines.push(Line::from(""));

        for (idx, item) in self
            .rows
            .iter()
            .enumerate()
            .skip(row_start)
            .take(visible_rows)
        {
            let checked = *self.selected.get(idx).unwrap_or(&false);
            let is_cursor = idx == self.cursor;
            let mark = status_mark(checked);

            let row_style = if is_cursor {
                Style::default()
                    .fg(self.ui_theme.selection_text)
                    .bg(self.ui_theme.selection_bg)
                    .add_modifier(Modifier::BOLD)
            } else if checked {
                Style::default().fg(self.ui_theme.text_body)
            } else {
                Style::default().fg(self.ui_theme.text_muted)
            };
            let pointer = status_pointer(is_cursor);

            if is_cursor {
                let selected_style = Style::default()
                    .fg(self.ui_theme.selection_text)
                    .bg(self.ui_theme.selection_bg)
                    .add_modifier(Modifier::BOLD);
                let line = status_row_text(pointer, mark, item, inner.width as usize);
                lines.push(Line::from(Span::styled(line, selected_style)));
            } else {
                let line = status_row_text(pointer, mark, item, inner.width as usize);
                lines.push(Line::from(Span::styled(line, row_style)));
            }
        }

        Paragraph::new(lines).render(inner, buf);
    }
}

fn visible_row_start(total_rows: usize, cursor: usize, visible_rows: usize) -> usize {
    if total_rows == 0 || visible_rows == 0 || total_rows <= visible_rows {
        return 0;
    }
    let max_start = total_rows - visible_rows;
    cursor
        .saturating_add(1)
        .saturating_sub(visible_rows)
        .min(max_start)
}

fn render_ascii_status_picker_chrome(area: Rect, buf: &mut Buffer, theme: UiTheme) -> Rect {
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
        let title = ascii_prefix(" Status line ", area.width.saturating_sub(4) as usize);
        buf.set_string(area.x + 2, area.y, &title, title_style);
    }
    if area.width > 8 && area.height > 1 {
        let hint = ascii_prefix(
            " Space toggle a all n none Enter save Esc cancel ",
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

fn status_picker_footer_line(footer: &str, max_width: usize, theme: UiTheme) -> Line<'static> {
    Line::from(Span::styled(
        ascii_prefix(footer, max_width),
        Style::default().fg(theme.text_muted),
    ))
}

fn status_mark(checked: bool) -> &'static str {
    status_mark_for_ascii(checked, palette::ascii_ui_enabled())
}

fn status_mark_for_ascii(checked: bool, ascii: bool) -> &'static str {
    match (checked, ascii) {
        (true, true) => "[x]",
        (true, false) => "[\u{2713}]",
        (false, _) => "[ ]",
    }
}

fn status_pointer(active: bool) -> &'static str {
    status_pointer_for_ascii(active, palette::ascii_ui_enabled())
}

fn status_pointer_for_ascii(active: bool, ascii: bool) -> &'static str {
    match (active, ascii) {
        (true, true) => ">",
        (true, false) => "\u{25B8}",
        (false, _) => " ",
    }
}

fn status_row_text(pointer: &str, mark: &str, item: &StatusItem, width: usize) -> String {
    let text = crate::commands::command_text_with_ascii_fallback(format!(
        " {pointer} {mark} {}  ({})",
        item.label(),
        item.hint()
    ));
    let mut text = truncate_to_width(&text, width);
    let current_width = text.width();
    if current_width < width {
        text.push_str(&" ".repeat(width - current_width));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::EnvVarGuard;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn opens_with_active_items_pre_selected() {
        let active = StatusItem::default_footer();
        let view = StatusPickerView::new(&active, ApiProvider::Deepseek);
        assert_eq!(view.current_selection(), active);
    }

    #[test]
    fn space_toggles_current_row_and_emits_live_preview() {
        let active = StatusItem::default_footer();
        let mut view = StatusPickerView::new(&active, ApiProvider::Deepseek);
        // Cursor starts at row 0 = StatusItem::Mode (currently checked).
        let action = view.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        match action {
            ViewAction::Emit(ViewEvent::StatusItemsUpdated { items, final_save }) => {
                assert!(!final_save);
                assert!(!items.contains(&StatusItem::Mode));
            }
            other => panic!("expected live preview emit, got {other:?}"),
        }
    }

    #[test]
    fn enter_emits_final_save() {
        let active = StatusItem::default_footer();
        let mut view = StatusPickerView::new(&active, ApiProvider::Deepseek);
        let action = view.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        match action {
            ViewAction::EmitAndClose(ViewEvent::StatusItemsUpdated { final_save, .. }) => {
                assert!(final_save);
            }
            other => panic!("expected final save EmitAndClose, got {other:?}"),
        }
    }

    #[test]
    fn esc_reverts_to_snapshot() {
        let active = StatusItem::default_footer();
        let mut view = StatusPickerView::new(&active, ApiProvider::Deepseek);
        // Toggle a few items off so the working set diverges from snapshot.
        view.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        view.move_down();
        view.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        let action = view.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        match action {
            ViewAction::EmitAndClose(ViewEvent::StatusItemsUpdated { items, final_save }) => {
                assert!(!final_save);
                assert_eq!(items, active);
            }
            other => panic!("expected revert EmitAndClose, got {other:?}"),
        }
    }

    #[test]
    fn select_all_and_select_none_keys_work() {
        let active: Vec<StatusItem> = Vec::new();
        let mut view = StatusPickerView::new(&active, ApiProvider::Deepseek);
        let action = view.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        match action {
            ViewAction::Emit(ViewEvent::StatusItemsUpdated { items, .. }) => {
                assert_eq!(items.len(), StatusItem::all().len());
            }
            other => panic!("expected select-all emit, got {other:?}"),
        }
        let action = view.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        match action {
            ViewAction::Emit(ViewEvent::StatusItemsUpdated { items, .. }) => {
                assert!(items.is_empty());
            }
            other => panic!("expected select-none emit, got {other:?}"),
        }
    }

    #[test]
    fn arrow_keys_wrap_cursor_at_edges() {
        let active = StatusItem::default_footer();
        let mut view = StatusPickerView::new(&active, ApiProvider::Deepseek);
        assert_eq!(view.cursor, 0);
        view.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(view.cursor, StatusItem::all().len() - 1);
        view.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(view.cursor, 0);
        view.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(view.cursor, 1);
        view.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(view.cursor, 0);
    }

    #[test]
    fn visible_row_start_keeps_cursor_in_view() {
        assert_eq!(visible_row_start(14, 0, 8), 0);
        assert_eq!(visible_row_start(14, 7, 8), 0);
        assert_eq!(visible_row_start(14, 8, 8), 1);
        assert_eq!(visible_row_start(14, 13, 8), 6);
    }

    #[test]
    fn selected_row_text_fills_available_width() {
        let text = status_row_text(
            status_pointer_for_ascii(true, false),
            "[ ]",
            &StatusItem::LastToolElapsed,
            40,
        );
        assert_eq!(text.width(), 40);
        assert!(text.starts_with(" \u{25B8} [ ] Last tool elapsed"));
    }

    #[test]
    fn inactive_row_text_fills_available_width() {
        let text = status_row_text(
            status_pointer_for_ascii(false, false),
            "[ ]",
            &StatusItem::LastToolElapsed,
            24,
        );
        assert_eq!(text.width(), 24);
        assert!(
            text.is_char_boundary(text.len()),
            "row must not split UTF-8 codepoints: {text:?}"
        );
    }

    #[test]
    fn status_picker_symbols_have_ascii_fallbacks() {
        assert_eq!(status_mark_for_ascii(true, true), "[x]");
        assert_eq!(status_mark_for_ascii(false, true), "[ ]");
        assert_eq!(status_pointer_for_ascii(true, true), ">");
        assert_eq!(status_pointer_for_ascii(false, true), " ");
        assert_eq!(status_mark_for_ascii(true, false), "[\u{2713}]");
        assert_eq!(status_pointer_for_ascii(true, false), "\u{25B8}");
    }

    #[test]
    fn status_row_text_applies_ascii_fallback_to_hint_text() {
        let _lock = crate::test_support::lock_test_env();
        let _ascii = EnvVarGuard::set("CODEWHALE_ASCII_UI", "1");

        let text = status_row_text(
            status_pointer_for_ascii(true, true),
            status_mark_for_ascii(true, true),
            &StatusItem::Mode,
            40,
        );

        assert!(text.contains("agent - yolo - plan"), "got: {text}");
        assert!(!text.contains('\u{00B7}'));
    }

    #[test]
    fn ascii_status_picker_chrome_uses_plain_border_chars() {
        let area = Rect::new(1, 1, 24, 8);
        let mut buf = Buffer::empty(Rect::new(0, 0, 28, 12));
        let inner =
            render_ascii_status_picker_chrome(area, &mut buf, palette::DEEPSEEK_SHELL_UI_THEME);

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
        let prefix = ascii_prefix(" 状态栏设置 ", 8);

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
    fn status_picker_footer_line_truncates_to_display_width() {
        let line = status_picker_footer_line(
            " Space toggle a all n none Enter save Esc cancel \u{4e0a}\u{4e0b}\u{79fb}\u{52a8} ",
            22,
            palette::DEEPSEEK_SHELL_UI_THEME,
        );
        let plain = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(UnicodeWidthStr::width(plain.as_str()) <= 22);
        assert!(plain.is_char_boundary(plain.len()));
    }

    #[test]
    fn balance_excluded_for_non_deepseek_provider() {
        let active = StatusItem::default_footer();
        let view = StatusPickerView::new(&active, ApiProvider::Openrouter);
        // Balance should not appear as a row for non-DeepSeek providers.
        assert!(!view.rows.contains(&StatusItem::Balance));
        // Mode should still be present.
        assert!(view.rows.contains(&StatusItem::Mode));
    }
}
