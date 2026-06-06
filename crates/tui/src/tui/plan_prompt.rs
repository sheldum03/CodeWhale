//! Modal prompt for selecting what to do after a plan is generated.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Rect};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph, Widget, Wrap};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::palette;
use crate::palette::UiTheme;
use crate::tools::plan::PlanSnapshot;
use crate::tui::views::{ModalKind, ModalView, ViewAction, ViewEvent};

const PLAN_OPTIONS: [(&str, &str); 4] = [
    (
        "Accept plan (Agent)",
        "Start implementation in Agent mode with approvals",
    ),
    (
        "Accept plan (YOLO)",
        "Start implementation in YOLO mode (auto-approve)",
    ),
    ("Revise plan", "Ask follow-ups or request plan changes"),
    (
        "Exit Plan mode",
        "Return to Agent mode without implementation",
    ),
];

fn modal_block(ui_theme: UiTheme) -> Block<'static> {
    Block::default()
        .title(Line::from(vec![Span::styled(
            " Plan Confirmation ",
            Style::default().fg(ui_theme.accent_primary).bold(),
        )]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ui_theme.border))
        .padding(Padding::uniform(1))
}

fn render_modal_chrome(area: Rect, popup_area: Rect, buf: &mut Buffer) {
    let shadow_x = popup_area.x.saturating_add(1);
    let shadow_y = popup_area.y.saturating_add(1);
    let shadow_right = area.x.saturating_add(area.width);
    let shadow_bottom = area.y.saturating_add(area.height);
    let shadow_width = popup_area.width.min(shadow_right.saturating_sub(shadow_x));
    let shadow_height = popup_area
        .height
        .min(shadow_bottom.saturating_sub(shadow_y));

    if shadow_width > 0 && shadow_height > 0 {
        Block::default().render(
            Rect {
                x: shadow_x,
                y: shadow_y,
                width: shadow_width,
                height: shadow_height,
            },
            buf,
        );
    }

    Clear.render(popup_area, buf);
}

fn push_option_lines(
    lines: &mut Vec<Line<'static>>,
    selected: bool,
    number: usize,
    label: &str,
    description: &str,
    ui_theme: UiTheme,
) {
    let row_style = if selected {
        Style::default()
            .fg(ui_theme.selection_text)
            .bg(ui_theme.selection_bg)
            .bold()
    } else {
        Style::default().fg(ui_theme.text_body)
    };
    let detail_style = if selected {
        row_style
    } else {
        Style::default().fg(ui_theme.text_muted)
    };
    let prefix = if selected { ">" } else { " " };

    lines.push(Line::from(Span::styled(
        format!("{prefix} {number}) {label}"),
        row_style,
    )));
    lines.push(Line::from(Span::styled(
        format!("    {description}"),
        detail_style,
    )));
}

#[derive(Debug, Clone)]
pub struct PlanPromptView {
    selected: usize,
    /// The plan snapshot to display (if update_plan was called).
    plan: Option<PlanSnapshot>,
    ui_theme: UiTheme,
}

impl Default for PlanPromptView {
    fn default() -> Self {
        Self::new(None)
    }
}

impl PlanPromptView {
    pub fn new(plan: Option<PlanSnapshot>) -> Self {
        Self {
            selected: 0,
            plan,
            ui_theme: palette::UI_THEME,
        }
    }

    #[must_use]
    pub fn with_ui_theme(mut self, ui_theme: UiTheme) -> Self {
        self.ui_theme = ui_theme;
        self
    }

    fn max_index(&self) -> usize {
        PLAN_OPTIONS.len().saturating_sub(1)
    }

    fn submit_selected(&self) -> ViewAction {
        ViewAction::EmitAndClose(ViewEvent::PlanPromptSelected {
            option: self.selected + 1,
        })
    }

    fn submit_number(number: u32) -> ViewAction {
        if (1..=u32::try_from(PLAN_OPTIONS.len()).unwrap_or(0)).contains(&number) {
            ViewAction::EmitAndClose(ViewEvent::PlanPromptSelected {
                option: usize::try_from(number).unwrap_or(1),
            })
        } else {
            ViewAction::None
        }
    }
}

