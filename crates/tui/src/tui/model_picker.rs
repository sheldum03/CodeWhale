//! `/model` picker modal: pick a model and thinking-effort tier (#39, #2026).
//!
//! The picker intentionally presents model and thinking as independent choices
//! instead of collapsing them into preset route names. The "auto" option is
//! always available; custom (unrecognized) model ids appear as a separate row.
//! Pass-through providers fall back to only "auto" plus the current custom row.
//!
//! On apply we emit a [`ViewEvent::ModelPickerApplied`] with the resolved
//! model id and effort tier.

use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::config::{ApiProvider, model_completion_names_for_provider};
use crate::palette;
use crate::palette::UiTheme;
use crate::tui::app::{App, ReasoningEffort};
use crate::tui::views::{ModalKind, ModalView, ViewAction, ViewEvent};

fn model_picker_nav_hint() -> &'static str {
    if palette::ascii_ui_enabled() {
        " Up/Down "
    } else {
        " \u{2191}\u{2193} "
    }
}

/// Thinking-effort rows shown in the picker, in the order DeepSeek
/// behaviorally distinguishes them.
const PICKER_EFFORTS: &[ReasoningEffort] = &[
    ReasoningEffort::Auto,
    ReasoningEffort::Off,
    ReasoningEffort::High,
    ReasoningEffort::Max,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pane {
    Model,
    Effort,
}

pub struct ModelPickerView {
    initial_model: String,
    initial_provider: ApiProvider,
    initial_effort: ReasoningEffort,
    /// Working selection (separate from the initial values so we can offer a
    /// clean Esc-to-cancel without mutating App state).
    selected_model_idx: usize,
    selected_effort_idx: usize,
    focus: Pane,
    /// True when the active model is one we don't list — we still show it
    /// so the picker doesn't quietly forget the user's chosen IDs.
    show_custom_model_row: bool,
    model_rows: Vec<ModelPickerRow>,
    ui_theme: UiTheme,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModelPickerRow {
    id: String,
    provider: Option<ApiProvider>,
    hint: String,
}

impl ModelPickerView {
    #[must_use]
    pub fn new(app: &App) -> Self {
        let initial_model = if app.auto_model {
            "auto".to_string()
        } else {
            app.model.clone()
        };
        let model_rows = picker_model_rows_for_app(app);
        let mut selected_model_idx = model_rows.iter().position(|row| {
            row.id == initial_model
                && (row.provider.is_none() || row.provider == Some(app.api_provider))
        });
        let show_custom_model_row = selected_model_idx.is_none();
        if show_custom_model_row {
            selected_model_idx = Some(model_rows.len());
        }
        let selected_model_idx = selected_model_idx.unwrap_or(0);

        let initial_effort = app.reasoning_effort;
        // Map low/medium → high, xhigh → max for picker purposes.
        let normalized = match initial_effort {
            ReasoningEffort::Low | ReasoningEffort::Medium => ReasoningEffort::High,
            other => other,
        };
        let selected_effort_idx = PICKER_EFFORTS
            .iter()
            .position(|e| *e == normalized)
            .unwrap_or(2); // default to High if somehow unknown

        Self {
            initial_model,
            initial_provider: app.api_provider,
            initial_effort,
            selected_model_idx,
            selected_effort_idx,
            focus: Pane::Model,
            show_custom_model_row,
            model_rows,
            ui_theme: palette::UI_THEME,
        }
    }

    #[must_use]
    pub fn with_ui_theme(mut self, ui_theme: UiTheme) -> Self {
        self.ui_theme = ui_theme;
        self
    }

    #[cfg(test)]
    fn visible_model_ids(&self) -> Vec<&str> {
        self.model_rows.iter().map(|row| row.id.as_str()).collect()
    }

    fn visible_model_rows(&self) -> &[ModelPickerRow] {
        &self.model_rows
    }

    fn model_row_count(&self) -> usize {
        self.model_rows.len() + if self.show_custom_model_row { 1 } else { 0 }
    }

    /// Resolve the currently highlighted row to a model id.
    fn resolved_model(&self) -> String {
        if self.show_custom_model_row && self.selected_model_idx == self.model_rows.len() {
            self.initial_model.clone()
        } else if self.selected_model_idx < self.model_rows.len() {
            self.model_rows[self.selected_model_idx].id.clone()
        } else {
            self.initial_model.clone()
        }
    }

    fn resolved_provider(&self) -> Option<ApiProvider> {
        if self.show_custom_model_row && self.selected_model_idx == self.model_rows.len() {
            return Some(self.initial_provider);
        }
        self.model_rows
            .get(self.selected_model_idx)
            .and_then(|row| row.provider)
    }

    fn resolved_effort(&self) -> ReasoningEffort {
        if self.resolved_model().trim().eq_ignore_ascii_case("auto") {
            return ReasoningEffort::Auto;
        }
        PICKER_EFFORTS[self.selected_effort_idx]
    }

    fn move_up(&mut self) -> bool {
        match self.focus {
            Pane::Model => {
                if self.selected_model_idx > 0 {
                    self.selected_model_idx -= 1;
                    return true;
                }
            }
            Pane::Effort => {
                if self.selected_effort_idx > 0 {
                    self.selected_effort_idx -= 1;
                    return true;
                }
            }
        }
        false
    }

    fn move_down(&mut self) -> bool {
        match self.focus {
            Pane::Model => {
                let max = self.model_row_count().saturating_sub(1);
                if self.selected_model_idx < max {
                    self.selected_model_idx += 1;
                    return true;
                }
            }
            Pane::Effort => {
                let max = PICKER_EFFORTS.len().saturating_sub(1);
                if self.selected_effort_idx < max {
                    self.selected_effort_idx += 1;
                    return true;
                }
            }
        }
        false
    }

    fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Pane::Model => Pane::Effort,
            Pane::Effort => Pane::Model,
        };
    }

    fn build_event(&self) -> ViewEvent {
        let provider = self
            .resolved_provider()
            .filter(|provider| *provider != self.initial_provider);
        ViewEvent::ModelPickerApplied {
            model: self.resolved_model(),
            provider,
            effort: self.resolved_effort(),
            previous_model: self.initial_model.clone(),
            previous_effort: self.initial_effort,
        }
    }

    fn render_pane(
        &self,
        area: Rect,
        buf: &mut Buffer,
        title: &str,
        rows: Vec<(String, String)>,
        selected: usize,
        focused: bool,
    ) {
        let border_style = if focused {
            Style::default().fg(self.ui_theme.accent_primary)
        } else {
            Style::default().fg(self.ui_theme.border)
        };
        let visible_height = usize::from(area.height.saturating_sub(2));
        let (start, end) = visible_row_window(selected, rows.len(), visible_height);
        let title = if rows.len() > visible_height && visible_height > 0 {
            format!(" {title} {}-{}/{} ", start + 1, end, rows.len())
        } else {
            format!(" {title} ")
        };
        let title_style = Style::default().fg(self.ui_theme.text_body).bold();
        let inner = if palette::ascii_ui_enabled() {
            render_ascii_model_picker_pane(area, buf, &title, border_style, title_style)
        } else {
            let block = Block::default()
                .title(Line::from(Span::styled(title, title_style)))
                .borders(Borders::ALL)
                .border_style(border_style)
                .style(Style::default());
            let inner = block.inner(area);
            block.render(area, buf);
            inner
        };

        let mut lines = Vec::with_capacity(end.saturating_sub(start));
        for (idx, (label, hint)) in rows.iter().enumerate().skip(start).take(end - start) {
            let is_selected = idx == selected;
            let marker = model_picker_marker(is_selected);
            let label_style = if is_selected {
                Style::default()
                    .fg(self.ui_theme.selection_text)
                    .bg(self.ui_theme.selection_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.ui_theme.text_body)
            };
            let hint_style = if is_selected {
                Style::default()
                    .fg(self.ui_theme.selection_text)
                    .bg(self.ui_theme.selection_bg)
            } else {
                Style::default().fg(self.ui_theme.text_muted)
            };
            let spans = picker_row_spans(
                label,
                hint,
                marker,
                usize::from(inner.width),
                label_style,
                hint_style,
            );
            lines.push(Line::from(spans));
        }
        Paragraph::new(lines).render(inner, buf);
    }
}

