//! `/theme` picker with live preview.
//!
//! Modeled after `feedback_picker`. Differences:
//! - The option list comes from `palette::SELECTABLE_THEMES`.
//! - Up/Down emit a `ConfigUpdated{persist:false}` so the host swaps
//!   `app.ui_theme` immediately and the whole TUI re-paints under the
//!   modal — the user sees the candidate theme before committing.
//! - Enter persists (`persist:true`); Esc emits one more
//!   `ConfigUpdated{persist:false}` to restore the original theme name
//!   that was active when the picker opened.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Widget},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::palette::{self, SELECTABLE_THEMES, ThemeId, UiTheme};
use crate::tui::views::{ModalKind, ModalView, ViewAction, ViewEvent};

fn theme_picker_nav_hint() -> &'static str {
    if palette::ascii_ui_enabled() {
        " Up/Down "
    } else {
        " \u{2191}/\u{2193} "
    }
}

fn theme_picker_intro() -> &'static str {
    if palette::ascii_ui_enabled() {
        "Pick a theme - preview is live; Enter saves to settings.toml."
    } else {
        "Pick a theme \u{2014} preview is live; Enter saves to settings.toml."
    }
}

pub struct ThemePickerView {
    selected: usize,
    /// Settings name of the theme that was active when the picker opened.
    /// Used to revert on Esc.
    original_name: String,
    /// Cached UiTheme for `ThemeId::System`, captured once at construction
    /// so the per-frame render doesn't re-invoke `UiTheme::detect()` (which
    /// reads `COLORFGBG`) on every keystroke.
    system_ui_theme: UiTheme,
    /// Runtime `background_color` overlay inferred from the theme that was
    /// active when the picker opened. The live ConfigUpdated preview keeps
    /// this overlay when swapping candidate themes, so the picker should too.
    background_override: Option<Color>,
}

impl ThemePickerView {
    #[must_use]
    pub fn new(original_name: String) -> Self {
        let system_ui_theme = UiTheme::detect();
        let original_id = ThemeId::from_name(&original_name).unwrap_or(ThemeId::System);
        let original_ui_theme = if matches!(original_id, ThemeId::System) {
            system_ui_theme
        } else {
            original_id.ui_theme()
        };
        Self::new_with_ui_theme_and_system(original_name, original_ui_theme, system_ui_theme)
    }

    #[must_use]
    pub fn new_with_ui_theme(original_name: String, original_ui_theme: UiTheme) -> Self {
        Self::new_with_ui_theme_and_system(original_name, original_ui_theme, UiTheme::detect())
    }

    fn new_with_ui_theme_and_system(
        original_name: String,
        original_ui_theme: UiTheme,
        system_ui_theme: UiTheme,
    ) -> Self {
        let normalized = original_name.trim().to_ascii_lowercase();
        let original_id = ThemeId::from_name(&normalized).unwrap_or(ThemeId::System);
        let base_original = if matches!(original_id, ThemeId::System) {
            system_ui_theme
        } else {
            original_id.ui_theme()
        };
        let background_override = runtime_background_override(base_original, original_ui_theme);
        // If the persisted name or alias resolves to one of the entries, start
        // there; otherwise fall back to "System" so the cursor lands on a valid row.
        let selected = SELECTABLE_THEMES
            .iter()
            .position(|id| *id == original_id)
            .unwrap_or(0);
        Self {
            selected,
            original_name,
            system_ui_theme,
            background_override,
        }
    }

    fn current(&self) -> ThemeId {
        SELECTABLE_THEMES
            .get(self.selected)
            .copied()
            .unwrap_or(ThemeId::System)
    }

    /// Resolve a theme to a `UiTheme`, returning the cached `System`
    /// resolution to avoid repeated env-var reads inside `render`.
    fn ui_theme_for(&self, id: ThemeId) -> UiTheme {
        let theme = if matches!(id, ThemeId::System) {
            self.system_ui_theme
        } else {
            id.ui_theme()
        };
        if let Some(background) = self.background_override {
            theme.with_background_color(background)
        } else {
            theme
        }
    }

    fn preview_event(&self) -> ViewAction {
        ViewAction::Emit(ViewEvent::ConfigUpdated {
            key: "theme".to_string(),
            value: self.current().name().to_string(),
            persist: false,
        })
    }

