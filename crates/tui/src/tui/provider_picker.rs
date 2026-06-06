//! `/provider` picker modal — pick a provider (DeepSeek / NVIDIA NIM /
//! hosted providers / self-hosted providers) and, if it lacks credentials, type the API key
//! inline before completing the switch (#52).
//!
//! The picker is intentionally a single modal with two visible states:
//!
//! 1. **List** — pick a provider; each row shows the active provider arrow
//!    and an "API key configured" / "needs API key" hint. Enter on a
//!    configured provider applies the switch immediately
//!    ([`ViewEvent::ProviderPickerApplied`]). Enter on an un-configured one
//!    transitions the same modal into the key-entry state.
//! 2. **Key entry** — masked input box pre-filled with the provider's
//!    canonical env-var name as a hint. Enter submits
//!    [`ViewEvent::ProviderPickerApiKeySubmitted`], which the UI handler
//!    persists via `save_api_key_for` before switching.
//!
//! Pressing Esc backs out: from key entry returns to the list; from the
//! list closes the modal without changes.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::config::{ApiProvider, Config, has_api_key_for, kimi_cli_credentials_present};
use crate::palette;
use crate::palette::UiTheme;
use crate::tui::views::{ModalKind, ModalView, ViewAction, ViewEvent};

fn provider_picker_nav_hint() -> &'static str {
    if palette::ascii_ui_enabled() {
        " Up/Down "
    } else {
        " \u{2191}\u{2193} "
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    List,
    KeyEntry,
}

pub struct ProviderPickerView {
    providers: Vec<(ApiProvider, bool)>,
    active_provider: ApiProvider,
    selected_idx: usize,
    stage: Stage,
    api_key_input: String,
    ui_theme: UiTheme,
}

impl ProviderPickerView {
    #[must_use]
    pub fn new(active: ApiProvider, config: &Config) -> Self {
        let providers: Vec<(ApiProvider, bool)> = ApiProvider::all()
            .iter()
            .map(|p| (*p, has_api_key_for(config, *p)))
            .collect();
        let selected_idx = providers
            .iter()
            .position(|(p, _)| *p == active)
            .unwrap_or(0);
        Self {
            providers,
            active_provider: active,
            selected_idx,
            stage: Stage::List,
            api_key_input: String::new(),
            ui_theme: palette::UI_THEME,
        }
    }

    #[must_use]
    pub fn with_ui_theme(mut self, ui_theme: UiTheme) -> Self {
        self.ui_theme = ui_theme;
        self
    }

    fn move_up(&mut self) {
        if self.providers.is_empty() {
            return;
        }
        if self.selected_idx == 0 {
            self.selected_idx = self.providers.len() - 1;
        } else {
            self.selected_idx -= 1;
        }
    }

    fn move_down(&mut self) {
        if self.providers.is_empty() {
            return;
        }
        if self.selected_idx + 1 == self.providers.len() {
            self.selected_idx = 0;
        } else {
            self.selected_idx += 1;
        }
    }

    fn selected_provider(&self) -> ApiProvider {
        self.providers[self.selected_idx].0
    }

    fn selected_has_key(&self) -> bool {
        self.providers[self.selected_idx].1
    }

    fn enter_key_entry(&mut self) {
        self.stage = Stage::KeyEntry;
        self.api_key_input.clear();
    }

