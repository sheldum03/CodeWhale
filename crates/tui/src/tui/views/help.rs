//! Searchable help overlay for `?`, `F1`, and `Ctrl+/`.
//!
//! Renders two stacked sections — *Slash commands* and *Keybindings* — with
//! a live substring filter applied as the user types in the search box. The
//! command list is sourced from [`crate::commands::COMMANDS`] and the
//! keybinding list from [`crate::tui::keybindings::KEYBINDINGS`] so neither
//! can drift from the wired-up handlers.
//!
//! Keys: any printable character extends the filter, `Backspace` (or `Ctrl+H`)
//! shrinks it,
//! `↑`/`↓` (or `Ctrl+P`/`Ctrl+N`) move the selection, `PgUp`/`PgDn` jump by
//! ten rows, `Home`/`End` jump to ends, and `Esc` closes. Pressing `?` again
//! at the call-site (`tui::ui`) also toggles the overlay closed.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Widget},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::commands;
use crate::localization::{Locale, MessageId, tr};
use crate::palette;
use crate::palette::UiTheme;
use crate::tui::keybindings::{KEYBINDINGS, display_chord};
use crate::tui::views::{ModalKind, ModalView, ViewAction};

/// Two top-level sections rendered in the overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HelpSection {
    Command,
    Keybinding,
}

impl HelpSection {
    fn label(self, locale: Locale) -> &'static str {
        match self {
            Self::Command => tr(locale, MessageId::HelpSlashCommands),
            Self::Keybinding => tr(locale, MessageId::HelpKeybindings),
        }
    }

    /// Sort key — commands before keybindings keeps the most-used surface up
    /// top so an unfiltered overlay opens with the user's likely target in
    /// view without scrolling.
    fn rank(self) -> u8 {
        match self {
            Self::Command => 0,
            Self::Keybinding => 1,
        }
    }
}

#[derive(Debug, Clone)]
struct HelpEntry {
    section: HelpSection,
    /// Sort-within-section key — keybinding entries reuse their declared
    /// section's rank so the help overlay groups Navigation, Editing, … in
    /// the same order as `tui::keybindings`.
    sub_rank: u8,
    label: String,
    description: String,
    /// Lowercased haystack used for substring matching; pre-built so each
    /// keystroke does not re-allocate per entry.
    haystack: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HelpRenderRow {
    Section(HelpSection),
    Entry { slot: usize, entry_idx: usize },
}

pub struct HelpView {
    locale: Locale,
    entries: Vec<HelpEntry>,
    /// Indices into `entries`, in display order, after filtering.
    filtered: Vec<usize>,
    query: String,
    selected: usize,
    ui_theme: UiTheme,
}

impl Default for HelpView {
    fn default() -> Self {
        Self::new()
    }
}

impl HelpView {
    pub fn new() -> Self {
        Self::new_for_locale(Locale::En)
    }

    pub fn new_for_locale(locale: Locale) -> Self {
        let entries = build_entries(locale);
        let mut view = Self {
            locale,
            entries,
            filtered: Vec::new(),
            query: String::new(),
            selected: 0,
            ui_theme: palette::UI_THEME,
        };
        view.refilter();
        view
    }

    #[must_use]
    pub fn with_ui_theme(mut self, ui_theme: UiTheme) -> Self {
        self.ui_theme = ui_theme;
        self
    }

    fn tr(&self, id: MessageId) -> &'static str {
        tr(self.locale, id)
    }

    fn refilter(&mut self) {
        // Substring matching is intentional — fuzzy matchers can hide the
        // exact-prefix hit a user is typing toward, which is the wrong
        // failure mode for a *help* surface. We split on whitespace so
        // multi-term queries (`apply mode`) act as an AND.
        let query = self.query.trim().to_ascii_lowercase();
        let terms: Vec<&str> = query
            .split_whitespace()
            .filter(|term| !term.is_empty())
            .collect();

        let mut filtered: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| terms.iter().all(|term| entry.haystack.contains(term)))
            .map(|(idx, _)| idx)
            .collect();

        filtered.sort_by_key(|idx| {
            let entry = &self.entries[*idx];
            (entry.section.rank(), entry.sub_rank, entry.label.clone())
        });
        self.filtered = filtered;
        if self.selected >= self.filtered.len() {
            self.selected = self.filtered.len().saturating_sub(1);
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.filtered.is_empty() {
            self.selected = 0;
            return;
        }
        let len = self.filtered.len() as isize;
        let next = (self.selected as isize + delta).clamp(0, len - 1) as usize;
        self.selected = next;
    }