    fn commit_event(&self) -> ViewAction {
        ViewAction::EmitAndClose(ViewEvent::ConfigUpdated {
            key: "theme".to_string(),
            value: self.current().name().to_string(),
            persist: true,
        })
    }

    fn revert_event(&self) -> ViewAction {
        ViewAction::EmitAndClose(ViewEvent::ConfigUpdated {
            key: "theme".to_string(),
            value: self.original_name.clone(),
            persist: false,
        })
    }

    fn move_up(&mut self) {
        self.selected = (self.selected + SELECTABLE_THEMES.len() - 1) % SELECTABLE_THEMES.len();
    }

    fn move_down(&mut self) {
        self.selected = (self.selected + 1) % SELECTABLE_THEMES.len();
    }
}

impl ModalView for ThemePickerView {
    fn kind(&self) -> ModalKind {
        ModalKind::ThemePicker
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Esc => self.revert_event(),
            KeyCode::Enter => self.commit_event(),
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_up();
                self.preview_event()
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_down();
                self.preview_event()
            }
            KeyCode::Home => {
                self.selected = 0;
                self.preview_event()
            }
            KeyCode::End => {
                self.selected = SELECTABLE_THEMES.len().saturating_sub(1);
                self.preview_event()
            }
            // Number shortcuts: '1'..='9' jump to that row (1-indexed).
            // '0' is rejected explicitly — saturating_sub would otherwise
            // collapse it onto row 0, which is unintuitive.
            KeyCode::Char(c)
                if matches!(c, '1'..='9')
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                let idx = (c as usize) - ('1' as usize);
                if idx < SELECTABLE_THEMES.len() {
                    self.selected = idx;
                    self.preview_event()
                } else {
                    ViewAction::None
                }
            }
            _ => ViewAction::None,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        // Modal must always fit inside `area`. The old `.max(52) / .max(10)`
        // floors could produce dimensions larger than the available area on
        // very small terminals (or split-pane setups), which then made the
        // centering arithmetic underflow and ratatui assert. Take a
        // soft-preferred size and clamp it strictly to `area`.
        let popup_width = 78u16.min(area.width.saturating_sub(4));
        // 1 title + 1 spacer + N rows + spacer + bottom hint
        let needed_height = (SELECTABLE_THEMES.len() as u16).saturating_add(9);
        let popup_height = needed_height.min(area.height.saturating_sub(4));

        if popup_width == 0 || popup_height == 0 {
            // Nothing sensible to draw — the host's caller has already
            // cleared the area, so we just return.
            return;
        }

        let popup_area = Rect {
            x: area.x + (area.width.saturating_sub(popup_width)) / 2,
            y: area.y + (area.height.saturating_sub(popup_height)) / 2,
            width: popup_width,
            height: popup_height,
        };

        // The live theme has already been swapped under us via ConfigUpdated,
        // so we pull the *current* preview's UiTheme from the cursor row to
        // skin the modal chrome. That way the popup itself shifts color as
        // the cursor moves, matching what the background will look like
        // after Enter.
        let live = self.ui_theme_for(self.current());

        Clear.render(popup_area, buf);

        let inner = if palette::ascii_ui_enabled() {
            render_ascii_theme_picker_chrome(popup_area, buf, live)
        } else {
            let block = Block::default()
                .title(Line::from(Span::styled(
                    " Theme ",
                    Style::default()
                        .fg(live.status_working)
                        .add_modifier(Modifier::BOLD),
                )))
                .title_bottom(Line::from(vec![
                    Span::styled(theme_picker_nav_hint(), Style::default().fg(live.text_muted)),
                    Span::raw("preview "),
                    Span::styled(" Enter ", Style::default().fg(live.text_muted)),
                    Span::raw("save "),
                    Span::styled(" Esc ", Style::default().fg(live.text_muted)),
                    Span::raw("revert "),
                ]))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(live.border))
                .style(Style::default().bg(live.surface_bg))
                .padding(Padding::uniform(1));

            let inner = block.inner(popup_area);
            block.render(popup_area, buf);
            inner
        };

        let mut lines: Vec<Line> = Vec::with_capacity(SELECTABLE_THEMES.len() + 5);
        lines.push(Line::from(Span::styled(
            theme_picker_intro(),
            Style::default().fg(live.text_muted),
        )));
        lines.push(Line::from(""));

        for (idx, id) in SELECTABLE_THEMES.iter().enumerate() {
            let id = *id;
            let is_selected = idx == self.selected;
            let row_style = if is_selected {
                Style::default()
                    .fg(live.selection_text)
                    .bg(live.selection_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(live.text_body)
            };
            let tagline_style = if is_selected {
                Style::default()
                    .fg(live.selection_text)
                    .bg(live.selection_bg)
            } else {
                Style::default().fg(live.text_dim)
            };
            let number_style = if is_selected {
                Style::default()
                    .fg(live.selection_text)
                    .bg(live.selection_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(live.text_hint)
            };
            let pointer = theme_picker_pointer(is_selected);

            // 3-cell color swatch per row using the candidate theme's own
            // accent + panel + border colors so the picker doubles as a
            // legend. Use the cached resolver so `System` doesn't repeat
            // `UiTheme::detect()`.
            let row_theme = self.ui_theme_for(id);
            let swatch = vec![
                Span::styled("  ", Style::default().bg(row_theme.surface_bg)),
                Span::styled("  ", Style::default().bg(row_theme.panel_bg)),
                Span::styled("  ", Style::default().bg(row_theme.status_working)),
                Span::styled("  ", Style::default().bg(row_theme.mode_yolo)),
                Span::styled("  ", Style::default().bg(row_theme.mode_plan)),
            ];

            let mut spans: Vec<Span> = Vec::with_capacity(8);
            spans.push(Span::styled(format!(" {pointer} "), row_style));
            spans.push(Span::styled(format!("{}. ", idx + 1), number_style));
            spans.push(Span::styled(pad_display_width(id.display_name(), 22), row_style));
            spans.extend(swatch);
            spans.push(Span::raw("  "));
            spans.push(Span::styled(id.tagline(), tagline_style));

            lines.push(Line::from(spans));
        }

        Paragraph::new(lines).render(inner, buf);
    }
}