    fn env_var_for(provider: ApiProvider) -> &'static str {
        match provider {
            ApiProvider::Deepseek | ApiProvider::DeepseekCN => "DEEPSEEK_API_KEY",
            ApiProvider::NvidiaNim => "NVIDIA_API_KEY",
            ApiProvider::Openai => "OPENAI_API_KEY",
            ApiProvider::Atlascloud => "ATLASCLOUD_API_KEY",
            ApiProvider::WanjieArk => "WANJIE_ARK_API_KEY",
            ApiProvider::Volcengine => "VOLCENGINE_API_KEY",
            ApiProvider::Openrouter => "OPENROUTER_API_KEY",
            ApiProvider::XiaomiMimo => "XIAOMI_MIMO_API_KEY / XIAOMI_API_KEY / MIMO_API_KEY",
            ApiProvider::Novita => "NOVITA_API_KEY",
            ApiProvider::Fireworks => "FIREWORKS_API_KEY",
            ApiProvider::Siliconflow | ApiProvider::SiliconflowCn => "SILICONFLOW_API_KEY",
            ApiProvider::Arcee => "ARCEE_API_KEY",
            ApiProvider::Moonshot => "MOONSHOT_API_KEY / KIMI_API_KEY",
            ApiProvider::Sglang => "SGLANG_API_KEY",
            ApiProvider::Vllm => "VLLM_API_KEY",
            ApiProvider::Ollama => "OLLAMA_API_KEY",
            ApiProvider::Huggingface => "HUGGINGFACE_API_KEY / HF_TOKEN",
        }
    }

    fn provider_hint(provider: ApiProvider, has_key: bool) -> String {
        match provider {
            ApiProvider::Moonshot if kimi_cli_credentials_present() => {
                "(Kimi CLI OAuth ready)".to_string()
            }
            ApiProvider::XiaomiMimo if has_key => "(configured; token-plan endpoint)".to_string(),
            ApiProvider::XiaomiMimo => {
                "(needs API key; token-plan endpoint by default)".to_string()
            }
            ApiProvider::Ollama => "self-hosted; defaults to http://localhost:11434".to_string(),
            ApiProvider::Sglang | ApiProvider::Vllm if has_key => {
                "(configured; optional key)".to_string()
            }
            ApiProvider::Sglang | ApiProvider::Vllm => "(optional key)".to_string(),
            _ if has_key => "(configured)".to_string(),
            _ => "(needs API key)".to_string(),
        }
    }

    fn visible_start(&self, visible_rows: usize) -> usize {
        if visible_rows == 0 {
            return 0;
        }
        let max_start = self.providers.len().saturating_sub(visible_rows);
        self.selected_idx
            .saturating_add(1)
            .saturating_sub(visible_rows)
            .min(max_start)
    }

    fn selected_row_style(&self, fg: Color) -> Style {
        Style::default()
            .fg(fg)
            .bg(self.ui_theme.selection_bg)
            .add_modifier(Modifier::BOLD)
    }

    fn selected_row_bg_style(&self) -> Style {
        Style::default().bg(self.ui_theme.selection_bg)
    }

    fn render_list(&self, area: Rect, buf: &mut Buffer) {
        let enter_action = if self.selected_has_key() {
            "apply"
        } else {
            "set key"
        };
        let inner = if palette::ascii_ui_enabled() {
            let footer = format!(
                "{}move Enter {enter_action} R edit key Esc cancel ",
                provider_picker_nav_hint()
            );
            render_ascii_provider_picker_chrome(area, buf, " Provider ", &footer, self.ui_theme)
        } else {
            let outer = Block::default()
                .title(Line::from(Span::styled(
                    " Provider ",
                    Style::default()
                        .fg(self.ui_theme.accent_primary)
                        .add_modifier(Modifier::BOLD),
                )))
                .title_bottom(provider_picker_footer_line(
                    &format!(
                        "{}move Enter {enter_action} R edit key Esc cancel ",
                        provider_picker_nav_hint()
                    ),
                    area.width.saturating_sub(4) as usize,
                    self.ui_theme,
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.ui_theme.border))
                .style(Style::default());
            let inner = outer.inner(area);
            outer.render(area, buf);
            inner
        };

        let visible_rows = usize::from(inner.height);
        let visible_start = self.visible_start(visible_rows);
        let mut lines: Vec<Line> = Vec::with_capacity(visible_rows);
        for (idx, (provider, has_key)) in self
            .providers
            .iter()
            .enumerate()
            .skip(visible_start)
            .take(visible_rows)
        {
            let is_selected = idx == self.selected_idx;
            let is_active = *provider == self.active_provider;
            let arrow = provider_picker_marker(is_selected);
            let active_dot = if is_active { " *" } else { "  " };
            let spacer_style = if is_selected {
                self.selected_row_bg_style()
            } else {
                Style::default()
            };
            let label_style = if is_selected {
                self.selected_row_style(self.ui_theme.selection_text)
            } else {
                Style::default().fg(self.ui_theme.text_body)
            };
            let hint_style = if is_selected {
                let hint_fg = if *has_key {
                    self.ui_theme.selection_text
                } else {
                    self.ui_theme.warning
                };
                self.selected_row_style(hint_fg)
            } else if *has_key {
                Style::default().fg(self.ui_theme.text_muted)
            } else {
                Style::default().fg(self.ui_theme.warning)
            };
            let hint = Self::provider_hint(*provider, *has_key);
            let row = provider_row_text(
                arrow,
                provider.display_name(),
                active_dot,
                &hint,
                usize::from(inner.width),
            );
            let mut line = Line::from(vec![
                Span::styled(row.prefix, spacer_style),
                Span::styled(row.name, label_style),
                Span::styled(row.active_marker, label_style),
                Span::styled(row.gap, spacer_style),
                Span::styled(row.hint, hint_style),
            ]);
            if is_selected {
                line.style = self.selected_row_bg_style();
                let target_width = usize::from(inner.width);
                let line_width = line.width();
                if line_width < target_width {
                    line.spans.push(Span::styled(
                        " ".repeat(target_width - line_width),
                        self.selected_row_bg_style(),
                    ));
                }
            }
            lines.push(line);
        }
        Paragraph::new(lines).render(inner, buf);
    }

    fn render_key_entry(&self, area: Rect, buf: &mut Buffer) {
        let provider = self.selected_provider();
        let inner = if palette::ascii_ui_enabled() {
            let title = provider_key_title(
                provider.display_name(),
                true,
                area.width.saturating_sub(4) as usize,
            );
            render_ascii_provider_picker_chrome(
                area,
                buf,
                &title,
                " Enter save & switch Esc back ",
                self.ui_theme,
            )
        } else {
            let title = provider_key_title(
                provider.display_name(),
                false,
                area.width.saturating_sub(4) as usize,
            );
            let outer = Block::default()
                .title(Line::from(Span::styled(
                    title,
                    Style::default()
                        .fg(self.ui_theme.accent_primary)
                        .add_modifier(Modifier::BOLD),
                )))
                .title_bottom(provider_picker_footer_line(
                    " Enter save & switch Esc back ",
                    area.width.saturating_sub(4) as usize,
                    self.ui_theme,
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.ui_theme.border))
                .style(Style::default());
            let inner = outer.inner(area);
            outer.render(area, buf);
            inner
        };

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(2),
                Constraint::Min(1),
            ])
            .split(inner);

        let masked = mask_key(&self.api_key_input);
        let display = if masked.is_empty() {
            "(paste key here)".to_string()
        } else {
            masked
        };
        let display = fit_key_display(&display, layout[0].width.saturating_sub(5) as usize);
        let key_lines = vec![Line::from(vec![
            Span::styled("Key: ", Style::default().fg(self.ui_theme.text_muted)),
            Span::styled(
                display,
                Style::default()
                    .fg(self.ui_theme.text_body)
                    .add_modifier(Modifier::BOLD),
            ),
        ])];
        Paragraph::new(key_lines).render(layout[0], buf);

        let hint = provider_key_env_hint(Self::env_var_for(provider), usize::from(layout[1].width));
        Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().fg(self.ui_theme.text_muted),
        )))
        .render(layout[1], buf);
    }
}