    fn move_selection_wrapping(&mut self, delta: isize) {
        if self.filtered.is_empty() {
            self.selected = 0;
            return;
        }
        let len = self.filtered.len() as isize;
        let next = (self.selected as isize + delta).rem_euclid(len) as usize;
        self.selected = next;
    }

    fn render_rows(&self) -> Vec<HelpRenderRow> {
        let mut rows = Vec::new();
        let mut active_section: Option<HelpSection> = None;

        for (slot, entry_idx) in self.filtered.iter().copied().enumerate() {
            let entry = &self.entries[entry_idx];
            if active_section != Some(entry.section) {
                rows.push(HelpRenderRow::Section(entry.section));
                active_section = Some(entry.section);
            }
            rows.push(HelpRenderRow::Entry { slot, entry_idx });
        }

        rows
    }

    fn selected_render_row(rows: &[HelpRenderRow], selected: usize) -> usize {
        rows.iter()
            .position(|row| matches!(row, HelpRenderRow::Entry { slot, .. } if *slot == selected))
            .unwrap_or(0)
    }

    fn visible_row_start(rows: &[HelpRenderRow], selected: usize, visible_budget: usize) -> usize {
        if rows.len() <= visible_budget {
            return 0;
        }

        let selected_row = Self::selected_render_row(rows, selected);
        let half = visible_budget / 2;
        if selected_row <= half {
            0
        } else if selected_row + half >= rows.len() {
            rows.len().saturating_sub(visible_budget)
        } else {
            selected_row.saturating_sub(half)
        }
    }
}

fn build_entries(locale: Locale) -> Vec<HelpEntry> {
    let mut entries = Vec::new();

    for command in commands::COMMANDS {
        let label = format!("/{}", command.name);
        let localized = command.description_for(locale);
        let description = if command.aliases.is_empty() {
            localized.to_string()
        } else {
            format!(
                "{}  (aliases: {})",
                localized,
                command
                    .aliases
                    .iter()
                    .map(|a| format!("/{a}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let haystack = format!(
            "{} {} {}",
            label.to_ascii_lowercase(),
            description.to_ascii_lowercase(),
            command.usage.to_ascii_lowercase()
        );
        entries.push(HelpEntry {
            section: HelpSection::Command,
            // Commands have no inherent ordering — fall back to alphabetical
            // by leaning on `label.clone()` in the final sort_by_key tuple.
            sub_rank: 0,
            label,
            description,
            haystack,
        });
    }

    for binding in KEYBINDINGS {
        let label = display_chord(binding.chord);
        let description = format!(
            "[{}] {}",
            binding.section.label(locale),
            tr(locale, binding.description_id)
        );
        let haystack = format!(
            "{} {}",
            label.to_ascii_lowercase(),
            description.to_ascii_lowercase()
        );
        entries.push(HelpEntry {
            section: HelpSection::Keybinding,
            sub_rank: binding.section.rank(),
            label,
            description,
            haystack,
        });
    }

    entries
}

fn modal_block(ui_theme: UiTheme) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ui_theme.border))
        .style(Style::default().bg(ui_theme.surface_bg))
        .padding(Padding::uniform(1))
}

fn truncate_to_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if text.width() <= max_width {
        return text.to_string();
    }
    let ellipsis = help_ellipsis(max_width);
    let ellipsis_width = ellipsis.width();
    let mut out = String::new();
    let limit = max_width.saturating_sub(ellipsis_width);
    for ch in text.chars() {
        let next_width = out.width() + ch.to_string().width();
        if next_width > limit {
            break;
        }
        out.push(ch);
    }
    out.push_str(ellipsis);
    out
}

fn pad_to_display_width(mut text: String, width: usize) -> String {
    let current = text.width();
    if current < width {
        text.push_str(&" ".repeat(width - current));
    }
    text
}

fn help_ellipsis(max_width: usize) -> &'static str {
    if palette::ascii_ui_enabled() {
        match max_width {
            0 => "",
            1 => ".",
            2 => "..",
            _ => "...",
        }
    } else {
        "\u{2026}"
    }
}

fn help_selected_prefix(selected: bool) -> &'static str {
    if !selected {
        "  "
    } else if palette::ascii_ui_enabled() {
        "> "
    } else {
        "\u{25B6} "
    }
}

