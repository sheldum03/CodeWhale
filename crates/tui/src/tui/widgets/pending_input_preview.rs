//! Pending-input preview widget for the composer area.
//!
//! Port of `codex-rs/tui/src/bottom_pane/pending_input_preview.rs` for
//! issue #85. Renders queued/steered messages above the composer when a
//! turn is in flight, so user input typed during a running turn doesn't
//! disappear silently. The backing state still distinguishes queue/steer
//! origins, but the UI renders one coherent pending-input list.
//!
//! Empty state renders zero rows so the composer doesn't gain wasted height
//! when there's nothing to show.
//!
//! Wired into `ui.rs::render` between the chat area and the composer; the user
//! can see when typed input has been captured for later delivery.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthChar;

use crate::palette;
use crate::palette::UiTheme;
use crate::tui::widgets::Renderable;

/// Per-item line cap before we collapse the rest into a `…` overflow row.
const PREVIEW_LINE_LIMIT: usize = 3;

/// Description of the keybinding the hint line at the bottom should advertise
/// for the "edit last queued message" action.
#[derive(Debug, Clone)]
pub struct EditBinding {
    pub label: &'static str,
}

impl EditBinding {
    pub const UP: EditBinding = EditBinding { label: "↑" };
}

/// Widget showing pending input while a turn is in progress.
#[derive(Debug, Clone)]
pub struct PendingInputPreview {
    pub context_items: Vec<ContextPreviewItem>,
    pub pending_steers: Vec<String>,
    pub rejected_steers: Vec<String>,
    pub queued_messages: Vec<String>,
    pub edit_binding: EditBinding,
    pub ui_theme: UiTheme,
}

/// Compact pre-send context row shown above the composer. `included=false`
/// marks missing/skipped context distinctly from files/media that will be
/// sent or inlined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextPreviewItem {
    pub kind: String,
    pub label: String,
    pub detail: Option<String>,
    pub included: bool,
    pub removable: bool,
    pub selected: bool,
}

impl PendingInputPreview {
    pub fn new() -> Self {
        Self {
            context_items: Vec::new(),
            pending_steers: Vec::new(),
            rejected_steers: Vec::new(),
            queued_messages: Vec::new(),
            edit_binding: EditBinding::UP,
            ui_theme: palette::UI_THEME,
        }
    }

    #[must_use]
    pub fn with_ui_theme(mut self, ui_theme: UiTheme) -> Self {
        self.ui_theme = ui_theme;
        self
    }

    fn has_pending_inputs(&self) -> bool {
        !self.pending_steers.is_empty()
            || !self.rejected_steers.is_empty()
            || !self.queued_messages.is_empty()
    }

    /// Build the (possibly empty) ordered line list this widget would render
    /// at `width`. Pulled out so `desired_height` can ask the same renderer
    /// without duplicating wrapping logic.
    fn lines(&self, width: u16) -> Vec<Line<'static>> {
        if (self.context_items.is_empty() && !self.has_pending_inputs()) || width < 4 {
            return Vec::new();
        }

        let dim = Style::default()
            .fg(self.ui_theme.text_dim)
            .add_modifier(Modifier::DIM);
        let dim_italic = dim.add_modifier(Modifier::ITALIC);

        let mut lines: Vec<Line<'static>> = Vec::new();

        if !self.context_items.is_empty() {
            push_section_header(&mut lines, "Context for next send", self.ui_theme);
            for item in &self.context_items {
                push_context_item(&mut lines, item, width, self.ui_theme);
            }
        }