impl ModalView for PlanPromptView {
    fn kind(&self) -> ModalKind {
        ModalKind::PlanPrompt
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                ViewAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(self.max_index());
                ViewAction::None
            }
            KeyCode::Char('1') => {
                self.selected = 0;
                self.submit_selected()
            }
            KeyCode::Char('2') => {
                self.selected = 1;
                self.submit_selected()
            }
            KeyCode::Char('3') => {
                self.selected = 2;
                self.submit_selected()
            }
            KeyCode::Char('4') => {
                self.selected = 3;
                self.submit_selected()
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                self.selected = 0;
                self.submit_selected()
            }
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.selected = 1;
                self.submit_selected()
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.selected = 2;
                self.submit_selected()
            }
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Char('e') | KeyCode::Char('E') => {
                self.selected = 3;
                self.submit_selected()
            }
            KeyCode::Char(ch) if ch.is_ascii_digit() => {
                let number = ch.to_digit(10).unwrap_or(0);
                Self::submit_number(number)
            }
            KeyCode::Enter => self.submit_selected(),
            KeyCode::Esc => ViewAction::EmitAndClose(ViewEvent::PlanPromptDismissed),
            _ => ViewAction::None,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let popup_area = centered_rect(72, 52, area);
        let content_width = usize::from(popup_area.width.saturating_sub(4).max(1));
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(vec![Span::styled(
            "Action required",
            Style::default().fg(self.ui_theme.accent_primary).bold(),
        )]));
        lines.push(Line::from(vec![Span::styled(
            "Choose what should happen after this plan.",
            Style::default().fg(self.ui_theme.text_body).bold(),
        )]));
        lines.push(Line::from(""));

        // v0.8.44: render plan details when update_plan was called (#834)
        if let Some(ref plan) = self.plan {
            if let Some(ref explanation) = plan.explanation {
                for line in wrap_text(explanation, content_width) {
                    lines.push(Line::from(Span::styled(
                        line,
                        Style::default().fg(self.ui_theme.text_muted),
                    )));
                }
                lines.push(Line::from(""));
            }
            if !plan.items.is_empty() {
                lines.push(Line::from(Span::styled(
                    "Plan steps:",
                    Style::default().fg(self.ui_theme.accent_primary).bold(),
                )));
                for item in &plan.items {
                    let status_mark = plan_step_status_mark(item.status);
                    for line in plan_step_lines(status_mark, &item.step, content_width) {
                        lines.push(Line::from(Span::styled(
                            line,
                            Style::default().fg(self.ui_theme.text_body),
                        )));
                    }
                }
                lines.push(Line::from(""));
            }
        }

        for (idx, (label, description)) in PLAN_OPTIONS.iter().enumerate() {
            let number = idx + 1;
            push_option_lines(
                &mut lines,
                self.selected == idx,
                number,
                label,
                description,
                self.ui_theme,
            );
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                "1-4 / a / y / r / q",
                Style::default().fg(self.ui_theme.accent_primary).bold(),
            ),
            Span::styled(" quick pick", Style::default().fg(self.ui_theme.text_muted)),
            Span::raw("  "),
            Span::styled("Up/Down", Style::default().fg(self.ui_theme.accent_primary).bold()),
            Span::styled(" move", Style::default().fg(self.ui_theme.text_muted)),
            Span::raw("  "),
            Span::styled("Enter", Style::default().fg(self.ui_theme.accent_primary).bold()),
            Span::styled(" confirm", Style::default().fg(self.ui_theme.text_muted)),
            Span::raw("  "),
            Span::styled("Esc", Style::default().fg(self.ui_theme.accent_primary).bold()),
            Span::styled(" close", Style::default().fg(self.ui_theme.text_muted)),
        ]));

        render_modal_chrome(area, popup_area, buf);
        if palette::ascii_ui_enabled() {
            let inner = render_ascii_plan_prompt_chrome(popup_area, buf, self.ui_theme);
            Paragraph::new(lines)
                .alignment(Alignment::Left)
                .wrap(Wrap { trim: true })
                .render(inner, buf);
        } else {
            Paragraph::new(lines)
                .alignment(Alignment::Left)
                .wrap(Wrap { trim: true })
                .block(modal_block(self.ui_theme))
                .render(popup_area, buf);
        }
    }
}

fn render_ascii_plan_prompt_chrome(area: Rect, buf: &mut Buffer, theme: UiTheme) -> Rect {
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
        .bold();

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
        let title = ascii_prefix(
            " Plan Confirmation ",
            area.width.saturating_sub(4) as usize,
        );
        buf.set_string(area.x + 2, area.y, &title, title_style);
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

fn plan_step_status_mark(status: crate::tools::plan::StepStatus) -> &'static str {
    let ascii = palette::ascii_ui_enabled();
    match (status, ascii) {
        (crate::tools::plan::StepStatus::Pending, true) => ".",
        (crate::tools::plan::StepStatus::InProgress, true) => ">",
        (crate::tools::plan::StepStatus::Completed, true) => "+",
        (crate::tools::plan::StepStatus::Pending, false) => "\u{b7}",
        (crate::tools::plan::StepStatus::InProgress, false) => "\u{25b6}",
        (crate::tools::plan::StepStatus::Completed, false) => "\u{2713}",
    }
}

fn plan_step_lines(status_mark: &str, step: &str, width: usize) -> Vec<String> {
    let first_prefix = format!("  {status_mark} ");
    let continuation_prefix = "    ";
    let first_width = UnicodeWidthStr::width(first_prefix.as_str());
    let continuation_width = UnicodeWidthStr::width(continuation_prefix);
    let first_content_width = width.saturating_sub(first_width).max(1);
    let continuation_content_width = width.saturating_sub(continuation_width).max(1);

    let mut wrapped = wrap_text(step, first_content_width);
    if wrapped.is_empty() {
        return vec![first_prefix];
    }

    let first = format!("{first_prefix}{}", wrapped.remove(0));
    let mut lines = vec![first];
    for line in wrapped {
        for continuation in wrap_text(&line, continuation_content_width) {
            lines.push(format!("{continuation_prefix}{continuation}"));
        }
    }
    lines
}