impl ModalView for HelpView {
    fn kind(&self) -> ModalKind {
        ModalKind::Help
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Esc => ViewAction::Close,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                ViewAction::Close
            }
            KeyCode::Char('q') | KeyCode::Char('Q') if self.query.is_empty() => ViewAction::Close,
            KeyCode::Up => {
                self.move_selection_wrapping(-1);
                ViewAction::None
            }
            KeyCode::Down => {
                self.move_selection_wrapping(1);
                ViewAction::None
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_selection_wrapping(-1);
                ViewAction::None
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_selection_wrapping(1);
                ViewAction::None
            }
            KeyCode::PageUp => {
                self.move_selection(-10);
                ViewAction::None
            }
            KeyCode::PageDown => {
                self.move_selection(10);
                ViewAction::None
            }
            KeyCode::Home => {
                self.selected = 0;
                ViewAction::None
            }
            KeyCode::End => {
                if !self.filtered.is_empty() {
                    self.selected = self.filtered.len() - 1;
                }
                ViewAction::None
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.refilter();
                ViewAction::None
            }
            // Terminals where stty erase == ^H send Ctrl+H instead of
            // Backspace (DEL). Treat it identically so the filter input
            // works across all platforms (#958).
            KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.query.pop();
                self.refilter();
                ViewAction::None
            }
            KeyCode::Char(c)
                if !c.is_control()
                    && (key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT) =>
            {
                self.query.push(c);
                self.refilter();
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let popup_width = 90.min(area.width.saturating_sub(4));
        let popup_height = 28.min(area.height.saturating_sub(4));
        let popup_area = Rect {
            x: area.width.saturating_sub(popup_width) / 2,
            y: area.height.saturating_sub(popup_height) / 2,
            width: popup_width,
            height: popup_height,
        };

        Clear.render(popup_area, buf);

        let inner_width = popup_width.saturating_sub(4) as usize;
        let mut lines: Vec<Line<'static>> = Vec::new();

        let query_label = if self.query.is_empty() {
            self.tr(MessageId::HelpFilterPlaceholder).to_string()
        } else {
            format!("{}{}", self.tr(MessageId::HelpFilterPrefix), self.query)
        };
        let query_label = truncate_to_width(&query_label, inner_width);
        lines.push(Line::from(Span::styled(
            query_label,
            Style::default()
                .fg(self.ui_theme.accent_primary)
                .add_modifier(Modifier::BOLD),
        )));

        let match_count = if self.query.is_empty() {
            format!("{} entries", self.entries.len())
        } else {
            format!("{} / {} matches", self.filtered.len(), self.entries.len())
        };
        let match_count = truncate_to_width(&match_count, inner_width);
        lines.push(Line::from(Span::styled(
            match_count,
            Style::default()
                .fg(self.ui_theme.text_dim)
                .add_modifier(Modifier::ITALIC),
        )));
        lines.push(Line::from(""));

        if self.filtered.is_empty() {
            let no_matches = truncate_to_width(self.tr(MessageId::HelpNoMatches), inner_width);
            lines.push(Line::from(Span::styled(
                no_matches,
                Style::default()
                    .fg(self.ui_theme.text_muted)
                    .add_modifier(Modifier::ITALIC),
            )));
        } else {
            // The chord/label column takes up to 28 cols on wide screens;
            // descriptions fill the remainder. Borders and padding eat 4
            // cells from each side (border 1 + padding 1) × 2.
            let label_width = 28.min(inner_width.saturating_sub(8));
            let desc_capacity = inner_width.saturating_sub(label_width + 4);

            // The block uses a one-cell border plus one-cell padding, so the
            // real paragraph body is four rows shorter than the outer popup.
            // Budget against that body height so selected rows are not clipped
            // by the bottom border/padding.
            let header_lines = lines.len();
            let visible_budget = (popup_height as usize)
                .saturating_sub(4)
                .saturating_sub(header_lines)
                .max(1);

            let rows = self.render_rows();
            let row_start = Self::visible_row_start(&rows, self.selected, visible_budget);

            for row in rows.iter().skip(row_start).take(visible_budget) {
                match *row {
                    HelpRenderRow::Section(section) => {
                        let count = self
                            .filtered
                            .iter()
                            .filter(|idx| self.entries[**idx].section == section)
                            .count();
                        lines.push(Line::from(Span::styled(
                            format!("  {} ({})", section.label(self.locale), count),
                            Style::default()
                                .fg(self.ui_theme.accent_primary)
                                .add_modifier(Modifier::BOLD),
                        )));
                    }
                    HelpRenderRow::Entry { slot, entry_idx } => {
                        let entry = &self.entries[entry_idx];
                        let is_selected = slot == self.selected;
                        let style = if is_selected {
                            Style::default()
                                .fg(self.ui_theme.selection_text)
                                .bg(self.ui_theme.selection_bg)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(self.ui_theme.text_body)
                        };
                        let cursor = help_selected_prefix(is_selected);
                        let label = pad_to_display_width(
                            truncate_to_width(&entry.label, label_width),
                            label_width,
                        );
                        let desc = truncate_to_width(&entry.description, desc_capacity);
                        let line_text = format!("{cursor}{label}  {desc}");
                        lines.push(Line::from(Span::styled(line_text, style)));
                    }
                }
            }
        }

        if palette::ascii_ui_enabled() {
            let footer = format!(
                "{}{}{}{}",
                self.tr(MessageId::HelpFooterTypeFilter),
                self.tr(MessageId::HelpFooterMove),
                self.tr(MessageId::HelpFooterJump),
                self.tr(MessageId::HelpFooterClose)
            );
            let inner = render_ascii_help_chrome(
                popup_area,
                buf,
                &format!(" {} ", self.tr(MessageId::HelpTitle)),
                &footer,
                self.ui_theme,
            );
            Paragraph::new(lines).render(inner, buf);
        } else {
            let footer = help_footer_line(
                vec![
                    self.tr(MessageId::HelpFooterTypeFilter).to_string(),
                    self.tr(MessageId::HelpFooterMove).to_string(),
                    self.tr(MessageId::HelpFooterJump).to_string(),
                    self.tr(MessageId::HelpFooterClose).to_string(),
                ],
                popup_area.width.saturating_sub(4) as usize,
                self.ui_theme,
            );
            let block = modal_block(self.ui_theme)
                .title(Line::from(vec![Span::styled(
                    format!(" {} ", self.tr(MessageId::HelpTitle)),
                    Style::default()
                        .fg(self.ui_theme.accent_primary)
                        .add_modifier(Modifier::BOLD),
                )]))
                .title_bottom(footer);

            Paragraph::new(lines).block(block).render(popup_area, buf);
        }
    }
}