        if self.has_pending_inputs() {
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            push_section_header(&mut lines, "Pending inputs", self.ui_theme);
            let item_prefix = pending_item_prefix(false);
            for steer in &self.pending_steers {
                push_truncated_item(&mut lines, steer, width, dim, item_prefix, "    ");
            }
            for steer in &self.rejected_steers {
                push_truncated_item(&mut lines, steer, width, dim, item_prefix, "    ");
            }
            for message in &self.queued_messages {
                push_truncated_item(&mut lines, message, width, dim_italic, item_prefix, "    ");
            }
            if !self.queued_messages.is_empty() {
                let edit_label = if palette::ascii_ui_enabled() {
                    "Up"
                } else {
                    self.edit_binding.label
                };
                lines.push(Line::from(vec![Span::styled(
                    format!("    {edit_label} edit last queued message"),
                    dim,
                )]));
            }
        }

        lines
    }
}

impl Default for PendingInputPreview {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderable for PendingInputPreview {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let lines = self.lines(area.width);
        if lines.is_empty() {
            return;
        }
        Paragraph::new(lines).render(area, buf);
    }

    fn desired_height(&self, width: u16) -> u16 {
        let lines = self.lines(width);
        u16::try_from(lines.len()).unwrap_or(u16::MAX)
    }
}

fn push_section_header(lines: &mut Vec<Line<'static>>, header: &'static str, ui_theme: UiTheme) {
    let style = Style::default()
        .fg(ui_theme.accent_primary)
        .add_modifier(Modifier::BOLD);
    lines.push(Line::from(vec![
        Span::styled(pending_section_prefix(), style),
        Span::styled(header, style),
    ]));
}

fn push_context_item(
    lines: &mut Vec<Line<'static>>,
    item: &ContextPreviewItem,
    width: u16,
    ui_theme: UiTheme,
) {
    let status_style = if item.selected {
        Style::default()
            .fg(ui_theme.selection_text)
            .bg(ui_theme.selection_bg)
            .add_modifier(Modifier::BOLD)
    } else if item.included {
        Style::default().fg(ui_theme.text_muted)
    } else {
        Style::default().fg(ui_theme.warning)
    };
    let label_style = if item.selected {
        Style::default()
            .fg(ui_theme.selection_text)
            .bg(ui_theme.selection_bg)
    } else if item.included {
        Style::default().fg(ui_theme.text)
    } else {
        Style::default().fg(ui_theme.text_muted)
    };
    let detail = item
        .detail
        .as_deref()
        .filter(|detail| !detail.trim().is_empty())
        .map(|detail| format!("{}{detail}", pending_separator()))
        .unwrap_or_default();
    let action = if item.selected {
        format!("{}Backspace/Delete removes", pending_separator())
    } else if item.removable {
        format!("{}removable", pending_separator())
    } else {
        String::new()
    };
    let body = format!("[{}] {}{}{}", item.kind, item.label, detail, action);
    let body_width = width.saturating_sub(4).max(1) as usize;
    for (idx, segment) in wrap_to_width(&body, body_width).into_iter().enumerate() {
        let prefix = if idx == 0 {
            pending_item_prefix(item.selected)
        } else {
            "    "
        };
        lines.push(Line::from(vec![
            Span::styled(prefix.to_string(), status_style),
            Span::styled(segment, label_style),
        ]));
    }
}

/// Render a single bucket item with `↳` prefix, truncating to
/// [`PREVIEW_LINE_LIMIT`] visible rows. Multi-line input wraps at the given
/// column budget and the continuation rows get the `subsequent_indent` so
/// the prefix and the body stay column-aligned.
fn push_truncated_item(
    lines: &mut Vec<Line<'static>>,
    raw: &str,
    width: u16,
    style: Style,
    prefix: &str,
    subsequent_indent: &str,
) {
    let body_width = width.saturating_sub(display_width(prefix) as u16) as usize;
    let body_width = body_width.max(1);

    let mut produced: Vec<String> = Vec::new();
    for (idx, paragraph) in raw.split('\n').enumerate() {
        let wrapped = wrap_to_width(paragraph, body_width);
        for (j, segment) in wrapped.into_iter().enumerate() {
            let row = if idx == 0 && j == 0 {
                format!("{prefix}{segment}")
            } else {
                format!("{subsequent_indent}{segment}")
            };
            produced.push(row);
            if produced.len() > PREVIEW_LINE_LIMIT {
                break;
            }
        }
        if produced.len() > PREVIEW_LINE_LIMIT {
            break;
        }
    }

    let truncated = produced.len() > PREVIEW_LINE_LIMIT;
    for (i, row) in produced.into_iter().enumerate() {
        if i >= PREVIEW_LINE_LIMIT {
            break;
        }
        lines.push(Line::from(Span::styled(row, style)));
    }
    if truncated {
        lines.push(Line::from(Span::styled(
            format!("{}{}", subsequent_indent, pending_overflow_marker()),
            style,
        )));
    }
}