/// Wrap text into lines no wider than `width` terminal display columns.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }
        let words: Vec<&str> = paragraph.split_whitespace().collect();
        let mut current = String::new();
        let mut current_width = 0usize;
        for word in words {
            let word_width = UnicodeWidthStr::width(word);
            if word_width > width {
                if !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                    current_width = 0;
                }
                for segment in split_word_by_display_width(word, width) {
                    lines.push(segment);
                }
            } else if current_width + usize::from(!current.is_empty()) + word_width > width {
                lines.push(std::mem::take(&mut current));
                current_width = 0;
                current.push_str(word);
                current_width = word_width;
            } else {
                if !current.is_empty() {
                    current.push(' ');
                    current_width += 1;
                }
                current.push_str(word);
                current_width += word_width;
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    lines
}

fn split_word_by_display_width(word: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for ch in word.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if !current.is_empty() && current_width + ch_width > width {
            lines.push(current);
            current = String::new();
            current_width = 0;
        }
        current.push(ch);
        current_width += ch_width;
    }

    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    fn render_view(view: &PlanPromptView, width: u16, height: u16) -> String {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        (0..height)
            .map(|y| (0..width).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn plan_prompt_calls_out_required_action_and_controls() {
        let rendered = render_view(&PlanPromptView::new(None), 110, 36);

        assert!(rendered.contains("Action required"));
        assert!(rendered.contains("Choose what should happen after this plan."));
        assert!(rendered.contains("1-4"));
        assert!(rendered.contains("Enter"));
    }

    #[test]
    fn plan_prompt_keeps_selected_option_and_description_together() {
        let mut view = PlanPromptView::new(None);
        view.selected = 1;

        let rendered = render_view(&view, 110, 36);

        assert!(rendered.contains("> 2) Accept plan (YOLO)"));
        assert!(rendered.contains("Start implementation in YOLO mode (auto-approve)"));
    }

    #[test]
    fn plan_step_status_marks_follow_ascii_mode() {
        use crate::tools::plan::StepStatus;

        let pending = plan_step_status_mark(StepStatus::Pending);
        let running = plan_step_status_mark(StepStatus::InProgress);
        let completed = plan_step_status_mark(StepStatus::Completed);

        if palette::ascii_ui_enabled() {
            assert_eq!((pending, running, completed), (".", ">", "+"));
        } else {
            assert_eq!(
                (pending, running, completed),
                ("\u{b7}", "\u{25b6}", "\u{2713}")
            );
        }
    }

    #[test]
    fn plan_step_lines_wrap_to_display_width() {
        let lines = plan_step_lines(
            ">",
            &"\u{8ba1}\u{5212}\u{6b65}\u{9aa4}\u{9700}\u{8981}\u{7a84}\u{5b9a}\u{5bbd}\u{5ea6}".repeat(4),
            24,
        );

        assert!(lines.len() > 1);
        for line in &lines {
            assert!(
                UnicodeWidthStr::width(line.as_str()) <= 24,
                "plan step line overflowed display width: {line:?}"
            );
            assert!(
                line.is_char_boundary(line.len()),
                "plan step line must not split UTF-8 codepoints: {line:?}"
            );
        }
        assert!(lines[1].starts_with("    "));
    }

    #[test]
    fn wrap_text_respects_cjk_display_width() {
        let lines = wrap_text("计划步骤需要保持宽字符不溢出", 12);

        assert!(
            lines.len() > 1,
            "wide CJK text should wrap across multiple display-width rows: {lines:?}"
        );
        for line in lines {
            assert!(
                UnicodeWidthStr::width(line.as_str()) <= 12,
                "line overflowed display width: {line:?}"
            );
        }
    }

    #[test]
    fn wrap_text_splits_long_unspaced_words_by_display_width() {
        let lines = wrap_text("alphaβeta路径路径omega", 10);

        assert!(
            lines.len() > 1,
            "long unspaced text should split by display width: {lines:?}"
        );
        for line in lines {
            assert!(
                UnicodeWidthStr::width(line.as_str()) <= 10,
                "line overflowed display width: {line:?}"
            );
        }
    }

    #[test]
    fn ascii_plan_prompt_chrome_uses_plain_border_chars() {
        let area = Rect::new(1, 1, 24, 8);
        let mut buf = Buffer::empty(Rect::new(0, 0, 28, 12));
        let inner =
            render_ascii_plan_prompt_chrome(area, &mut buf, palette::DEEPSEEK_SHELL_UI_THEME);

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
        let prefix = ascii_prefix(" 计划确认 ", 8);

        assert!(
            UnicodeWidthStr::width(prefix.as_str()) <= 8,
            "prefix overflowed display width: {prefix:?}"
        );
        assert!(
            prefix.is_char_boundary(prefix.len()),
            "prefix should end on a valid char boundary: {prefix:?}"
        );
    }
}