fn visible_row_window(selected: usize, total: usize, viewport_height: usize) -> (usize, usize) {
    if total == 0 || viewport_height == 0 {
        return (0, 0);
    }

    let visible = viewport_height.min(total);
    let mut start = selected.saturating_sub(visible / 2);
    if start + visible > total {
        start = total.saturating_sub(visible);
    }
    (start, start + visible)
}

fn model_picker_marker(selected: bool) -> &'static str {
    if !selected {
        " "
    } else if palette::ascii_ui_enabled() {
        ">"
    } else {
        "\u{25B8}"
    }
}

fn picker_row_spans<'a>(
    label: &'a str,
    hint: &'a str,
    marker: &'static str,
    width: usize,
    label_style: Style,
    hint_style: Style,
) -> Vec<Span<'a>> {
    let prefix_width = 3;
    let label_width = width.saturating_sub(prefix_width);
    let label = fit_text(label, label_width);
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(marker, label_style),
        Span::raw(" "),
        Span::styled(label, label_style),
    ];

    if !hint.is_empty() {
        let hint_text = format!("  ({hint})");
        let used = prefix_width
            + unicode_width::UnicodeWidthStr::width(
                spans
                    .last()
                    .map(|span| span.content.as_ref())
                    .unwrap_or_default(),
            );
        if used + unicode_width::UnicodeWidthStr::width(hint_text.as_str()) <= width {
            spans.push(Span::styled(hint_text, hint_style));
        }
    }

    spans
}