fn help_footer_line(parts: Vec<String>, max_width: usize, theme: UiTheme) -> Line<'static> {
    let style = Style::default().fg(theme.text_muted);
    let spans = parts
        .into_iter()
        .map(|part| Span::styled(part, style))
        .collect::<Vec<_>>();
    Line::from(truncate_spans_to_width(spans, max_width))
}

fn truncate_spans_to_width(spans: Vec<Span<'static>>, max_width: usize) -> Vec<Span<'static>> {
    if max_width == 0 {
        return Vec::new();
    }

    let mut remaining = max_width;
    let mut out = Vec::with_capacity(spans.len());
    for span in spans {
        let content = span.content.as_ref();
        let content_width = UnicodeWidthStr::width(content);
        if content_width <= remaining {
            remaining = remaining.saturating_sub(content_width);
            out.push(span);
            continue;
        }

        let clipped = ascii_prefix(content, remaining);
        if !clipped.is_empty() {
            out.push(Span::styled(clipped, span.style));
        }
        break;
    }
    out
}

fn render_ascii_help_chrome(
    area: Rect,
    buf: &mut Buffer,
    title: &str,
    footer: &str,
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

    let fill_style = Style::default().bg(theme.surface_bg);
    let border_style = Style::default().fg(theme.border).bg(theme.surface_bg);
    let title_style = Style::default()
        .fg(theme.accent_primary)
        .bg(theme.surface_bg)
        .add_modifier(Modifier::BOLD);
    let footer_style = Style::default()
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
        let title = ascii_prefix(title, area.width.saturating_sub(4) as usize);
        buf.set_string(area.x + 2, area.y, &title, title_style);
    }
    if area.width > 8 && area.height > 1 {
        let footer = ascii_prefix(footer, area.width.saturating_sub(4) as usize);
        buf.set_string(
            area.x + 2,
            area.y + area.height.saturating_sub(1),
            &footer,
            footer_style,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn type_filter(view: &mut HelpView, text: &str) {
        for ch in text.chars() {
            view.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
    }

    #[test]
    fn empty_filter_lists_all_entries() {
        let view = HelpView::new();
        // Total = registered slash commands + catalogued keybindings.
        let expected = commands::COMMANDS.len() + KEYBINDINGS.len();
        assert_eq!(view.filtered.len(), expected);
        assert_eq!(view.entries.len(), expected);
    }

    #[test]
    fn substring_filter_narrows_to_command() {
        let mut view = HelpView::new();
        type_filter(&mut view, "mode yolo");
        assert!(!view.filtered.is_empty());
        // Every filtered entry should genuinely contain the query in its
        // searchable haystack — no false positives slipped past.
        for idx in &view.filtered {
            assert!(
                view.entries[*idx].haystack.contains("yolo"),
                "entry {:?} leaked through `mode yolo` filter",
                view.entries[*idx]
            );
        }
        // The unified `/mode` command must surface when filtering for a
        // concrete mode value.
        assert!(
            view.filtered
                .iter()
                .any(|idx| view.entries[*idx].label == "/mode"),
            "/mode should match the `mode yolo` filter"
        );
    }

    #[test]
    fn substring_filter_finds_keybinding_by_chord() {
        let mut view = HelpView::new();
        type_filter(&mut view, "ctrl+r");
        assert!(!view.filtered.is_empty(), "Ctrl+R should match");
        assert!(
            view.filtered
                .iter()
                .any(|idx| view.entries[*idx].label.eq_ignore_ascii_case("ctrl+r")),
            "Ctrl+R chord must surface in the filtered set"
        );
    }

    #[test]
    fn multiple_terms_act_as_and() {
        let mut view = HelpView::new();
        type_filter(&mut view, "session picker");
        assert!(
            !view.filtered.is_empty(),
            "expected at least one entry mentioning both `session` and `picker`"
        );
        for idx in &view.filtered {
            let haystack = &view.entries[*idx].haystack;
            assert!(
                haystack.contains("session") && haystack.contains("picker"),
                "entry {:?} leaked through `session picker` AND filter",
                view.entries[*idx]
            );
        }
    }

    #[test]
    fn unknown_filter_yields_empty_set() {
        let mut view = HelpView::new();
        type_filter(&mut view, "zzzqqxxnope");
        assert!(view.filtered.is_empty());
        assert_eq!(view.selected, 0);
    }

    #[test]
    fn backspace_widens_match_set() {
        let mut view = HelpView::new();
        type_filter(&mut view, "yolox");
        let narrow = view.filtered.len();
        view.handle_key(key(KeyCode::Backspace));
        let wider = view.filtered.len();
        assert!(
            wider > narrow,
            "backspace must broaden the matching set (was {narrow}, now {wider})"
        );
    }

    #[test]
    fn ctrl_h_widens_match_set() {
        let mut view = HelpView::new();
        type_filter(&mut view, "yolox");
        let narrow = view.filtered.len();
        view.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL));
        let wider = view.filtered.len();
        assert!(
            wider > narrow,
            "Ctrl+H must behave as Backspace, broadening the matching set (was {narrow}, now {wider})"
        );
    }

    #[test]
    fn esc_closes_overlay() {
        let mut view = HelpView::new();
        let action = view.handle_key(key(KeyCode::Esc));
        assert!(matches!(action, ViewAction::Close));
    }

    #[test]
    fn ctrl_c_closes_overlay() {
        let mut view = HelpView::new();
        let action = view.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(matches!(action, ViewAction::Close));
    }

    #[test]
    fn q_closes_empty_filter_but_types_when_filtering() {
        let mut view = HelpView::new();
        let action = view.handle_key(key(KeyCode::Char('q')));
        assert!(matches!(action, ViewAction::Close));

        let mut view = HelpView::new();
        type_filter(&mut view, "mod");
        let action = view.handle_key(key(KeyCode::Char('q')));
        assert!(matches!(action, ViewAction::None));
        assert_eq!(view.query, "modq");
    }

    #[test]
    fn help_ellipsis_has_ascii_fallback() {
        if palette::ascii_ui_enabled() {
            assert_eq!(help_ellipsis(1), ".");
            assert_eq!(help_ellipsis(2), "..");
            assert_eq!(help_ellipsis(3), "...");
        } else {
            assert_eq!(help_ellipsis(1), "\u{2026}");
            assert_eq!(help_ellipsis(3), "\u{2026}");
        }
    }

    #[test]
    fn arrow_keys_move_selection_and_wrap_edges() {
        let mut view = HelpView::new();
        // Down once → row 1; Up twice wraps from the first row to the last.
        view.handle_key(key(KeyCode::Down));
        assert_eq!(view.selected, 1);
        view.handle_key(key(KeyCode::Up));
        view.handle_key(key(KeyCode::Up));
        assert_eq!(view.selected, view.filtered.len() - 1);
        // Down from last wraps to first; End still jumps to the last row.
        view.handle_key(key(KeyCode::Down));
        assert_eq!(view.selected, 0);
        view.handle_key(key(KeyCode::End));
        assert_eq!(view.selected, view.filtered.len() - 1);
    }

    #[test]
    fn visible_window_keeps_selected_entry_visible_after_scroll() {
        let mut view = HelpView::new();
        let selected = view
            .filtered
            .iter()
            .position(|idx| view.entries[*idx].label == "/home")
            .expect("/home command should be present");
        view.selected = selected;

        let rows = view.render_rows();
        let row_start = HelpView::visible_row_start(&rows, view.selected, 12);
        let visible = &rows[row_start..(row_start + 12).min(rows.len())];

        assert!(
            visible
                .iter()
                .any(|row| matches!(row, HelpRenderRow::Entry { slot, .. } if *slot == selected)),
            "selected help entry should stay in the visible render window"
        );
    }

    #[test]
    fn render_keeps_next_row_after_help_visible() {
        let mut view = HelpView::new();
        let help_slot = view
            .filtered
            .iter()
            .position(|idx| view.entries[*idx].label == "/help")
            .expect("/help command should be present");
        view.selected = help_slot;
        view.handle_key(key(KeyCode::Down));
        let selected_idx = view.filtered[view.selected];
        let selected_label = view.entries[selected_idx].label.clone();

        let area = Rect::new(0, 0, 96, 32);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        let mut highlighted_label = false;
        for y in area.top()..area.bottom() {
            let mut row = String::new();
            let mut row_has_highlight = false;
            for x in area.left()..area.right() {
                let cell = &buf[(x, y)];
                row.push_str(cell.symbol());
                row_has_highlight |= cell.bg == palette::UI_THEME.selection_bg
                    && cell.fg == palette::UI_THEME.selection_text;
            }
            if row_has_highlight && row.contains(&selected_label) {
                highlighted_label = true;
                break;
            }
        }

        assert!(
            highlighted_label,
            "selected row after /help should stay visibly highlighted"
        );
    }

    #[test]
    fn selected_help_row_uses_stronger_highlight() {
        let view = HelpView::new();
        let area = Rect::new(0, 0, 96, 32);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        let mut found_highlight = false;
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                let cell = &buf[(x, y)];
                if cell.bg == palette::UI_THEME.selection_bg
                    && cell.fg == palette::UI_THEME.selection_text
                {
                    found_highlight = true;
                    break;
                }
            }
        }

        assert!(
            found_highlight,
            "selected row should use a strong blue highlight"
        );
    }

    #[test]
    fn help_selected_prefix_has_ascii_fallback() {
        assert_eq!(help_selected_prefix(false), "  ");
        let selected = help_selected_prefix(true);
        if palette::ascii_ui_enabled() {
            assert_eq!(selected, "> ");
        } else {
            assert_eq!(selected, "\u{25B6} ");
        }
    }

    #[test]
    fn render_includes_help_chrome_for_empty_filter() {
        let view = HelpView::new();
        let area = Rect::new(0, 0, 96, 32);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        let dump = buffer_text(&buf, area);
        // Title border + section headings should always render.
        assert!(dump.contains("Help"), "missing help title:\n{dump}");
        assert!(
            dump.contains("Type to filter"),
            "missing filter prompt:\n{dump}"
        );
        assert!(
            dump.contains("Slash commands"),
            "missing slash-command section heading:\n{dump}"
        );
        // Footer hint should advertise close key on the bottom border.
        assert!(
            dump.contains("Esc close"),
            "missing Esc close footer hint:\n{dump}"
        );
    }

    #[test]
    fn ascii_help_chrome_uses_plain_border_chars() {
        let area = Rect::new(1, 1, 24, 8);
        let mut buf = Buffer::empty(Rect::new(0, 0, 28, 12));
        let inner = render_ascii_help_chrome(
            area,
            &mut buf,
            " Help ",
            " Type to filter  Esc close ",
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
        assert_eq!(inner, Rect::new(area.x + 2, area.y + 2, 20, 4));
    }

    #[test]
    fn ascii_prefix_respects_cjk_display_width() {
        let prefix = ascii_prefix(" 帮助面板 ", 8);

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
    fn help_footer_line_truncates_to_display_width() {
        let line = help_footer_line(
            vec![
                "输入以筛选 ".to_string(),
                "上下移动 ".to_string(),
                "PgUp/PgDn 跳转 ".to_string(),
                "Esc 关闭 ".to_string(),
            ],
            18,
            palette::DEEPSEEK_SHELL_UI_THEME,
        );
        let plain = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(UnicodeWidthStr::width(plain.as_str()) <= 18);
        assert!(
            plain.is_char_boundary(plain.len()),
            "footer must not split UTF-8 codepoints: {plain:?}"
        );
    }

    #[test]
    fn render_with_filter_shows_only_matching_section_and_status() {
        let mut view = HelpView::new();
        type_filter(&mut view, "mode yolo");
        let area = Rect::new(0, 0, 96, 24);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        let dump = buffer_text(&buf, area);
        assert!(
            dump.contains("Filter: mode yolo"),
            "filter echo missing:\n{dump}"
        );
        assert!(
            dump.contains("matches"),
            "match counter missing in dump:\n{dump}"
        );
        assert!(
            dump.contains("/mode"),
            "expected /mode command in filtered render:\n{dump}"
        );
        assert!(
            !dump.contains("/model"),
            "non-matching commands should not render under a `mode yolo` filter:\n{dump}"
        );
    }

    #[test]
    fn long_help_filter_echo_fits_narrow_width() {
        let mut view = HelpView::new_for_locale(Locale::ZhHans)
            .with_ui_theme(palette::DEEPSEEK_SHELL_UI_THEME);
        let long_query = "\u{4f1a}\u{8bdd}\u{5217}\u{8868}".repeat(12);
        type_filter(&mut view, &long_query);

        let area = Rect::new(0, 0, 40, 16);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        for y in area.top()..area.bottom() {
            let mut row = String::new();
            for x in area.left()..area.right() {
                row.push_str(buf[(x, y)].symbol());
            }
            assert!(
                UnicodeWidthStr::width(row.as_str()) <= usize::from(area.width),
                "long filter row overflowed narrow help overlay: {row:?}"
            );
        }
    }

    #[test]
    fn localized_help_rows_fit_80_columns() {
        let view = HelpView::new_for_locale(Locale::ZhHans)
            .with_ui_theme(palette::DEEPSEEK_SHELL_UI_THEME);
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);

        view.render(area, &mut buf);

        for y in area.top()..area.bottom() {
            let mut row = String::new();
            for x in area.left()..area.right() {
                row.push_str(buf[(x, y)].symbol());
            }
            assert!(
                UnicodeWidthStr::width(row.as_str()) <= usize::from(area.width),
                "help row overflowed 80 columns: {row:?}"
            );
        }
    }

    #[test]
    fn localized_help_chrome_renders_without_missing_markers() {
        let view = HelpView::new_for_locale(Locale::ZhHans);
        let area = Rect::new(0, 0, 48, 18);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        let dump = buffer_text(&buf, area);
        assert!(
            dump.contains('帮') && dump.contains('助'),
            "missing localized title:\n{dump}"
        );
        assert!(
            !dump.contains("MISSING"),
            "missing-key marker leaked:\n{dump}"
        );
    }

    #[test]
    fn localized_help_keybinding_descriptions_use_zh_hans() {
        let entries = build_entries(Locale::ZhHans);
        let kb_entries: Vec<_> = entries
            .iter()
            .filter(|e| e.section == HelpSection::Keybinding)
            .collect();
        assert!(!kb_entries.is_empty(), "no keybinding entries found");

        for entry in &kb_entries {
            assert!(
                entry
                    .description
                    .chars()
                    .any(|c| { ('\u{4e00}'..='\u{9fff}').contains(&c) }),
                "keybinding description not localized: {}",
                entry.description
            );
        }
    }

    fn buffer_text(buf: &Buffer, area: Rect) -> String {
        let mut out = String::new();
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }
}
