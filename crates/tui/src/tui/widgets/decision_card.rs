//! Decision-card widget for structured user input.
//!
//! When Brother Whale needs input, it surfaces a decision card: a labelled
//! question followed by numbered options, with the default option highlighted.
//! The user navigates with 1-9 keys (or j/k / Up/Down) and confirms with
//! Enter. Every decision is logged so the user can inspect the choice later.
//!
//! This replaces vague "what should I do?" prompts with a structured choice
//! surface — acceptance criterion from the v0.8.43 truth-surface tracker.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::renderable::Renderable;
use crate::palette::{self, UiTheme};

/// A single option in a decision card.
#[derive(Debug, Clone)]
pub struct DecisionOption {
    /// Short label for the option (e.g. "Apply the patch").
    pub label: String,
    /// Optional longer description shown below the label.
    pub description: Option<String>,
}

/// A decision card surfacing a structured choice to the user.
#[derive(Debug, Clone)]
pub struct DecisionCard {
    /// The question or prompt the user is answering.
    pub question: String,
    /// The available options. Each is numbered 1..N.
    pub options: Vec<DecisionOption>,
    /// Index into `options` of the default (highlighted) choice.
    pub default_index: usize,
    /// Index of the currently selected option.
    pub selected_index: usize,
    /// Whether the card has been submitted (Enter pressed).
    pub confirmed: bool,
    /// The index that was confirmed, if any.
    pub confirmed_index: Option<usize>,
    /// Active UI theme for all card chrome and labels.
    pub ui_theme: UiTheme,
}

impl DecisionCard {
    pub fn new(question: String, options: Vec<DecisionOption>, default_index: usize) -> Self {
        let default = default_index.min(options.len().saturating_sub(1));
        Self {
            question,
            options,
            default_index: default,
            selected_index: default,
            confirmed: false,
            confirmed_index: None,
            ui_theme: palette::UI_THEME,
        }
    }

    /// Attach the active UI theme so the card follows `/theme` changes.
    #[must_use]
    pub fn with_ui_theme(mut self, ui_theme: UiTheme) -> Self {
        self.ui_theme = ui_theme;
        self
    }

    /// Number of options.
    pub fn option_count(&self) -> usize {
        self.options.len()
    }

    /// Move selection up (wrap around).
    pub fn select_prev(&mut self) {
        if self.option_count() == 0 {
            return;
        }
        self.selected_index = self
            .selected_index
            .checked_sub(1)
            .unwrap_or(self.option_count() - 1);
    }

    /// Move selection down (wrap around).
    pub fn select_next(&mut self) {
        if self.option_count() == 0 {
            return;
        }
        self.selected_index = (self.selected_index + 1) % self.option_count();
    }

    /// Select by number key (1-based).
    pub fn select_number(&mut self, n: usize) {
        if n > 0 && n <= self.option_count() {
            self.selected_index = n - 1;
        }
    }

    /// Confirm the current selection.
    pub fn confirm(&mut self) {
        self.confirmed = true;
        self.confirmed_index = Some(self.selected_index);
    }

    /// Get the label of the confirmed option, if any.
    pub fn confirmed_label(&self) -> Option<&str> {
        self.confirmed_index
            .and_then(|i| self.options.get(i))
            .map(|opt| opt.label.as_str())
    }
}