fn theme_picker_pointer(is_selected: bool) -> &'static str {
    theme_picker_pointer_with_ascii(is_selected, crate::palette::ascii_ui_enabled())
}

fn runtime_background_override(base: UiTheme, runtime: UiTheme) -> Option<Color> {
    if runtime.surface_bg != base.surface_bg
        && runtime.header_bg == runtime.surface_bg
        && runtime.footer_bg == runtime.surface_bg
    {
        Some(runtime.surface_bg)
    } else {
        None
    }
}

fn render_ascii_theme_picker_chrome(area: Rect, buf: &mut Buffer, theme: UiTheme) -> Rect {
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
        .fg(theme.status_working)
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
        let title = ascii_prefix(" Theme ", area.width.saturating_sub(4) as usize);
        buf.set_string(area.x + 2, area.y, &title, title_style);
    }
    if area.width > 8 && area.height > 1 {
        let hint = ascii_prefix(
            " Up/Down preview Enter save Esc revert ",
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

fn pad_display_width(text: &str, width: usize) -> String {
    let mut out = ascii_prefix(text, width);
    let out_width = UnicodeWidthStr::width(out.as_str());
    if out_width < width {
        out.push_str(&" ".repeat(width - out_width));
    }
    out
}

fn theme_picker_pointer_with_ascii(is_selected: bool, ascii: bool) -> &'static str {
    if !is_selected {
        return " ";
    }
    if ascii {
        ">"
    } else {
        "\u{25B6}"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn selected_name(action: &ViewAction) -> Option<&str> {
        match action {
            ViewAction::Emit(ViewEvent::ConfigUpdated { key, value, .. })
            | ViewAction::EmitAndClose(ViewEvent::ConfigUpdated { key, value, .. })
                if key == "theme" =>
            {
                Some(value.as_str())
            }
            _ => None,
        }
    }

    #[test]
    fn opens_at_persisted_theme() {
        let v = ThemePickerView::new("tokyo-night".to_string());
        assert_eq!(v.current(), ThemeId::TokyoNight);
    }

    #[test]
    fn opens_at_deepseek_shell_theme() {
        let v = ThemePickerView::new("deepseek-shell".to_string());
        assert_eq!(v.current(), ThemeId::DeepSeekShell);
    }

    #[test]
    fn opens_at_deepseek_shell_theme_alias() {
        let v = ThemePickerView::new("ds-shell".to_string());
        assert_eq!(v.current(), ThemeId::DeepSeekShell);
    }

    #[test]
    fn unknown_persisted_name_falls_back_to_first_row() {
        let v = ThemePickerView::new("not-a-real-theme".to_string());
        assert_eq!(v.selected, 0);
        assert_eq!(v.current(), ThemeId::System);
    }

    #[test]
    fn arrow_down_previews_next_theme() {
        let mut v = ThemePickerView::new("system".to_string());
        let action = v.handle_key(key(KeyCode::Down));
        assert!(matches!(action, ViewAction::Emit(_)));
        assert_eq!(selected_name(&action), Some(ThemeId::Terminal.name()));
    }

    #[test]
    fn arrow_navigation_wraps_at_picker_edges() {
        let mut v = ThemePickerView::new("system".to_string());
        let last = SELECTABLE_THEMES.last().unwrap();

        let action = v.handle_key(key(KeyCode::Up));
        assert_eq!(selected_name(&action), Some(last.name()));

        let action = v.handle_key(key(KeyCode::Down));
        assert_eq!(selected_name(&action), Some(SELECTABLE_THEMES[0].name()));
    }

    #[test]
    fn enter_commits_with_persist_true() {
        let mut v = ThemePickerView::new("system".to_string());
        v.handle_key(key(KeyCode::Down));
        v.handle_key(key(KeyCode::Down));
        v.handle_key(key(KeyCode::Down));
        v.handle_key(key(KeyCode::Down));
        v.handle_key(key(KeyCode::Down)); // -> CatppuccinMocha
        let action = v.handle_key(key(KeyCode::Enter));
        match action {
            ViewAction::EmitAndClose(ViewEvent::ConfigUpdated {
                key,
                value,
                persist,
            }) => {
                assert_eq!(key, "theme");
                assert_eq!(value, ThemeId::CatppuccinMocha.name());
                assert!(persist);
            }
            other => panic!("expected commit, got {other:?}"),
        }
    }

    #[test]
    fn esc_reverts_to_original() {
        let mut v = ThemePickerView::new("dracula".to_string());
        v.handle_key(key(KeyCode::Up));
        v.handle_key(key(KeyCode::Up));
        let action = v.handle_key(key(KeyCode::Esc));
        match action {
            ViewAction::EmitAndClose(ViewEvent::ConfigUpdated {
                key,
                value,
                persist,
            }) => {
                assert_eq!(key, "theme");
                assert_eq!(value, "dracula");
                assert!(!persist);
            }
            other => panic!("expected revert, got {other:?}"),
        }
    }

    #[test]
    fn digit_jumps_to_row() {
        let mut v = ThemePickerView::new("system".to_string());
        let action = v.handle_key(key(KeyCode::Char('6')));
        // Row 6 (1-indexed) -> index 5 -> CatppuccinMocha
        assert_eq!(
            selected_name(&action),
            Some(ThemeId::CatppuccinMocha.name())
        );
    }

    #[test]
    fn digit_zero_is_rejected_not_remapped_to_row_zero() {
        let mut v = ThemePickerView::new("dracula".to_string());
        let before = v.selected;
        let action = v.handle_key(key(KeyCode::Char('0')));
        assert!(matches!(action, ViewAction::None));
        assert_eq!(v.selected, before, "'0' should not move the cursor");
    }

    #[test]
    fn render_lists_deepseek_shell_theme() {
        let v = ThemePickerView::new("deepseek-shell".to_string());
        let area = ratatui::layout::Rect::new(0, 0, 96, 24);
        let mut buf = ratatui::buffer::Buffer::empty(area);

        v.render(area, &mut buf);

        let mut rendered = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                rendered.push_str(buf[(x, y)].symbol());
            }
        }
        assert!(rendered.contains("DeepSeek Shell"), "{rendered:?}");
        assert!(rendered.contains("Deep terminal shell"), "{rendered:?}");
    }

    #[test]
    fn selected_theme_row_uses_selection_text_token() {
        let mut theme = palette::UI_THEME;
        theme.text_body = Color::Indexed(100);
        theme.text_muted = Color::Indexed(101);
        theme.status_working = Color::Indexed(102);
        theme.selection_text = Color::Indexed(231);
        theme.selection_bg = Color::Indexed(24);
        let v =
            ThemePickerView::new_with_ui_theme_and_system("system".to_string(), theme, theme);
        let area = ratatui::layout::Rect::new(0, 0, 96, 24);
        let mut buf = ratatui::buffer::Buffer::empty(area);

        v.render(area, &mut buf);

        let selected_cells = buf
            .content()
            .iter()
            .filter(|cell| cell.bg == theme.selection_bg)
            .collect::<Vec<_>>();
        assert!(
            selected_cells
                .iter()
                .any(|cell| cell.fg == theme.selection_text),
            "selected row should use selection_text foreground"
        );
        assert!(
            selected_cells
                .iter()
                .all(|cell| cell.fg != theme.text_body
                    && cell.fg != theme.text_muted
                    && cell.fg != theme.status_working),
            "selected row should not mix ordinary text/status foregrounds"
        );
    }

    #[test]
    fn theme_preview_preserves_runtime_background_overlay() {
        let custom_bg = ratatui::style::Color::Rgb(1, 2, 3);
        let runtime_theme = palette::DEEPSEEK_SHELL_UI_THEME.with_background_color(custom_bg);
        let v = ThemePickerView::new_with_ui_theme("deepseek-shell".to_string(), runtime_theme);

        assert_eq!(v.ui_theme_for(ThemeId::DeepSeekShell).surface_bg, custom_bg);
        assert_eq!(v.ui_theme_for(ThemeId::Dracula).surface_bg, custom_bg);
    }

    #[test]
    fn theme_preview_without_runtime_background_overlay_uses_candidate_defaults() {
        let v = ThemePickerView::new_with_ui_theme(
            "deepseek-shell".to_string(),
            palette::DEEPSEEK_SHELL_UI_THEME,
        );

        assert_eq!(
            v.ui_theme_for(ThemeId::Dracula).surface_bg,
            palette::DRACULA_UI_THEME.surface_bg
        );
    }

    #[test]
    fn theme_picker_pointer_has_ascii_fallback() {
        assert_eq!(theme_picker_pointer_with_ascii(false, true), " ");
        assert_eq!(theme_picker_pointer_with_ascii(true, true), ">");
        assert_eq!(theme_picker_pointer_with_ascii(true, false), "\u{25B6}");
    }

    #[test]
    fn theme_picker_intro_uses_ascii_separator_when_enabled() {
        crate::palette::set_ascii_ui_for_tests(Some(false));
        assert!(theme_picker_intro().contains('\u{2014}'));

        crate::palette::set_ascii_ui_for_tests(Some(true));
        assert_eq!(
            theme_picker_intro(),
            "Pick a theme - preview is live; Enter saves to settings.toml."
        );
        assert!(!theme_picker_intro().contains('\u{2014}'));

        crate::palette::set_ascii_ui_for_tests(None);
    }

    #[test]
    fn ascii_theme_picker_chrome_uses_plain_border_chars() {
        let area = ratatui::layout::Rect::new(1, 1, 20, 8);
        let mut buf = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 24, 12));
        let inner =
            render_ascii_theme_picker_chrome(area, &mut buf, palette::DEEPSEEK_SHELL_UI_THEME);

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
            ratatui::layout::Rect::new(area.x + 2, area.y + 2, area.width - 4, area.height - 4)
        );
    }

    #[test]
    fn ascii_prefix_respects_cjk_display_width() {
        let prefix = ascii_prefix(" 主题选择 ", 8);

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
    fn theme_name_padding_uses_display_width() {
        let padded = pad_display_width("\u{4e3b}\u{9898}\u{9009}\u{62e9}", 10);

        assert_eq!(UnicodeWidthStr::width(padded.as_str()), 10);
        assert!(
            padded.starts_with("\u{4e3b}\u{9898}\u{9009}\u{62e9}"),
            "padded theme name should preserve visible text: {padded:?}"
        );
    }

    #[test]
    fn render_does_not_panic_on_zero_sized_area() {
        // The picker historically panicked here via .max(W).max(H) floors
        // that produced dimensions larger than the available area, then
        // underflowed the centering arithmetic.
        let v = ThemePickerView::new("system".to_string());
        let outer = ratatui::layout::Rect::new(0, 0, 10, 10);
        let area = ratatui::layout::Rect::new(0, 0, 0, 0);
        let mut buf = ratatui::buffer::Buffer::empty(outer);
        v.render(area, &mut buf);
    }

    #[test]
    fn render_does_not_panic_on_tiny_area() {
        // 20×6 is smaller than every soft floor the picker prefers.
        let v = ThemePickerView::new("system".to_string());
        let area = ratatui::layout::Rect::new(0, 0, 20, 6);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        v.render(area, &mut buf);
    }
}