fn provider_picker_marker(selected: bool) -> &'static str {
    if !selected {
        " "
    } else if palette::ascii_ui_enabled() {
        ">"
    } else {
        "\u{25B8}"
    }
}

fn mask_key(input: &str) -> String {
    let trimmed = input.trim();
    let len = trimmed.chars().count();
    if len == 0 {
        return String::new();
    }
    if len <= 4 {
        return "*".repeat(len);
    }
    let visible: String = trimmed
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{}{}", "*".repeat(len - 4), visible)
}

fn fit_key_display(text: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width <= 3 {
        return ".".repeat(max_width);
    }

    let suffix_width = max_width.saturating_sub(3);
    let mut suffix = String::new();
    let mut width = 0usize;
    for ch in text.chars().rev() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > suffix_width {
            break;
        }
        suffix.insert(0, ch);
        width += ch_width;
    }
    format!("...{suffix}")
}

fn provider_key_env_hint(env_var: &str, max_width: usize) -> String {
    fit_row_text(
        &format!("Or set the {env_var} environment variable and re-open /provider."),
        max_width,
    )
}

fn provider_key_title(provider_name: &str, ascii: bool, max_width: usize) -> String {
    let separator = if ascii { "-" } else { "\u{2014}" };
    fit_row_text(&format!(" API key {separator} {provider_name} "), max_width)
}