fn pending_section_prefix() -> &'static str {
    pending_section_prefix_for_ascii(palette::ascii_ui_enabled())
}

fn pending_section_prefix_for_ascii(ascii: bool) -> &'static str {
    if ascii {
        "- "
    } else {
        "\u{2022} "
    }
}

fn pending_item_prefix(selected: bool) -> &'static str {
    pending_item_prefix_for_ascii(selected, palette::ascii_ui_enabled())
}

fn pending_item_prefix_for_ascii(selected: bool, ascii: bool) -> &'static str {
    if ascii {
        "  > "
    } else if selected {
        "  \u{25B8} "
    } else {
        "  \u{21B3} "
    }
}

fn pending_overflow_marker() -> &'static str {
    pending_overflow_marker_for_ascii(palette::ascii_ui_enabled())
}

fn pending_overflow_marker_for_ascii(ascii: bool) -> &'static str {
    if ascii {
        "..."
    } else {
        "\u{2026}"
    }
}

fn pending_separator() -> &'static str {
    pending_separator_for_ascii(palette::ascii_ui_enabled())
}

fn pending_separator_for_ascii(ascii: bool) -> &'static str {
    if ascii { " - " } else { " \u{00B7} " }
}

/// Naive word-aware wrap that respects unicode display widths. Long URL-like
/// tokens that exceed `width` are kept to one truncated row instead of being
/// hard-broken into low-signal fragments or overflowing the composer layout.
fn wrap_to_width(text: &str, width: usize) -> Vec<String> {
    if width == 0 || text.is_empty() {
        return vec![text.to_string()];
    }

    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for word in text.split_inclusive(' ') {
        let word_width = display_width(word);
        if current_width + word_width > width && !current.is_empty() {
            out.push(std::mem::take(&mut current));
            current_width = 0;
        }
        if word_width > width {
            // Token longer than the budget: flush current, then keep one
            // readable row inside the visual budget instead of letting a URL
            // or localized path punch through the composer layout.
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            out.push(truncate_to_width(word.trim_end(), width));
            continue;
        }
        current.push_str(word);
        current_width += word_width;
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn display_width(s: &str) -> usize {
    s.chars()
        .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
        .sum()
}

fn truncate_to_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if display_width(text) <= max_width {
        return text.to_string();
    }

    let marker = truncation_marker(max_width);
    let marker_width = display_width(marker);
    let budget = max_width.saturating_sub(marker_width);
    let mut out = String::new();
    let mut width = 0usize;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > budget {
            break;
        }
        out.push(ch);
        width += ch_width;
    }
    out.push_str(marker);
    out
}