fn fit_text(text: &str, width: usize) -> String {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    if width <= 3 {
        return ".".repeat(width);
    }

    let mut out = String::new();
    let target = width - 3;
    let mut used = 0usize;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width > target {
            break;
        }
        used += ch_width;
        out.push(ch);
    }
    out.push_str("...");
    out
}

#[cfg(test)]
fn picker_model_ids_for_provider(provider: ApiProvider) -> Vec<&'static str> {
    let mut models = vec!["auto"];
    for id in model_completion_names_for_provider(provider) {
        if id != "auto" && !models.contains(&id) {
            models.push(id);
        }
    }
    models
}

fn picker_model_rows_for_app(app: &App) -> Vec<ModelPickerRow> {
    let mut rows = Vec::new();
    push_model_row(
        &mut rows,
        "auto".to_string(),
        None,
        picker_model_hint("auto").to_string(),
    );

    for id in model_completion_names_for_provider(app.api_provider) {
        if id != "auto" {
            push_model_row(
                &mut rows,
                id.to_string(),
                Some(app.api_provider),
                picker_model_hint(id).to_string(),
            );
        }
    }

    if let Some(model) = app
        .provider_models
        .get(app.api_provider.as_str())
        .map(|model| model.trim())
        .filter(|model| !model.is_empty())
    {
        push_model_row(
            &mut rows,
            model.to_string(),
            Some(app.api_provider),
            format!("{} saved", app.api_provider.display_name()),
        );
    }

    rows
}

fn push_model_row(
    rows: &mut Vec<ModelPickerRow>,
    id: String,
    provider: Option<ApiProvider>,
    hint: String,
) {
    if rows
        .iter()
        .any(|row| row.id == id && row.provider == provider)
    {
        return;
    }
    rows.push(ModelPickerRow { id, provider, hint });
}

fn picker_model_hint(id: &str) -> &'static str {
    match id {
        "auto" => "select per turn",
        "deepseek-v4-pro" | "deepseek/deepseek-v4-pro" | "deepseek-ai/deepseek-v4-pro" => {
            "larger model"
        }
        "deepseek-v4-flash" | "deepseek/deepseek-v4-flash" | "deepseek-ai/deepseek-v4-flash" => {
            "faster model"
        }
        "arcee-ai/trinity-large-thinking" => "large thinking",
        "xiaomi/mimo-v2.5-pro" | "mimo-v2.5-pro" => "reasoning / coding",
        "xiaomi/mimo-v2.5" | "mimo-v2.5" => "v2.5 omni",
        "mimo-v2.5-tts" | "mimo-v2-tts" => "speech / TTS",
        "mimo-v2.5-tts-voicedesign" => "voice design",
        "mimo-v2.5-tts-voiceclone" => "voice clone",
        "minimax/minimax-m3" => "1M multimodal",
        _ => "provider model",
    }
}