struct ProviderRowText {
    prefix: String,
    name: String,
    active_marker: String,
    gap: String,
    hint: String,
}

fn provider_row_text(
    arrow: &str,
    provider_name: &str,
    active_marker: &str,
    hint: &str,
    max_width: usize,
) -> ProviderRowText {
    let prefix = format!(" {arrow} ");
    let prefix_width = UnicodeWidthStr::width(prefix.as_str());
    let content_width = max_width.saturating_sub(prefix_width);
    let name_width = UnicodeWidthStr::width(provider_name);

    if content_width <= name_width {
        return ProviderRowText {
            prefix,
            name: fit_row_text(provider_name, content_width),
            active_marker: String::new(),
            gap: String::new(),
            hint: String::new(),
        };
    }

    let mut remaining = content_width.saturating_sub(name_width);
    let active_marker_width = UnicodeWidthStr::width(active_marker);
    let active_marker = if active_marker_width <= remaining {
        remaining = remaining.saturating_sub(active_marker_width);
        active_marker.to_string()
    } else {
        String::new()
    };
    let gap = if !hint.is_empty() && remaining >= 2 {
        remaining = remaining.saturating_sub(2);
        "  ".to_string()
    } else {
        String::new()
    };

    ProviderRowText {
        prefix,
        name: provider_name.to_string(),
        active_marker,
        gap,
        hint: fit_row_text(hint, remaining),
    }
}

fn fit_row_text(text: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width <= 3 {
        return ".".repeat(max_width);
    }

    let mut out = String::new();
    let mut width = 0usize;
    let value_width = max_width.saturating_sub(3);
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > value_width {
            break;
        }
        out.push(ch);
        width += ch_width;
    }
    out.push_str("...");
    out
}

fn render_ascii_provider_picker_chrome(
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

fn provider_picker_footer_line(footer: &str, max_width: usize, theme: UiTheme) -> Line<'static> {
    Line::from(Span::styled(
        ascii_prefix(footer, max_width),
        Style::default().fg(theme.text_muted),
    ))
}