fn truncation_marker(max_width: usize) -> &'static str {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn render_to_string(widget: &PendingInputPreview, width: u16) -> Vec<String> {
        let height = widget.desired_height(width);
        if height == 0 {
            return Vec::new();
        }
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        widget.render(Rect::new(0, 0, width, height), &mut buf);
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buf[(x, y)].symbol().chars().next().unwrap_or(' '))
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn empty_widget_has_zero_height() {
        let preview = PendingInputPreview::new();
        assert_eq!(preview.desired_height(40), 0);
    }

    #[test]
    fn single_queued_message_renders_header_item_and_hint() {
        let mut preview = PendingInputPreview::new();
        preview.queued_messages.push("Hello, world!".to_string());
        let rows = render_to_string(&preview, 40);
        // Expect: header line, message line, hint line.
        assert_eq!(rows.len(), 3, "got rows: {rows:?}");
        assert!(rows[0].contains("Pending inputs"));
        assert!(rows[1].contains("Hello, world!"));
        assert!(rows[2].contains("edit last queued message"));
    }

    #[test]
    fn context_items_render_before_queue_buckets() {
        let mut preview = PendingInputPreview::new();
        preview.context_items.push(ContextPreviewItem {
            kind: "file".to_string(),
            label: "src/main.rs".to_string(),
            detail: Some("included".to_string()),
            included: true,
            removable: false,
            selected: false,
        });
        preview.context_items.push(ContextPreviewItem {
            kind: "missing".to_string(),
            label: "nope.txt".to_string(),
            detail: Some("not found".to_string()),
            included: false,
            removable: false,
            selected: false,
        });
        let rows = render_to_string(&preview, 64);
        assert!(rows[0].contains("Context for next send"));
        assert!(rows[1].contains("[file] src/main.rs"));
        assert!(rows[2].contains("[missing] nope.txt"));
    }

    #[test]
    fn selected_removable_attachment_renders_delete_hint() {
        let mut preview = PendingInputPreview::new();
        preview.context_items.push(ContextPreviewItem {
            kind: "image".to_string(),
            label: "/tmp/pasted.png".to_string(),
            detail: Some("attached media".to_string()),
            included: true,
            removable: true,
            selected: true,
        });

        let rows = render_to_string(&preview, 96);

        assert!(
            rows.iter()
                .any(|row| row.contains("Backspace/Delete removes"))
        );
        assert!(rows.iter().any(|row| row.contains(pending_item_prefix(true))));
    }

    #[test]
    fn pending_steer_renders_without_queue_edit_hint() {
        let mut preview = PendingInputPreview::new();
        preview.pending_steers.push("Please continue.".to_string());
        let rows = render_to_string(&preview, 80);
        assert!(
            rows.iter().any(|r| r.contains("Pending inputs")),
            "missing pending input header: {rows:?}"
        );
        assert!(
            !rows.iter().any(|r| r.contains("Esc")),
            "unexpected Esc hint: {rows:?}"
        );
        assert!(
            !rows.iter().any(|r| r.contains("edit last queued message")),
            "unexpected edit hint in pending-steer-only view: {rows:?}"
        );
    }

    #[test]
    fn all_pending_inputs_render_as_one_list() {
        let mut preview = PendingInputPreview::new();
        preview.pending_steers.push("steer".to_string());
        preview.rejected_steers.push("rejected".to_string());
        preview.queued_messages.push("queued".to_string());
        let rows = render_to_string(&preview, 60);
        assert!(rows[0].contains("Pending inputs"));
        assert_eq!(
            rows.iter().filter(|r| r.contains("Pending inputs")).count(),
            1
        );
        assert!(rows.iter().any(|r| r.contains("steer")));
        assert!(rows.iter().any(|r| r.contains("rejected")));
        assert!(rows.iter().any(|r| r.contains("queued")));
        let edit_label = if palette::ascii_ui_enabled() { "Up" } else { "↑" };
        assert!(rows.iter().any(|r| r.contains(edit_label)));
    }

    #[test]
    fn message_truncates_to_three_visible_lines() {
        let mut preview = PendingInputPreview::new();
        preview
            .queued_messages
            .push("line1\nline2\nline3\nline4\nline5".to_string());
        let rows = render_to_string(&preview, 40);
        // Header + 3 visible lines + ellipsis row + hint = 6 rows.
        assert_eq!(rows.len(), 6, "got rows: {rows:?}");
        assert!(rows[0].contains("Pending inputs"));
        assert!(rows[1].contains("line1"));
        assert!(rows[2].contains("line2"));
        assert!(rows[3].contains("line3"));
        assert!(rows[4].contains(pending_overflow_marker()));
        assert!(rows[5].contains("edit last queued message"));
    }

    #[test]
    fn long_url_does_not_explode_into_ellipsis_rows() {
        let mut preview = PendingInputPreview::new();
        preview.queued_messages.push(
            "example.test/api/v1/projects/alpha/releases/2026-02-17/build/1234567890/artifacts/x"
                .to_string(),
        );
        let rows = render_to_string(&preview, 36);
        // Header + URL row + hint = 3 rows; the URL must NOT cause a chain of
        // wrapped-ellipsis rows.
        assert_eq!(rows.len(), 3, "got rows: {rows:?}");
        assert!(
            rows.iter()
                .all(|row| display_width(row.as_str()) <= usize::from(36_u16)),
            "long URL row should stay within the render width: {rows:?}"
        );
        assert!(
            rows[1].contains(truncation_marker(32)),
            "long URL row should show truncation explicitly: {rows:?}"
        );
    }

    #[test]
    fn long_unspaced_cjk_item_truncates_to_render_width() {
        let mut preview = PendingInputPreview::new();
        preview
            .queued_messages
            .push("修复终端布局修复终端布局修复终端布局修复终端布局".to_string());
        let width = 18_u16;
        let rows = render_to_string(&preview, width);

        assert_eq!(rows.len(), 3, "got rows: {rows:?}");
        assert!(
            rows.iter()
                .all(|row| display_width(row.as_str()) <= usize::from(width)),
            "CJK pending input rows should stay within the render width: {rows:?}"
        );
        assert!(
            rows[1].contains(truncation_marker(usize::from(width.saturating_sub(4)))),
            "CJK row should show truncation explicitly: {rows:?}"
        );
    }

    #[test]
    fn narrow_width_renders_nothing() {
        let mut preview = PendingInputPreview::new();
        preview.queued_messages.push("hi".to_string());
        assert_eq!(preview.desired_height(2), 0);
    }

    #[test]
    fn pending_input_symbol_helpers_have_ascii_fallbacks() {
        assert_eq!(pending_section_prefix_for_ascii(true), "- ");
        assert_eq!(pending_item_prefix_for_ascii(false, true), "  > ");
        assert_eq!(pending_item_prefix_for_ascii(true, true), "  > ");
        assert_eq!(pending_overflow_marker_for_ascii(true), "...");
        assert_eq!(pending_separator_for_ascii(true), " - ");
        assert_eq!(pending_section_prefix_for_ascii(false), "\u{2022} ");
        assert_eq!(pending_item_prefix_for_ascii(false, false), "  \u{21B3} ");
        assert_eq!(pending_item_prefix_for_ascii(true, false), "  \u{25B8} ");
        assert_eq!(pending_overflow_marker_for_ascii(false), "\u{2026}");
        assert_eq!(pending_separator_for_ascii(false), " \u{00B7} ");
    }

    #[test]
    fn context_preview_uses_injected_theme_colors() {
        let mut theme = palette::UI_THEME;
        theme.accent_primary = ratatui::style::Color::Indexed(45);
        theme.selection_text = ratatui::style::Color::Indexed(231);
        theme.selection_bg = ratatui::style::Color::Indexed(24);
        let mut preview = PendingInputPreview::new().with_ui_theme(theme);
        preview.context_items.push(ContextPreviewItem {
            kind: "image".to_string(),
            label: "/tmp/pasted.png".to_string(),
            detail: Some("attached media".to_string()),
            included: true,
            removable: true,
            selected: true,
        });

        let lines = preview.lines(96);

        assert_eq!(lines[0].spans[0].style.fg, Some(theme.accent_primary));
        let selected_prefix = lines[1]
            .spans
            .iter()
            .find(|span| span.content.as_ref().contains(pending_item_prefix(true)))
            .expect("selected prefix span");
        assert_eq!(selected_prefix.style.fg, Some(theme.selection_text));
        assert_eq!(selected_prefix.style.bg, Some(theme.selection_bg));
    }
}