impl ModalView for ModelPickerView {
    fn kind(&self) -> ModalKind {
        ModalKind::ModelPicker
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Esc => ViewAction::Close,
            KeyCode::Enter => ViewAction::EmitAndClose(self.build_event()),
            KeyCode::Up => {
                self.move_up();
                ViewAction::None
            }
            KeyCode::Down => {
                self.move_down();
                ViewAction::None
            }
            KeyCode::PageUp => {
                for _ in 0..5 {
                    self.move_up();
                }
                ViewAction::None
            }
            KeyCode::PageDown => {
                for _ in 0..5 {
                    self.move_down();
                }
                ViewAction::None
            }
            KeyCode::Home => {
                match self.focus {
                    Pane::Model => self.selected_model_idx = 0,
                    Pane::Effort => self.selected_effort_idx = 0,
                }
                ViewAction::None
            }
            KeyCode::End => {
                match self.focus {
                    Pane::Model => {
                        self.selected_model_idx = self.model_row_count().saturating_sub(1);
                    }
                    Pane::Effort => {
                        self.selected_effort_idx = PICKER_EFFORTS.len().saturating_sub(1);
                    }
                }
                ViewAction::None
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Left | KeyCode::BackTab => {
                self.toggle_focus();
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> ViewAction {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.move_up();
                ViewAction::None
            }
            MouseEventKind::ScrollDown => {
                self.move_down();
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.render_classic(area, buf);
    }
}

impl ModelPickerView {
    fn render_classic(&self, area: Rect, buf: &mut Buffer) {
        let available_width = area.width.saturating_sub(4);
        let popup_width = if available_width >= 60 {
            available_width.min(96)
        } else {
            area.width.saturating_sub(2).max(1)
        };
        let desired_height = (self.model_row_count().max(PICKER_EFFORTS.len()) as u16)
            .saturating_add(4)
            .clamp(10, 22);
        let available_height = area.height.saturating_sub(4);
        let popup_height = if available_height >= 10 {
            desired_height.min(available_height)
        } else {
            area.height.saturating_sub(2).max(1)
        };
        let popup_area = Rect {
            x: area.x + (area.width.saturating_sub(popup_width)) / 2,
            y: area.y + (area.height.saturating_sub(popup_height)) / 2,
            width: popup_width,
            height: popup_height,
        };

        Clear.render(popup_area, buf);

        let inner = if palette::ascii_ui_enabled() {
            render_ascii_model_picker_chrome(popup_area, buf, self.ui_theme)
        } else {
            let outer = Block::default()
                .title(Line::from(Span::styled(
                    " Model & thinking ",
                    Style::default()
                        .fg(self.ui_theme.accent_primary)
                        .add_modifier(Modifier::BOLD),
                )))
                .title_bottom(model_picker_footer_line(
                    &format!("{}move Tab switch Enter apply Esc cancel ", model_picker_nav_hint()),
                    popup_area.width.saturating_sub(4) as usize,
                    self.ui_theme,
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.ui_theme.border))
                .style(Style::default());
            let inner = outer.inner(popup_area);
            outer.render(popup_area, buf);
            inner
        };

        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
            .split(inner);

        let mut model_rows: Vec<(String, String)> = self
            .visible_model_rows()
            .iter()
            .map(|row| (row.id.clone(), row.hint.clone()))
            .collect();
        if self.show_custom_model_row {
            model_rows.push((self.initial_model.clone(), "current (custom)".to_string()));
        }
        self.render_pane(
            columns[0],
            buf,
            "Model",
            model_rows,
            self.selected_model_idx,
            self.focus == Pane::Model,
        );

        let effort_rows: Vec<(String, String)> = PICKER_EFFORTS
            .iter()
            .map(|effort| {
                let label = effort.short_label().to_string();
                let hint = match effort {
                    ReasoningEffort::Auto => "choose per turn".to_string(),
                    ReasoningEffort::Off => "no extra reasoning".to_string(),
                    ReasoningEffort::High => "deeper reasoning".to_string(),
                    ReasoningEffort::Max => "maximum reasoning".to_string(),
                    _ => String::new(),
                };
                (label, hint)
            })
            .collect();
        self.render_pane(
            columns[1],
            buf,
            "Thinking",
            effort_rows,
            self.selected_effort_idx,
            self.focus == Pane::Effort,
        );
    }
}

fn render_ascii_model_picker_chrome(area: Rect, buf: &mut Buffer, theme: UiTheme) -> Rect {
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
        let title = ascii_prefix(
            " Model & thinking ",
            area.width.saturating_sub(4) as usize,
        );
        buf.set_string(area.x + 2, area.y, &title, title_style);
    }
    if area.width > 8 && area.height > 1 {
        let footer = ascii_prefix(
            " Up/Down move Tab switch Enter apply Esc cancel ",
            area.width.saturating_sub(4) as usize,
        );
        buf.set_string(
            area.x + 2,
            area.y + area.height.saturating_sub(1),
            &footer,
            footer_style,
        );
    }

    Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

fn render_ascii_model_picker_pane(
    area: Rect,
    buf: &mut Buffer,
    title: &str,
    border_style: Style,
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

fn model_picker_footer_line(footer: &str, max_width: usize, theme: UiTheme) -> Line<'static> {
    Line::from(Span::styled(
        ascii_prefix(footer, max_width),
        Style::default().fg(theme.text_muted),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::tui::app::{App, TuiOptions};
    use std::path::PathBuf;
    use unicode_width::UnicodeWidthStr;

    fn create_test_app() -> (App, std::sync::MutexGuard<'static, ()>) {
        let lock = crate::test_support::lock_test_env();
        let options = TuiOptions {
            model: "deepseek-v4-pro".to_string(),
            workspace: PathBuf::from("."),
            config_path: None,
            config_profile: None,
            allow_shell: false,
            use_alt_screen: true,
            use_mouse_capture: false,
            use_bracketed_paste: true,
            max_subagents: 1,
            skills_dir: PathBuf::from("."),
            memory_path: PathBuf::from("memory.md"),
            notes_path: PathBuf::from("notes.txt"),
            mcp_config_path: PathBuf::from("mcp.json"),
            use_memory: false,
            start_in_agent_mode: true,
            skip_onboarding: true,
            yolo: false,
            resume_session_id: None,
            initial_input: None,
        };
        let mut app = App::new(options, &Config::default());
        // App::new merges in the user's persisted settings.toml, which can override
        // the model, effort, and provider with whatever the developer
        // happens to have saved. Pin all three back to known values so
        // the picker tests below exercise the picker logic, not the
        // user's environment. In particular `api_provider` matters because
        // pass-through providers (Ollama, OpenAI) hide the DeepSeek model
        // rows and leave only `auto` + custom — Down has nowhere to go.
        app.model = "deepseek-v4-pro".to_string();
        app.auto_model = false;
        app.reasoning_effort = ReasoningEffort::Max;
        app.api_provider = crate::config::ApiProvider::Deepseek;
        app.model_ids_passthrough = false;
        app.provider_models.clear();
        (app, lock)
    }

    #[test]
    fn picker_initial_selection_matches_app_state() {
        let (mut app, _lock) = create_test_app();
        app.model = "deepseek-v4-flash".to_string();
        app.auto_model = false;
        app.reasoning_effort = ReasoningEffort::Max;
        let view = ModelPickerView::new(&app);
        assert_eq!(view.resolved_model(), "deepseek-v4-flash");
        assert_eq!(view.resolved_effort(), ReasoningEffort::Max);
    }

    #[test]
    fn picker_initial_selection_matches_auto_state() {
        let (mut app, _lock) = create_test_app();
        app.model = "auto".to_string();
        app.auto_model = true;
        app.reasoning_effort = ReasoningEffort::Auto;

        let view = ModelPickerView::new(&app);

        assert_eq!(view.resolved_model(), "auto");
        assert_eq!(view.resolved_effort(), ReasoningEffort::Auto);
    }

    #[test]
    fn picker_auto_model_forces_auto_effort_on_apply() {
        let (mut app, _lock) = create_test_app();
        app.model = "auto".to_string();
        app.auto_model = true;
        app.reasoning_effort = ReasoningEffort::Off;

        let view = ModelPickerView::new(&app);

        assert_eq!(view.resolved_model(), "auto");
        assert_eq!(view.resolved_effort(), ReasoningEffort::Auto);
    }

    #[test]
    fn picker_normalizes_low_medium_to_high() {
        let (mut app, _lock) = create_test_app();
        app.reasoning_effort = ReasoningEffort::Medium;
        app.auto_model = false;
        let view = ModelPickerView::new(&app);
        assert_eq!(
            view.resolved_effort(),
            ReasoningEffort::High,
            "medium should map to high in the picker"
        );
    }

    #[test]
    fn picker_exposes_auto_and_distinct_thinking_tiers() {
        let model_labels = picker_model_ids_for_provider(crate::config::ApiProvider::Deepseek);
        assert_eq!(
            model_labels,
            vec!["auto", "deepseek-v4-pro", "deepseek-v4-flash"]
        );

        let effort_labels: Vec<_> = PICKER_EFFORTS
            .iter()
            .map(|effort| effort.as_setting())
            .collect();
        assert_eq!(effort_labels, vec!["auto", "off", "high", "max"]);
    }

    #[test]
    fn picker_preserves_unknown_model_via_custom_row() {
        let (mut app, _lock) = create_test_app();
        app.model = "deepseek-v4-pro-2026-04-XX".to_string();
        app.auto_model = false;
        let view = ModelPickerView::new(&app);
        assert!(view.show_custom_model_row);
        assert_eq!(view.resolved_model(), "deepseek-v4-pro-2026-04-XX");
    }

    #[test]
    fn picker_lists_openrouter_large_models() {
        let (mut app, _lock) = create_test_app();
        app.api_provider = crate::config::ApiProvider::Openrouter;
        app.model_ids_passthrough = true;
        app.model = "minimax/minimax-m3".to_string();
        app.auto_model = false;

        let view = ModelPickerView::new(&app);
        let model_ids = view.visible_model_ids();

        assert!(model_ids.contains(&"arcee-ai/trinity-large-thinking"));
        assert!(model_ids.contains(&"xiaomi/mimo-v2.5-pro"));
        assert!(model_ids.contains(&"minimax/minimax-m3"));
        assert!(
            model_ids
                .iter()
                .take(6)
                .any(|id| *id == "minimax/minimax-m3"),
            "MiniMax M3 should be visible in the first picker window on normal terminals"
        );
        assert!(!view.show_custom_model_row);
        assert_eq!(view.resolved_model(), "minimax/minimax-m3");
    }

    #[test]
    fn picker_lists_xiaomi_mimo_chat_models_without_speech_models() {
        let (mut app, _lock) = create_test_app();
        app.api_provider = crate::config::ApiProvider::XiaomiMimo;
        app.model = "mimo-v2.5-pro".to_string();
        app.auto_model = false;

        let view = ModelPickerView::new(&app);
        let model_ids = view.visible_model_ids();

        for expected in ["mimo-v2.5-pro", "mimo-v2.5"] {
            assert!(model_ids.contains(&expected), "missing {expected}");
        }
        for deprecated in ["mimo-v2-pro", "mimo-v2-omni", "mimo-v2-flash"] {
            assert!(
                !model_ids.contains(&deprecated),
                "{deprecated} is deprecated and should not be promoted"
            );
        }
        for speech_model in [
            "mimo-v2.5-tts",
            "mimo-v2.5-tts-voicedesign",
            "mimo-v2.5-tts-voiceclone",
            "mimo-v2-tts",
        ] {
            assert!(
                !model_ids.contains(&speech_model),
                "{speech_model} should not appear in the chat model picker"
            );
        }
    }

    #[test]
    fn visible_row_window_tracks_selection_in_short_panes() {
        assert_eq!(visible_row_window(0, 16, 8), (0, 8));
        assert_eq!(visible_row_window(7, 16, 8), (3, 11));
        assert_eq!(visible_row_window(15, 16, 8), (8, 16));
        assert_eq!(visible_row_window(3, 4, 8), (0, 4));
        assert_eq!(visible_row_window(3, 4, 0), (0, 0));
    }

    #[test]
    fn model_picker_marker_has_ascii_fallback() {
        assert_eq!(model_picker_marker(false), " ");
        let selected = model_picker_marker(true);
        if palette::ascii_ui_enabled() {
            assert_eq!(selected, ">");
        } else {
            assert_eq!(selected, "\u{25B8}");
        }
    }

    #[test]
    fn ascii_model_picker_chrome_uses_plain_border_chars() {
        let area = Rect::new(1, 1, 24, 8);
        let mut buf = Buffer::empty(Rect::new(0, 0, 28, 12));
        let inner =
            render_ascii_model_picker_chrome(area, &mut buf, palette::DEEPSEEK_SHELL_UI_THEME);

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
        let prefix = ascii_prefix(" 模型选择 ", 8);

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
    fn model_picker_footer_line_truncates_to_display_width() {
        let line = model_picker_footer_line(
            " Up/Down move Tab switch Enter apply Esc cancel \u{4e0a}\u{4e0b}\u{79fb}\u{52a8} ",
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
    fn ascii_model_picker_pane_uses_plain_border_chars() {
        let area = Rect::new(2, 2, 18, 6);
        let mut buf = Buffer::empty(Rect::new(0, 0, 24, 10));
        let inner = render_ascii_model_picker_pane(
            area,
            &mut buf,
            " Models ",
            Style::default(),
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
        assert_eq!(inner, Rect::new(area.x + 1, area.y + 1, 16, 4));
    }

    #[test]
    fn narrow_picker_rows_hide_hint_before_clipping_model_id() {
        let spans = picker_row_spans(
            "minimax/minimax-m3",
            "1M multimodal",
            model_picker_marker(true),
            24,
            Style::default(),
            Style::default(),
        );
        let rendered = spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(rendered.contains("minimax/minimax-m3"));
        assert!(!rendered.contains("1M multimodal"));
        assert!(unicode_width::UnicodeWidthStr::width(rendered.as_str()) <= 24);
    }

    #[test]
    fn picker_preserves_custom_passthrough_model_ids() {
        let (mut app, _lock) = create_test_app();
        app.api_provider = crate::config::ApiProvider::Openrouter;
        app.model_ids_passthrough = true;
        app.model = "opencode-go/glm-5.1".to_string();
        app.auto_model = false;

        let view = ModelPickerView::new(&app);

        assert!(view.show_custom_model_row);
        assert_eq!(view.resolved_model(), "opencode-go/glm-5.1");
    }

    #[test]
    fn picker_exposes_saved_model_for_active_provider() {
        let (mut app, _lock) = create_test_app();
        app.api_provider = crate::config::ApiProvider::XiaomiMimo;
        app.model = "mimo-v2.5-custom".to_string();
        app.auto_model = false;
        app.provider_models
            .insert("xiaomi-mimo".to_string(), "mimo-v2.5-custom".to_string());

        let mut view = ModelPickerView::new(&app);
        view.selected_model_idx = view
            .model_rows
            .iter()
            .position(|row| {
                row.id == "mimo-v2.5-custom"
                    && row.provider == Some(crate::config::ApiProvider::XiaomiMimo)
            })
            .expect("saved Xiaomi MiMo model row");

        let action = view.handle_key(KeyEvent::new(
            KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        match action {
            ViewAction::EmitAndClose(ViewEvent::ModelPickerApplied {
                model, provider, ..
            }) => {
                assert_eq!(model, "mimo-v2.5-custom");
                assert_eq!(provider, None);
            }
            other => panic!("expected ModelPickerApplied EmitAndClose, got {other:?}"),
        }
    }

    #[test]
    fn picker_hides_saved_models_from_other_providers() {
        let (mut app, _lock) = create_test_app();
        app.api_provider = crate::config::ApiProvider::XiaomiMimo;
        app.model = "mimo-v2.5-pro".to_string();
        app.auto_model = false;
        app.provider_models
            .insert("deepseek".to_string(), "deepseek-v4-pro".to_string());
        app.provider_models
            .insert("moonshot".to_string(), "kimi-k2.6".to_string());

        let view = ModelPickerView::new(&app);
        let model_ids = view.visible_model_ids();

        assert!(model_ids.contains(&"mimo-v2.5-pro"));
        assert!(!model_ids.contains(&"deepseek-v4-pro"));
        assert!(!model_ids.contains(&"kimi-k2.6"));
        assert!(!view.show_custom_model_row);
    }

    #[test]
    fn picker_does_not_hijack_current_custom_model_with_saved_provider_row() {
        let (mut app, _lock) = create_test_app();
        app.api_provider = crate::config::ApiProvider::Openai;
        app.model_ids_passthrough = true;
        app.model = "kimi-k2.6".to_string();
        app.provider_models
            .insert("moonshot".to_string(), "kimi-k2.6".to_string());

        let mut view = ModelPickerView::new(&app);

        assert!(view.show_custom_model_row);
        assert_eq!(view.resolved_model(), "kimi-k2.6");
        let action = view.handle_key(KeyEvent::new(
            KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        match action {
            ViewAction::EmitAndClose(ViewEvent::ModelPickerApplied {
                model, provider, ..
            }) => {
                assert_eq!(model, "kimi-k2.6");
                assert_eq!(provider, None);
            }
            other => panic!("expected ModelPickerApplied EmitAndClose, got {other:?}"),
        }
    }

    #[test]
    fn arrow_keys_move_within_focused_pane() {
        let (mut app, _lock) = create_test_app();
        app.model = "deepseek-v4-pro".to_string();
        app.reasoning_effort = ReasoningEffort::High;
        let mut view = ModelPickerView::new(&app);
        assert_eq!(view.selected_model_idx, 1);
        view.handle_key(KeyEvent::new(
            KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(view.selected_model_idx, 2);
        view.handle_key(KeyEvent::new(
            KeyCode::Up,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(view.selected_model_idx, 1);

        view.handle_key(KeyEvent::new(
            KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(view.focus, Pane::Effort);
        assert_eq!(view.selected_effort_idx, 2);
        view.handle_key(KeyEvent::new(
            KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(view.selected_effort_idx, 3);
    }

    #[test]
    fn mouse_wheel_moves_focused_picker_pane() {
        let (mut app, _lock) = create_test_app();
        app.model = "deepseek-v4-pro".to_string();
        let mut view = ModelPickerView::new(&app);
        assert_eq!(view.selected_model_idx, 1);

        view.handle_mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: crossterm::event::KeyModifiers::NONE,
        });
        assert_eq!(view.selected_model_idx, 2);

        view.handle_mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::ScrollUp,
            column: 0,
            row: 0,
            modifiers: crossterm::event::KeyModifiers::NONE,
        });
        assert_eq!(view.selected_model_idx, 1);
    }

    #[test]
    fn tab_switches_between_model_and_thinking() {
        let (app, _lock) = create_test_app();
        let mut view = ModelPickerView::new(&app);
        assert_eq!(view.focus, Pane::Model);
        view.handle_key(KeyEvent::new(
            KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(view.focus, Pane::Effort);
        view.handle_key(KeyEvent::new(
            KeyCode::BackTab,
            crossterm::event::KeyModifiers::SHIFT,
        ));
        assert_eq!(view.focus, Pane::Model);
    }

    #[test]
    fn enter_emits_current_model_and_thinking() {
        let (mut app, _lock) = create_test_app();
        app.reasoning_effort = ReasoningEffort::High;
        app.model = "deepseek-v4-pro".to_string();
        app.auto_model = false;
        let mut view = ModelPickerView::new(&app);
        assert_eq!(view.selected_model_idx, 1);
        assert_eq!(view.selected_effort_idx, 2);

        // Move model from Pro to Flash, then switch to effort and move High to Max.
        view.handle_key(KeyEvent::new(
            KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        ));
        view.handle_key(KeyEvent::new(
            KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        ));
        view.handle_key(KeyEvent::new(
            KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        ));

        let action = view.handle_key(KeyEvent::new(
            KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        match action {
            ViewAction::EmitAndClose(ViewEvent::ModelPickerApplied {
                model,
                effort,
                previous_effort,
                ..
            }) => {
                assert_eq!(model, "deepseek-v4-flash");
                assert_eq!(effort, ReasoningEffort::Max);
                assert_eq!(previous_effort, ReasoningEffort::High);
            }
            other => panic!("expected ModelPickerApplied EmitAndClose, got {other:?}"),
        }
    }

    #[test]
    fn deepseek_provider_uses_neutral_two_pane_selection() {
        let (mut app, _lock) = create_test_app();
        app.model = "deepseek-v4-flash".to_string();
        app.auto_model = false;
        app.reasoning_effort = ReasoningEffort::Max;
        let view = ModelPickerView::new(&app);
        assert_eq!(view.selected_model_idx, 2);
        assert_eq!(view.selected_effort_idx, 3);
        assert_eq!(view.focus, Pane::Model);
        assert_eq!(view.resolved_model(), "deepseek-v4-flash");
        assert_eq!(view.resolved_effort(), ReasoningEffort::Max);
    }

    #[test]
    fn known_model_with_auto_effort_preserves_explicit_model() {
        let (mut app, _lock) = create_test_app();
        app.model = "deepseek-v4-pro".to_string();
        app.auto_model = false;
        app.reasoning_effort = ReasoningEffort::Auto;
        let view = ModelPickerView::new(&app);
        assert!(!view.show_custom_model_row);
        assert_eq!(view.selected_model_idx, 1);
        assert_eq!(view.selected_effort_idx, 0);
        assert_eq!(view.resolved_model(), "deepseek-v4-pro");
        assert_eq!(view.resolved_effort(), ReasoningEffort::Auto);
    }

    #[test]
    fn auto_model_selects_auto_row() {
        let (mut app, _lock) = create_test_app();
        app.model = "auto".to_string();
        app.auto_model = true;
        app.reasoning_effort = ReasoningEffort::Auto;
        let view = ModelPickerView::new(&app);
        assert_eq!(view.selected_model_idx, 0);
        assert_eq!(view.selected_effort_idx, 0);
        assert_eq!(view.resolved_model(), "auto");
        assert_eq!(view.resolved_effort(), ReasoningEffort::Auto);
    }

    #[test]
    fn custom_model_row_preserves_current_model_and_effort() {
        let (mut app, _lock) = create_test_app();
        app.model = "deepseek-v4-pro-2026-04-XX".to_string();
        app.auto_model = false;
        app.reasoning_effort = ReasoningEffort::High;
        let view = ModelPickerView::new(&app);
        assert!(view.show_custom_model_row);
        assert_eq!(view.selected_model_idx, 3);
        assert_eq!(view.selected_effort_idx, 2);
        assert_eq!(view.resolved_model(), "deepseek-v4-pro-2026-04-XX");
        assert_eq!(view.resolved_effort(), ReasoningEffort::High);
    }

    #[test]
    fn move_down_from_last_model_is_noop() {
        let (app, _lock) = create_test_app();
        let mut view = ModelPickerView::new(&app);
        view.selected_model_idx = view.model_row_count() - 1;
        let result = view.move_down();
        assert!(!result);
    }

    #[test]
    fn move_up_from_first_model_is_noop() {
        let (app, _lock) = create_test_app();
        let mut view = ModelPickerView::new(&app);
        view.selected_model_idx = 0;
        let result = view.move_up();
        assert!(!result);
    }

    #[test]
    fn immediate_esc_closes_without_apply() {
        let (app, _lock) = create_test_app();
        let mut view = ModelPickerView::new(&app);
        let action = view.handle_key(KeyEvent::new(
            KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(matches!(action, ViewAction::Close));
    }

    #[test]
    fn esc_after_selection_move_closes_without_apply() {
        let (mut app, _lock) = create_test_app();
        app.reasoning_effort = ReasoningEffort::High;
        let mut view = ModelPickerView::new(&app);
        view.handle_key(KeyEvent::new(
            KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        ));

        let action = view.handle_key(KeyEvent::new(
            KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        ));

        assert!(matches!(action, ViewAction::Close));
    }

    #[test]
    fn picker_only_exposes_auto_off_high_max() {
        let labels: Vec<&str> = PICKER_EFFORTS
            .iter()
            .map(|effort| effort.short_label())
            .collect();
        assert_eq!(labels, vec!["auto", "off", "high", "max"]);
    }
}