impl ModalView for ProviderPickerView {
    fn kind(&self) -> ModalKind {
        ModalKind::ProviderPicker
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_paste(&mut self, text: &str) -> bool {
        if self.stage == Stage::KeyEntry {
            let sanitized: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            if !sanitized.is_empty() {
                self.api_key_input.push_str(&sanitized);
            }
            true
        } else {
            false
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> ViewAction {
        match self.stage {
            Stage::List => match key.code {
                KeyCode::Esc => ViewAction::Close,
                KeyCode::Up => {
                    self.move_up();
                    ViewAction::None
                }
                KeyCode::Down => {
                    self.move_down();
                    ViewAction::None
                }
                KeyCode::Enter => {
                    let provider = self.selected_provider();
                    if self.selected_has_key() {
                        ViewAction::EmitAndClose(ViewEvent::ProviderPickerApplied { provider })
                    } else if provider == ApiProvider::Moonshot && kimi_cli_credentials_present() {
                        ViewAction::EmitAndClose(ViewEvent::ProviderPickerKimiOAuthEnabled {
                            provider,
                        })
                    } else {
                        self.enter_key_entry();
                        ViewAction::None
                    }
                }
                KeyCode::Char(c) if key.modifiers.is_empty() && c.eq_ignore_ascii_case(&'r') => {
                    self.enter_key_entry();
                    ViewAction::None
                }
                _ => ViewAction::None,
            },
            Stage::KeyEntry => match key.code {
                KeyCode::Esc => {
                    self.stage = Stage::List;
                    self.api_key_input.clear();
                    ViewAction::None
                }
                KeyCode::Backspace => {
                    self.api_key_input.pop();
                    ViewAction::None
                }
                KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.api_key_input.pop();
                    ViewAction::None
                }
                KeyCode::Enter => {
                    let key = self.api_key_input.trim().to_string();
                    if key.is_empty() {
                        // Stay in key-entry; the user can press Esc to abort.
                        ViewAction::None
                    } else {
                        let provider = self.selected_provider();
                        ViewAction::EmitAndClose(ViewEvent::ProviderPickerApiKeySubmitted {
                            provider,
                            api_key: key,
                        })
                    }
                }
                KeyCode::Char(c) => {
                    // Reject ASCII whitespace so a stray space/tab doesn't slip
                    // into a credential; bracketed paste happens via the input
                    // path that already trims on submit.
                    if !c.is_whitespace() {
                        self.api_key_input.push(c);
                    }
                    ViewAction::None
                }
                _ => ViewAction::None,
            },
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let popup_width = 64.min(area.width.saturating_sub(4)).max(40);
        let popup_height = match self.stage {
            Stage::List => (self.providers.len() as u16).saturating_add(2),
            Stage::KeyEntry => 10,
        }
        .min(area.height.saturating_sub(4))
        .max(8);
        let popup_area = Rect {
            x: area.x + (area.width.saturating_sub(popup_width)) / 2,
            y: area.y + (area.height.saturating_sub(popup_height)) / 2,
            width: popup_width,
            height: popup_height,
        };

        Clear.render(popup_area, buf);

        match self.stage {
            Stage::List => self.render_list(popup_area, buf),
            Stage::KeyEntry => self.render_key_entry(popup_area, buf),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};
    use unicode_width::UnicodeWidthStr;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn move_to_provider(picker: &mut ProviderPickerView, provider: ApiProvider) {
        let max_steps = picker.providers.len();
        for _ in 0..max_steps {
            if picker.selected_provider() == provider {
                return;
            }
            picker.handle_key(key(KeyCode::Down));
        }
        panic!("provider {provider:?} not found in picker");
    }

    fn render_text(picker: &ProviderPickerView, width: u16, height: u16) -> String {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        picker.render(area, &mut buf);
        (0..height)
            .map(|y| (0..width).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn picker_lists_all_providers() {
        let config = Config::default();
        let picker = ProviderPickerView::new(ApiProvider::Deepseek, &config);
        let names: Vec<_> = picker
            .providers
            .iter()
            .map(|(p, _)| p.display_name())
            .collect();
        assert_eq!(
            names,
            vec![
                "DeepSeek",
                "NVIDIA NIM",
                "OpenAI-compatible",
                "AtlasCloud",
                "Wanjie Ark",
                "Volcengine Ark",
                "OpenRouter",
                "Xiaomi MiMo",
                "Novita AI",
                "Fireworks AI",
                "SiliconFlow",
                "SiliconFlow (China)",
                "Arcee AI",
                "Moonshot/Kimi",
                "SGLang",
                "vLLM",
                "Ollama",
                "Hugging Face"
            ]
        );
    }

    #[test]
    fn ollama_is_selectable_without_key() {
        let config = Config::default();
        let mut picker = ProviderPickerView::new(ApiProvider::Deepseek, &config);
        move_to_provider(&mut picker, ApiProvider::Ollama);
        assert_eq!(picker.selected_provider(), ApiProvider::Ollama);
        assert!(picker.selected_has_key());
        let action = picker.handle_key(key(KeyCode::Enter));
        match action {
            ViewAction::EmitAndClose(ViewEvent::ProviderPickerApplied { provider }) => {
                assert_eq!(provider, ApiProvider::Ollama);
            }
            other => panic!("expected ProviderPickerApplied, got {other:?}"),
        }
    }

    #[test]
    fn picker_marks_active_provider_as_initial_selection() {
        let config = Config::default();
        let picker = ProviderPickerView::new(ApiProvider::Openrouter, &config);
        assert_eq!(picker.selected_provider(), ApiProvider::Openrouter);
        assert_eq!(picker.active_provider, ApiProvider::Openrouter);
    }

    #[test]
    fn list_navigation_wraps_between_first_and_last_provider() {
        let config = Config::default();
        let mut picker = ProviderPickerView::new(ApiProvider::Deepseek, &config);

        picker.handle_key(key(KeyCode::Up));
        assert_eq!(picker.selected_provider(), ApiProvider::Huggingface);

        picker.handle_key(key(KeyCode::Down));
        assert_eq!(picker.selected_provider(), ApiProvider::Deepseek);
    }

    #[test]
    fn enter_with_no_key_transitions_to_key_entry_stage() {
        let config = Config::default();
        let mut picker = ProviderPickerView::new(ApiProvider::Deepseek, &config);
        // Move to OpenRouter, which has no key in default config.
        move_to_provider(&mut picker, ApiProvider::Openrouter);
        assert_eq!(picker.selected_provider(), ApiProvider::Openrouter);
        let action = picker.handle_key(key(KeyCode::Enter));
        assert!(matches!(action, ViewAction::None));
        assert_eq!(picker.stage, Stage::KeyEntry);
    }

    #[test]
    fn enter_with_existing_key_emits_apply_and_closes() {
        let config = Config {
            api_key: Some("existing-deepseek-key".to_string()),
            ..Config::default()
        };
        let mut picker = ProviderPickerView::new(ApiProvider::NvidiaNim, &config);
        // Move up once to DeepSeek (index 0), which has a key from the config.
        picker.handle_key(key(KeyCode::Up));
        let action = picker.handle_key(key(KeyCode::Enter));
        match action {
            ViewAction::EmitAndClose(ViewEvent::ProviderPickerApplied { provider }) => {
                assert_eq!(provider, ApiProvider::Deepseek);
            }
            other => panic!("expected ProviderPickerApplied, got {other:?}"),
        }
    }

    #[test]
    fn configured_provider_can_reenter_key_entry_with_r() {
        let config = Config {
            providers: Some(crate::config::ProvidersConfig {
                xiaomi_mimo: crate::config::ProviderConfig {
                    api_key: Some("mimo-key".to_string()),
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Config::default()
        };
        let mut picker = ProviderPickerView::new(ApiProvider::Deepseek, &config);
        move_to_provider(&mut picker, ApiProvider::XiaomiMimo);

        let action = picker.handle_key(key(KeyCode::Char('r')));

        assert!(matches!(action, ViewAction::None));
        assert_eq!(picker.stage, Stage::KeyEntry);
        assert!(picker.api_key_input.is_empty());
    }

    #[test]
    fn ctrl_r_does_not_trigger_key_entry() {
        let config = Config::default();
        let mut picker = ProviderPickerView::new(ApiProvider::Deepseek, &config);

        let action = picker.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));

        assert!(matches!(action, ViewAction::None));
        assert_eq!(picker.stage, Stage::List);
    }

    #[test]
    fn configured_provider_footer_mentions_edit_key() {
        let config = Config {
            api_key: Some("existing-deepseek-key".to_string()),
            ..Config::default()
        };
        let picker = ProviderPickerView::new(ApiProvider::Deepseek, &config);

        let rendered = render_text(&picker, 80, 12);

        assert!(rendered.contains("Enter"));
        assert!(rendered.contains("apply"));
        assert!(rendered.contains("edit key"));
    }

    #[test]
    fn key_entry_enter_submits_after_typing() {
        let config = Config::default();
        let mut picker = ProviderPickerView::new(ApiProvider::Deepseek, &config);
        // Navigate to Novita and trigger key entry.
        move_to_provider(&mut picker, ApiProvider::Novita);
        picker.handle_key(key(KeyCode::Enter));
        assert_eq!(picker.stage, Stage::KeyEntry);
        for c in "novita-key".chars() {
            picker.handle_key(key(KeyCode::Char(c)));
        }
        let action = picker.handle_key(key(KeyCode::Enter));
        match action {
            ViewAction::EmitAndClose(ViewEvent::ProviderPickerApiKeySubmitted {
                provider,
                api_key,
            }) => {
                assert_eq!(provider, ApiProvider::Novita);
                assert_eq!(api_key, "novita-key");
            }
            other => panic!("expected ProviderPickerApiKeySubmitted, got {other:?}"),
        }
    }

    #[test]
    fn key_entry_esc_returns_to_list_without_emitting() {
        let config = Config::default();
        let mut picker = ProviderPickerView::new(ApiProvider::Deepseek, &config);
        move_to_provider(&mut picker, ApiProvider::Openrouter);
        picker.handle_key(key(KeyCode::Enter));
        assert_eq!(picker.stage, Stage::KeyEntry);
        picker.handle_key(key(KeyCode::Char('a')));
        let action = picker.handle_key(key(KeyCode::Esc));
        assert!(matches!(action, ViewAction::None));
        assert_eq!(picker.stage, Stage::List);
        assert!(picker.api_key_input.is_empty());
    }

    #[test]
    fn list_esc_closes_without_emitting() {
        let config = Config::default();
        let mut picker = ProviderPickerView::new(ApiProvider::Deepseek, &config);
        let action = picker.handle_key(key(KeyCode::Esc));
        assert!(matches!(action, ViewAction::Close));
    }

    #[test]
    fn key_entry_strips_whitespace_chars() {
        let config = Config::default();
        let mut picker = ProviderPickerView::new(ApiProvider::Deepseek, &config);
        move_to_provider(&mut picker, ApiProvider::Openrouter);
        picker.handle_key(key(KeyCode::Enter));
        assert_eq!(picker.stage, Stage::KeyEntry);
        for c in "abc def".chars() {
            picker.handle_key(key(KeyCode::Char(c)));
        }
        assert_eq!(picker.api_key_input, "abcdef");
    }

    #[test]
    fn small_list_render_keeps_selected_provider_visible_after_down_navigation() {
        let config = Config::default();
        let mut picker = ProviderPickerView::new(ApiProvider::Deepseek, &config);
        move_to_provider(&mut picker, ApiProvider::Ollama);

        let rendered = render_text(&picker, 80, 12);

        assert!(rendered.contains("Ollama"));
        assert!(!rendered.contains("DeepSeek *"));
    }

    #[test]
    fn small_list_render_keeps_initial_active_provider_visible() {
        let config = Config::default();
        let picker = ProviderPickerView::new(ApiProvider::Ollama, &config);

        let rendered = render_text(&picker, 80, 12);

        assert!(rendered.contains("Ollama *"));
    }

    #[test]
    fn tall_list_render_shows_all_providers_without_scrolling() {
        let config = Config::default();
        let picker = ProviderPickerView::new(ApiProvider::Deepseek, &config);

        let rendered = render_text(&picker, 80, 23);

        assert!(rendered.contains("DeepSeek *"));
        assert!(rendered.contains("Ollama"));
    }

    #[test]
    fn selected_provider_row_uses_strong_highlight() {
        let config = Config::default();
        let mut theme = palette::UI_THEME;
        theme.selection_bg = Color::Indexed(24);
        let picker = ProviderPickerView::new(ApiProvider::Deepseek, &config).with_ui_theme(theme);
        let area = Rect::new(0, 0, 80, 20);
        let mut buf = Buffer::empty(area);

        picker.render(area, &mut buf);

        let highlighted_cells = area
            .positions()
            .filter(|position| {
                let cell = &buf[*position];
                cell.bg == theme.selection_bg
            })
            .count();
        assert!(
            highlighted_cells >= 32,
            "selected provider row should use a visible continuous highlight"
        );
    }

    #[test]
    fn provider_picker_marker_has_ascii_fallback() {
        assert_eq!(provider_picker_marker(false), " ");
        let selected = provider_picker_marker(true);
        if palette::ascii_ui_enabled() {
            assert_eq!(selected, ">");
        } else {
            assert_eq!(selected, "\u{25B8}");
        }
    }

    #[test]
    fn key_display_truncates_to_display_width_preserving_suffix() {
        let display = fit_key_display("********************************abcd", 12);

        assert!(UnicodeWidthStr::width(display.as_str()) <= 12);
        assert_eq!(display, "...*****abcd");
    }

    #[test]
    fn key_display_handles_tiny_widths() {
        for width in 0..=3 {
            let display = fit_key_display("********abcd", width);
            assert_eq!(UnicodeWidthStr::width(display.as_str()), width);
        }
    }

    #[test]
    fn provider_key_env_hint_truncates_to_display_width() {
        let hint = provider_key_env_hint(
            "CODEWHALE_PROVIDER_\u{73af}\u{5883}\u{53d8}\u{91cf}_API_KEY",
            32,
        );

        assert!(UnicodeWidthStr::width(hint.as_str()) <= 32);
        assert!(
            hint.is_char_boundary(hint.len()),
            "provider env hint must not split UTF-8 codepoints: {hint:?}"
        );
    }

    #[test]
    fn provider_key_title_truncates_to_display_width() {
        let title = provider_key_title(
            &"\u{670d}\u{52a1}\u{5546}\u{540d}\u{79f0}".repeat(12),
            false,
            24,
        );

        assert!(UnicodeWidthStr::width(title.as_str()) <= 24);
        assert!(
            title.is_char_boundary(title.len()),
            "provider key title must not split UTF-8 codepoints: {title:?}"
        );
    }

    #[test]
    fn provider_row_text_fits_available_width() {
        let row = provider_row_text(
            ">",
            "A very long provider display name",
            " *",
            "needs API key",
            18,
        );
        let rendered = format!(
            "{}{}{}{}{}",
            row.prefix, row.name, row.active_marker, row.gap, row.hint
        );

        assert!(UnicodeWidthStr::width(rendered.as_str()) <= 18);
        assert!(rendered.contains("..."));
    }

    #[test]
    fn provider_row_text_handles_cjk_display_width() {
        let row = provider_row_text(
            ">",
            "服务商服务商服务商",
            " *",
            "需要 API key",
            16,
        );
        let rendered = format!(
            "{}{}{}{}{}",
            row.prefix, row.name, row.active_marker, row.gap, row.hint
        );

        assert!(UnicodeWidthStr::width(rendered.as_str()) <= 16);
        assert!(
            rendered.is_char_boundary(rendered.len()),
            "row must not split UTF-8 codepoints: {rendered:?}"
        );
    }

    #[test]
    fn ascii_provider_picker_chrome_uses_plain_border_chars() {
        let area = Rect::new(1, 1, 24, 8);
        let mut buf = Buffer::empty(Rect::new(0, 0, 28, 12));
        let inner = render_ascii_provider_picker_chrome(
            area,
            &mut buf,
            " Provider ",
            " Enter apply Esc cancel ",
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
        assert_eq!(inner, Rect::new(area.x + 1, area.y + 1, 22, 6));
    }

    #[test]
    fn ascii_prefix_respects_cjk_display_width() {
        let prefix = ascii_prefix(" 服务商选择 ", 8);

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
    fn provider_picker_footer_line_truncates_to_display_width() {
        let line = provider_picker_footer_line(
            " Up/Down move Enter apply R edit key Esc cancel \u{4e0a}\u{4e0b}\u{79fb}\u{52a8} ",
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
}