impl Default for DecisionCard {
    fn default() -> Self {
        Self::new(String::new(), Vec::new(), 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn decision_card_uses_injected_theme_for_selection() {
        let mut theme = palette::UI_THEME;
        theme.selection_bg = Color::Indexed(24);
        theme.selection_text = Color::Indexed(231);

        let card = DecisionCard::new(
            "Choose".to_string(),
            vec![DecisionOption {
                label: "Apply".to_string(),
                description: None,
            }],
            0,
        )
        .with_ui_theme(theme);
        let area = Rect::new(0, 0, 40, 8);
        let mut buf = Buffer::empty(area);

        card.render(area, &mut buf);

        let selected = &buf[(1, 3)];
        assert_eq!(selected.bg, theme.selection_bg);
        assert_eq!(selected.fg, theme.selection_text);
    }

    #[test]
    fn decision_card_symbol_helpers_have_nonempty_fallbacks() {
        assert!(!decision_separator().is_empty());
        assert!(!decision_selected_prefix().is_empty());
        assert!(decision_footer_hint().contains("Enter"));
    }

    #[test]
    fn ascii_decision_card_chrome_uses_plain_border_chars() {
        let area = Rect::new(1, 1, 24, 8);
        let mut buf = Buffer::empty(Rect::new(0, 0, 28, 12));
        let inner = render_ascii_decision_card_chrome(
            area,
            &mut buf,
            " Decision Required ",
            palette::DEEPSEEK_SHELL_UI_THEME,
            Style::default(),
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
        assert_eq!(inner, Rect::new(area.x + 1, area.y + 1, 22, 6));
    }

    #[test]
    fn ascii_prefix_respects_cjk_display_width() {
        let prefix = ascii_prefix(" 决策卡片 ", 8);

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
    fn decision_card_title_line_truncates_to_display_width() {
        let line = decision_card_title_line(
            &format!(" {} ", "\u{51b3}\u{7b56}\u{5361}\u{7247}".repeat(8)),
            14,
            Style::default(),
        );
        let plain = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(UnicodeWidthStr::width(plain.as_str()) <= 14);
        assert!(
            plain.is_char_boundary(plain.len()),
            "decision card title must not split UTF-8 codepoints: {plain:?}"
        );
    }

    #[test]
    fn decision_ellipsis_has_ascii_fallback() {
        if palette::ascii_ui_enabled() {
            assert_eq!(decision_ellipsis(1), ".");
            assert_eq!(decision_ellipsis(2), "..");
            assert_eq!(decision_ellipsis(3), "...");
        } else {
            assert_eq!(decision_ellipsis(1), "\u{2026}");
            assert_eq!(decision_ellipsis(3), "\u{2026}");
        }
    }

    #[test]
    fn truncate_to_width_respects_cjk_display_width_for_tiny_widths() {
        for max_width in 0..=3 {
            let truncated =
                truncate_to_width("\u{4fee}\u{590d}\u{7ec8}\u{7aef}\u{5e03}\u{5c40}", max_width);

            assert!(
                UnicodeWidthStr::width(truncated.as_str()) <= max_width,
                "truncated decision text overflowed {max_width} columns: {truncated:?}"
            );
            assert!(
                truncated.is_char_boundary(truncated.len()),
                "truncated decision text must not split UTF-8 codepoints: {truncated:?}"
            );
        }
    }

    #[test]
    fn truncate_to_width_preserves_exact_budget_before_ellipsis() {
        let truncated =
            truncate_to_width("\u{4fee}\u{590d}\u{7ec8}\u{7aef}\u{5e03}\u{5c40}", 7);

        assert_eq!(
            UnicodeWidthStr::width(truncated.as_str()),
            7,
            "truncated decision text should fill but not exceed budget: {truncated:?}"
        );
        assert!(truncated.ends_with(decision_ellipsis(7)));
    }
}

impl Renderable for DecisionCard {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height < 3 {
            return;
        }

        let border_style = Style::default().fg(self.ui_theme.border);
        let question_style = Style::default()
            .fg(self.ui_theme.text_body)
            .add_modifier(Modifier::BOLD);
        let dim_style = Style::default().fg(self.ui_theme.text_muted);
        let selected_style = Style::default()
            .fg(self.ui_theme.selection_text)
            .bg(self.ui_theme.selection_bg)
            .add_modifier(Modifier::BOLD);

        let inner = if palette::ascii_ui_enabled() {
            render_ascii_decision_card_chrome(
                area,
                buf,
                " Decision Required ",
                self.ui_theme,
                question_style,
            )
        } else {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .style(Style::default().bg(self.ui_theme.surface_bg))
                .title(decision_card_title_line(
                    " Decision Required ",
                    area.width.saturating_sub(4) as usize,
                    question_style,
                ))
                .title_style(question_style);
            let inner = block.inner(area);
            block.render(area, buf);
            inner
        };

        if inner.width < 2 || inner.height < 2 {
            return;
        }

        let mut y = inner.y;

        // Question line
        let question = truncate_to_width(&self.question, inner.width as usize);
        buf.set_string(inner.x, y, &question, question_style);
        y += 1;

        if y >= inner.y + inner.height {
            return;
        }

        // Separator
        let sep = decision_separator().repeat(inner.width as usize);
        buf.set_string(inner.x, y, &sep, dim_style);
        y += 1;

        // Options
        let max_options = (inner.y + inner.height).saturating_sub(y) as usize;
        for (i, option) in self.options.iter().enumerate().take(max_options) {
            if y >= inner.y + inner.height {
                break;
            }

            let num = format!("{}.", i + 1);
            let is_selected = i == self.selected_index;
            let style = if is_selected {
                selected_style
            } else {
                dim_style
            };

            // "1. Label (default)" or "1. Label"
            let mut label = format!("{} {}", num, option.label);
            if i == self.default_index {
                label.push_str(" (default)");
            }
            label = truncate_to_width(&label, inner.width.saturating_sub(1) as usize);

            let prefix = if is_selected {
                decision_selected_prefix()
            } else {
                "  "
            };
            let full_label = format!("{prefix}{label}");
            buf.set_string(inner.x, y, &full_label, style);
            y += 1;

            // Description line if present
            if let Some(ref desc) = option.description
                && y < inner.y + inner.height
            {
                let desc = format!(
                    "    {}",
                    truncate_to_width(desc, inner.width.saturating_sub(5) as usize)
                );
                buf.set_string(inner.x, y, &desc, dim_style);
                y += 1;
            }
        }

        // Footer hint
        if y < inner.y + inner.height {
            let hint = decision_footer_hint();
            let hint = truncate_to_width(hint, inner.width as usize);
            buf.set_string(inner.x, y, &hint, dim_style);
        }
    }

    fn desired_height(&self, _width: u16) -> u16 {
        // question + separator + options + footer
        let option_lines: u16 = self
            .options
            .iter()
            .map(|o| if o.description.is_some() { 2 } else { 1 })
            .sum();
        // 2 for borders, 1 question, 1 separator, options, 1 footer
        2 + 1 + 1 + option_lines + 1
    }
}

fn render_ascii_decision_card_chrome(
    area: Rect,
    buf: &mut Buffer,
    title: &str,
    ui_theme: UiTheme,
    title_style: Style,
) -> Rect {
    if area.width == 0 || area.height == 0 {
        return Rect {
            x: area.x,
            y: area.y,
            width: 0,
            height: 0,
        };
    }

    let fill_style = Style::default().bg(ui_theme.surface_bg);
    let border_style = Style::default()
        .fg(ui_theme.border)
        .bg(ui_theme.surface_bg);
    let title_style = title_style.bg(ui_theme.surface_bg);

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

    Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
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

fn decision_card_title_line(title: &str, max_width: usize, style: Style) -> Line<'static> {
    Line::from(Span::styled(ascii_prefix(title, max_width), style))
}

fn decision_separator() -> &'static str {
    if palette::ascii_ui_enabled() {
        "-"
    } else {
        "\u{2500}"
    }
}

fn decision_selected_prefix() -> &'static str {
    if palette::ascii_ui_enabled() {
        "> "
    } else {
        "\u{25B8} "
    }
}

fn decision_footer_hint() -> &'static str {
    if palette::ascii_ui_enabled() {
        "1-9 select  |  j/k navigate  |  Enter confirm"
    } else {
        "1-9 select  \u{00B7}  j/k navigate  \u{00B7}  Enter confirm"
    }
}

fn decision_ellipsis(max_width: usize) -> &'static str {
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

fn truncate_to_width(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(s) <= max_width {
        return s.to_string();
    }

    let ellipsis = decision_ellipsis(max_width);
    let ellipsis_width = UnicodeWidthStr::width(ellipsis);
    if ellipsis_width >= max_width {
        let mut out = String::new();
        let mut width = 0usize;
        for ch in ellipsis.chars() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if width + ch_width > max_width {
                break;
            }
            out.push(ch);
            width += ch_width;
        }
        return out;
    }

    let mut truncated = String::new();
    let mut width = 0usize;
    let limit = max_width.saturating_sub(ellipsis_width);
    for ch in s.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > limit {
            break;
        }
        truncated.push(ch);
        width += ch_width;
    }
    truncated.push_str(ellipsis);
    truncated
}
