//! Command palette modal for quick command/skill insertion.

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Widget, Wrap},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::commands;
use crate::localization::Locale;
use crate::palette;
use crate::palette::UiTheme;
use crate::skills;
use crate::tools::spec::ApprovalRequirement;
use crate::tools::spec::ToolCapability;
use crate::tools::{ToolContext, ToolRegistryBuilder};
use crate::tui::views::{CommandPaletteAction, ModalKind, ModalView, ViewAction, ViewEvent};

fn command_palette_move_hint() -> &'static str {
    if palette::ascii_ui_enabled() {
        " Up/Down/j/k move  "
    } else {
        " \u{2191}/\u{2193}/j/k move  "
    }
}

fn command_palette_description_separator() -> &'static str {
    if palette::ascii_ui_enabled() {
        " - "
    } else {
        " \u{2014} "
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum PaletteSection {
    Action,
    Command,
    Skill,
    Tool,
    Mcp,
}

#[derive(Debug, Clone)]
pub struct CommandPaletteEntry {
    section: PaletteSection,
    pub label: String,
    pub description: String,
    pub command: String,
    pub action: CommandPaletteAction,
}

pub struct CommandPaletteView {
    entries: Vec<CommandPaletteEntry>,
    filtered: Vec<usize>,
    query: String,
    selected: usize,
    ui_theme: UiTheme,
}

pub fn build_entries(
    locale: Locale,
    skills_dir: &Path,
    workspace: &Path,
    mcp_config_path: &Path,
    mcp_snapshot: Option<&crate::mcp::McpManagerSnapshot>,
) -> Vec<CommandPaletteEntry> {
    let mut entries = Vec::new();

    for command in commands::COMMANDS {
        let mut description = command.palette_description_for(locale);
        if command.requires_argument() {
            description.push_str("  ");
            description.push_str(command.usage);
        }
        let action = if command_runs_directly(command.name) {
            CommandPaletteAction::ExecuteCommand {
                command: format!("/{}", command.name),
            }
        } else {
            CommandPaletteAction::InsertText {
                text: command.palette_command(),
            }
        };
        entries.push(CommandPaletteEntry {
            section: PaletteSection::Command,
            label: format!("/{}", command.name),
            description,
            command: command.palette_command(),
            action,
        });
    }

    let skills = skills::discover_for_workspace_and_dir(workspace, skills_dir);
    for skill in skills.list() {
        entries.push(CommandPaletteEntry {
            section: PaletteSection::Skill,
            label: format!("skill:{}", skill.name),
            description: skill.description.clone(),
            command: format!("/skill {}", skill.name),
            action: CommandPaletteAction::ExecuteCommand {
                command: format!("/skill {}", skill.name),
            },
        });
    }

    let context = ToolContext::new(workspace);
    let registry = ToolRegistryBuilder::new()
        .with_file_tools()
        .with_search_tools()
        .with_shell_tools()
        .with_web_tools()
        .with_git_tools()
        .with_user_input_tool()
        .with_parallel_tool()
        .with_patch_tools()
        .with_note_tool()
        .with_diagnostics_tool()
        .with_project_tools()
        .with_test_runner_tool()
        .build(context);

    let mut tool_entries = registry
        .all()
        .into_iter()
        .filter_map(|tool| {
            let name = tool.name().to_string();
            let capabilities = tool.capabilities();

            let mut tags = Vec::new();
            if tool.is_read_only() {
                tags.push("read-only");
            }
            if capabilities.contains(&ToolCapability::WritesFiles) {
                tags.push("writes");
            }
            if capabilities.contains(&ToolCapability::ExecutesCode) {
                tags.push("shell");
            }
            if capabilities.contains(&ToolCapability::Network) {
                tags.push("network");
            }
            if tool.supports_parallel() {
                tags.push("parallel");
            }
            match tool.approval_requirement() {
                ApprovalRequirement::Required => tags.push("requires approval"),
                ApprovalRequirement::Suggest => tags.push("suggest approval"),
                ApprovalRequirement::Auto => {}
            }

            let mut description = tool.description().to_string();
            if !tags.is_empty() {
                description.push_str(" [");
                description.push_str(&tags.join(", "));
                description.push(']');
            }

            if name.trim().is_empty() {
                return None;
            }
            Some(CommandPaletteEntry {
                section: PaletteSection::Tool,
                label: format!("tool:{name}"),
                description: description.clone(),
                command: name,
                action: CommandPaletteAction::OpenTextPager {
                    title: format!("Tool: {}", tool.name()),
                    content: format_tool_details(tool.name(), tool.description(), &tags),
                },
            })
        })
        .collect::<Vec<_>>();
    tool_entries.sort_by(|a, b| a.label.cmp(&b.label));
    entries.extend(tool_entries);

    entries.extend(build_mcp_entries(mcp_config_path, mcp_snapshot));

    entries.sort_by(|a, b| a.label.cmp(&b.label));
    entries.sort_by_key(|entry| entry.section);
    entries
}

fn build_mcp_entries(
    mcp_config_path: &Path,
    mcp_snapshot: Option<&crate::mcp::McpManagerSnapshot>,
) -> Vec<CommandPaletteEntry> {
    let owned_snapshot = if mcp_snapshot.is_none() {
        crate::mcp::manager_snapshot_from_config(mcp_config_path, false).ok()
    } else {
        None
    };
    let snapshot = mcp_snapshot.or(owned_snapshot.as_ref());
    let mut entries = vec![CommandPaletteEntry {
        section: PaletteSection::Mcp,
        label: "mcp:manager".to_string(),
        description: format!("Open MCP manager ({})", mcp_config_path.display()),
        command: "/mcp".to_string(),
        action: CommandPaletteAction::ExecuteCommand {
            command: "/mcp".to_string(),
        },
    }];

    let Some(snapshot) = snapshot else {
        return entries;
    };

    for server in &snapshot.servers {
        let state = if server.enabled {
            if server.connected {
                "connected"
            } else if server.error.is_some() {
                "failed"
            } else {
                "enabled"
            }
        } else {
            "disabled"
        };
        entries.push(CommandPaletteEntry {
            section: PaletteSection::Mcp,
            label: format!("mcp:{}", server.name),
            description: format!(
                "{} {} [{}] tools={} resources={} prompts={}",
                server.transport,
                server.command_or_url,
                state,
                server.tools.len(),
                server.resources.len(),
                server.prompts.len()
            ),
            command: format!("/mcp show {}", server.name),
            action: CommandPaletteAction::OpenTextPager {
                title: format!("MCP Server: {}", server.name),
                content: format_mcp_server_details(snapshot, server),
            },
        });

        for tool in &server.tools {
            entries.push(CommandPaletteEntry {
                section: PaletteSection::Mcp,
                label: format!("mcp:{}:tool:{}", server.name, tool.name),
                description: format!(
                    "{}{}",
                    tool.model_name,
                    tool.description
                        .as_ref()
                        .map_or(String::new(), |desc| format!(" - {desc}"))
                ),
                command: tool.model_name.clone(),
                action: CommandPaletteAction::OpenTextPager {
                    title: format!("MCP Tool: {}", tool.model_name),
                    content: format!(
                        "Server: {}\nRuntime name: {}\nKind: tool\n\n{}",
                        server.name,
                        tool.model_name,
                        tool.description.as_deref().unwrap_or("(no description)")
                    ),
                },
            });
            // Add a "use" entry that inserts the tool's model_name into the input
            // so users can quickly reference the tool in their message to the AI.
            if !tool.model_name.trim().is_empty() {
                entries.push(CommandPaletteEntry {
                    section: PaletteSection::Mcp,
                    label: format!("mcp:{}:tool:{} > use", server.name, tool.name),
                    description: format!(
                        "Insert {} into input{}type args then send{}",
                        tool.model_name,
                        command_palette_description_separator(),
                        tool.description
                            .as_ref()
                            .map_or(String::new(), |desc| format!(" ({desc})"))
                    ),
                    command: tool.model_name.clone(),
                    action: CommandPaletteAction::InsertText {
                        text: tool.model_name.clone(),
                    },
                });
            }
        }

        for resource in &server.resources {
            entries.push(CommandPaletteEntry {
                section: PaletteSection::Mcp,
                label: format!("mcp:{}:resource:{}", server.name, resource.name),
                description: resource
                    .description
                    .clone()
                    .unwrap_or_else(|| "MCP resource".to_string()),
                command: resource.name.clone(),
                action: CommandPaletteAction::OpenTextPager {
                    title: format!("MCP Resource: {}", resource.name),
                    content: format!(
                        "Server: {}\nResource: {}\nModel helper: list_mcp_resources / read_mcp_resource",
                        server.name, resource.name
                    ),
                },
            });
        }

        for prompt in &server.prompts {
            entries.push(CommandPaletteEntry {
                section: PaletteSection::Mcp,
                label: format!("mcp:{}:prompt:{}", server.name, prompt.name),
                description: format!(
                    "{}{}",
                    prompt.model_name,
                    prompt
                        .description
                        .as_ref()
                        .map_or(String::new(), |desc| format!(" - {desc}"))
                ),
                command: prompt.model_name.clone(),
                action: CommandPaletteAction::OpenTextPager {
                    title: format!("MCP Prompt: {}", prompt.model_name),
                    content: format!(
                        "Server: {}\nRuntime name: {}\nKind: prompt",
                        server.name, prompt.model_name
                    ),
                },
            });
        }
    }

    entries
}

fn format_mcp_server_details(
    snapshot: &crate::mcp::McpManagerSnapshot,
    server: &crate::mcp::McpServerSnapshot,
) -> String {
    let mut lines = vec![
        format!("Config: {}", snapshot.config_path.display()),
        format!("Server: {}", server.name),
        format!("Enabled: {}", server.enabled),
        format!("Connected: {}", server.connected),
        format!("Transport: {}", server.transport),
        format!("Target: {}", server.command_or_url),
        format!(
            "Timeouts: connect={}s execute={}s read={}s",
            server.connect_timeout, server.execute_timeout, server.read_timeout
        ),
    ];
    if let Some(error) = server.error.as_ref() {
        lines.push(format!("Error: {error}"));
    }
    lines.push(String::new());
    lines.push(format!("Tools ({})", server.tools.len()));
    for tool in &server.tools {
        lines.push(format!("  - {}", tool.model_name));
    }
    lines.push(format!("Resources ({})", server.resources.len()));
    for resource in &server.resources {
        lines.push(format!("  - {}", resource.name));
    }
    lines.push(format!("Prompts ({})", server.prompts.len()));
    for prompt in &server.prompts {
        lines.push(format!("  - {}", prompt.model_name));
    }
    lines.join("\n")
}

fn modal_block(ui_theme: UiTheme) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ui_theme.border))
        .padding(Padding::uniform(1))
}

fn command_palette_bottom_hint() -> &'static str {
    if palette::ascii_ui_enabled() {
        " Up/Down/j/k move  Enter run/open  Esc close "
    } else {
        " \u{2191}/\u{2193}/j/k move  Enter run/open  Esc close "
    }
}

fn render_ascii_command_palette_chrome(
    area: Rect,
    buf: &mut Buffer,
    ui_theme: UiTheme,
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
    let border_style = Style::default().fg(ui_theme.border).bg(ui_theme.surface_bg);
    let title_style = Style::default()
        .fg(ui_theme.status_working)
        .bg(ui_theme.surface_bg)
        .add_modifier(Modifier::BOLD);
    let hint_style = Style::default()
        .fg(ui_theme.text_muted)
        .bg(ui_theme.surface_bg);

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
        let title = ascii_prefix(" Command Palette ", area.width.saturating_sub(4) as usize);
        buf.set_string(area.x + 2, area.y, &title, title_style);
    }
    if area.width > 8 && area.height > 1 {
        let hint = ascii_prefix(
            command_palette_bottom_hint(),
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

fn parse_section_term(term: &str) -> Option<(PaletteSection, String)> {
    let (section, query) = term.split_once(':')?;

    if section.is_empty() || query.is_empty() {
        return None;
    }

    let query = query.to_ascii_lowercase();
    let section = match section {
        "a" | "action" | "actions" => PaletteSection::Action,
        "c" | "cmd" | "command" | "commands" => PaletteSection::Command,
        "s" | "skill" | "skills" => PaletteSection::Skill,
        "t" | "tool" | "tools" => PaletteSection::Tool,
        "m" | "mcp" => PaletteSection::Mcp,
        _ => return None,
    };

    Some((section, query))
}

fn section_tag(section: PaletteSection) -> &'static str {
    match section {
        PaletteSection::Action => "action",
        PaletteSection::Command => "command",
        PaletteSection::Skill => "skill",
        PaletteSection::Tool => "tool",
        PaletteSection::Mcp => "mcp",
    }
}

fn section_rank(section: PaletteSection) -> usize {
    match section {
        PaletteSection::Action => 0,
        PaletteSection::Command => 1,
        PaletteSection::Skill => 2,
        PaletteSection::Tool => 3,
        PaletteSection::Mcp => 4,
    }
}

fn command_palette_ellipsis(max_width: usize) -> &'static str {
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

fn truncate_to_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if text.width() <= max_width {
        return text.to_string();
    }

    let ellipsis = command_palette_ellipsis(max_width);
    let ellipsis_width = ellipsis.width();
    let limit = max_width.saturating_sub(ellipsis_width);
    let mut out = String::new();
    let mut width = 0usize;
    for ch in text.chars() {
        let ch_width = ch.width().unwrap_or(0);
        if width + ch_width > limit {
            break;
        }
        out.push(ch);
        width += ch_width;
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

fn command_runs_directly(name: &str) -> bool {
    matches!(
        name,
        "help"
            | "clear"
            | "exit"
            | "models"
            | "queue"
            | "stash"
            | "hooks"
            | "subagents"
            | "links"
            | "home"
            | "save"
            | "sessions"
            | "compact"
            | "export"
            | "config"
            | "yolo"
            | "agent"
            | "plan"
            | "trust"
            | "logout"
            | "tokens"
            | "change"
            | "system"
            | "context"
            | "undo"
            | "retry"
            | "init"
            | "settings"
            | "skills"
            | "cost"
            | "jobs"
            | "mcp"
            | "task"
    )
}

fn format_tool_details(name: &str, description: &str, tags: &[&str]) -> String {
    let mut lines = vec![
        format!("Tool: {name}"),
        String::new(),
        description.to_string(),
    ];
    if !tags.is_empty() {
        lines.push(String::new());
        lines.push(format!("Capabilities: {}", tags.join(", ")));
    }
    lines.push(String::new());
    lines.push(
        "Use slash commands and skills here for direct actions; use tool entries to inspect what the agent can call."
            .to_string(),
    );
    lines.join("\n")
}

fn term_score(term: &str, label: &str, description: &str, command: &str, haystack: &str) -> usize {
    if term.is_empty() {
        return 0;
    }

    if label == term || command == term || description == term {
        return 0;
    }

    if label.starts_with(term) {
        return 8;
    }

    if command.starts_with(term) {
        return 16;
    }

    if description.contains(term) {
        return 64;
    }

    if label.contains(term) {
        return 32;
    }

    if command.contains(term) {
        return 48;
    }

    if haystack.contains(term) {
        return 96;
    }

    128
}

fn entry_match_score(entry: &CommandPaletteEntry, terms: &[&str]) -> Option<usize> {
    if terms.is_empty() {
        return Some(0);
    }

    let section = section_tag(entry.section);
    let label = entry.label.to_ascii_lowercase();
    let description = entry.description.to_ascii_lowercase();
    let command = entry.command.to_ascii_lowercase();
    let entry_text = format!("{section} {label} {description} {command}");

    let mut total_score = 0usize;

    for term in terms {
        if let Some((required_section, scoped_query)) = parse_section_term(term) {
            if entry.section != required_section {
                return None;
            }
            if !entry_text.contains(&scoped_query) {
                return None;
            }
            total_score += term_score(&scoped_query, &label, &description, &command, &entry_text);
            continue;
        }

        if !entry_text.contains(term) {
            return None;
        }
        total_score += term_score(term, &label, &description, &command, &entry_text);
    }

    Some(total_score)
}

/// Number of rendered rows the entry loop consumes for the window
/// `sections[start..end]`: one row per entry, plus one section-label row each
/// time the section changes, plus a separator blank before every section group
/// after the first.
fn rendered_entry_rows(sections: &[PaletteSection], start: usize, end: usize) -> usize {
    let end = end.min(sections.len());
    if start >= end {
        return 0;
    }
    let mut rows = 0usize;
    let mut active: Option<PaletteSection> = None;
    for (slot, sec) in sections[start..end].iter().enumerate() {
        if active != Some(*sec) {
            if slot > 0 {
                rows += 1; // separator blank
            }
            rows += 1; // section label
            active = Some(*sec);
        }
        rows += 1; // the entry itself
    }
    rows
}

/// Compute the `[start, end)` window of filtered entries to render so that the
/// selected entry is always visible and the rendered rows — entries plus the
/// per-section labels and separators inserted between them — fit within
/// `available` rows.
///
/// The previous logic sized the window purely by entry count (`popup_height -
/// 7`) while the same fixed-height area also held the header, section labels,
/// and separators. Those uncounted rows pushed the selection past the bottom
/// clip line, so it vanished and the list appeared frozen until the index
/// finally exceeded the (overlarge) entry budget (#2590).
fn visible_entry_window(
    sections: &[PaletteSection],
    selected: usize,
    available: usize,
) -> (usize, usize) {
    let total = sections.len();
    if total == 0 || available == 0 {
        return (0, 0);
    }
    let selected = selected.min(total - 1);
    // Always include the selected row, then greedily grow downward and upward
    // while the fully-rendered window still fits. Growth only ever adds rows,
    // so the greedy expansion terminates at the largest fitting window.
    let mut start = selected;
    let mut end = selected + 1;
    loop {
        let mut progressed = false;
        if end < total && rendered_entry_rows(sections, start, end + 1) <= available {
            end += 1;
            progressed = true;
        }
        if start > 0 && rendered_entry_rows(sections, start - 1, end) <= available {
            start -= 1;
            progressed = true;
        }
        if !progressed {
            break;
        }
    }
    (start, end)
}

impl CommandPaletteView {
    pub fn new(entries: Vec<CommandPaletteEntry>) -> Self {
        let mut view = Self {
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

    fn refilter(&mut self) {
        let query = self.query.trim().to_ascii_lowercase();
        let terms: Vec<&str> = query
            .split_whitespace()
            .filter(|term| !term.is_empty())
            .collect();

        let mut filtered = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(idx, entry)| entry_match_score(entry, &terms).map(|score| (idx, score)))
            .collect::<Vec<_>>();

        filtered.sort_by_key(|(idx, score)| {
            let entry = &self.entries[*idx];
            (section_rank(entry.section), *score, &entry.label)
        });
        self.filtered = filtered.into_iter().map(|(idx, _)| idx).collect();
        if self.selected >= self.filtered.len() {
            self.selected = 0;
        }
    }

    fn scope_hint_lines(&self) -> Line<'static> {
        let hint = "scope: c:/cmd: , s:/skill: , t:/tool: , m:/mcp:";
        Line::from(Span::styled(
            hint,
            Style::default()
                .fg(self.ui_theme.text_dim)
                .add_modifier(Modifier::ITALIC),
        ))
    }

    fn format_section_label(&self, section: PaletteSection, count: usize) -> Line<'static> {
        let title = match section {
            PaletteSection::Action => "Actions",
            PaletteSection::Command => "Commands",
            PaletteSection::Skill => "Skills",
            PaletteSection::Tool => "Tools",
            PaletteSection::Mcp => "MCP",
        };
        Line::from(vec![Span::styled(
            format!("  {title} ({count})  "),
            Style::default()
                .fg(self.ui_theme.accent_primary)
                .add_modifier(Modifier::BOLD),
        )])
    }

    fn scope_examples(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(Span::styled(
                "Try:",
                Style::default().fg(self.ui_theme.text_dim),
            )),
            Line::from(Span::styled(
                "  c:<term>  Command-only   e.g. c:agent",
                Style::default().fg(self.ui_theme.text_muted),
            )),
            Line::from(Span::styled(
                "  s:<term>  Skill-only     e.g. s:search",
                Style::default().fg(self.ui_theme.text_muted),
            )),
            Line::from(Span::styled(
                "  t:<term>  Tool-only      e.g. t:git",
                Style::default().fg(self.ui_theme.text_muted),
            )),
            Line::from(Span::styled(
                "  m:<term>  MCP-only       e.g. m:filesystem",
                Style::default().fg(self.ui_theme.text_muted),
            )),
        ]
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

    fn selected_entry(&self) -> Option<&CommandPaletteEntry> {
        self.filtered
            .get(self.selected)
            .and_then(|idx| self.entries.get(*idx))
    }
}

impl ModalView for CommandPaletteView {
    fn kind(&self) -> ModalKind {
        ModalKind::CommandPalette
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Esc => ViewAction::Close,
            KeyCode::Enter => {
                if let Some(entry) = self.selected_entry() {
                    ViewAction::EmitAndClose(ViewEvent::CommandPaletteSelected {
                        action: entry.action.clone(),
                    })
                } else {
                    ViewAction::None
                }
            }
            KeyCode::Up => {
                self.move_selection(-1);
                ViewAction::None
            }
            KeyCode::Down => {
                self.move_selection(1);
                ViewAction::None
            }
            KeyCode::Char('k') if self.query.is_empty() => {
                self.move_selection(-1);
                ViewAction::None
            }
            KeyCode::Char('j') if self.query.is_empty() => {
                self.move_selection(1);
                ViewAction::None
            }
            KeyCode::PageUp => {
                self.move_selection(-8);
                ViewAction::None
            }
            KeyCode::PageDown => {
                self.move_selection(8);
                ViewAction::None
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.refilter();
                ViewAction::None
            }
            // Ctrl+H is the legacy ASCII backspace many terminals emit.
            KeyCode::Char('h')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                self.query.pop();
                self.refilter();
                ViewAction::None
            }
            KeyCode::Char(c)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
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
        let popup_height = 22.min(area.height.saturating_sub(4));
        let popup_area = Rect {
            x: (area.width.saturating_sub(popup_width)) / 2,
            y: (area.height.saturating_sub(popup_height)) / 2,
            width: popup_width,
            height: popup_height,
        };

        Clear.render(popup_area, buf);
        let content_area = if palette::ascii_ui_enabled() {
            render_ascii_command_palette_chrome(popup_area, buf, self.ui_theme)
        } else {
            let block = modal_block(self.ui_theme)
                .title(" Command Palette ")
                .title_bottom(Line::from(vec![
                    Span::styled(
                        command_palette_move_hint(),
                        Style::default().fg(self.ui_theme.text_muted),
                    ),
                    Span::styled(
                        "Enter run/open  ",
                        Style::default().fg(self.ui_theme.text_muted),
                    ),
                    Span::styled("Esc close", Style::default().fg(self.ui_theme.text_muted)),
                ]));
            let inner = block.inner(popup_area);
            block.render(popup_area, buf);
            inner
        };

        let mut lines = Vec::new();
        let query_label = if self.query.is_empty() {
            "Type to filter".to_string()
        } else {
            format!("Filter: {}", self.query)
        };
        lines.push(Line::from(Span::styled(
            query_label,
            Style::default().fg(self.ui_theme.text_muted),
        )));
        let match_count = if self.query.is_empty() {
            format!("{} entries", self.entries.len())
        } else {
            format!("{} / {} matches", self.filtered.len(), self.entries.len())
        };
        lines.push(Line::from(Span::styled(
            match_count,
            Style::default().fg(self.ui_theme.text_dim).italic(),
        )));
        lines.push(self.scope_hint_lines());
        lines.extend(self.scope_examples());
        lines.push(Line::from(""));

        // Rows the bordered popup can show for the list, minus the header that
        // was already pushed above. The entry loop additionally emits section
        // labels and separators, so the scroll window is sized against the real
        // rendered cost rather than a flat entry count (#2590).
        let header_lines = lines.len();
        let available = (content_area.height as usize).saturating_sub(header_lines);
        let mut action_count = 0usize;
        let mut command_count = 0usize;
        let mut skill_count = 0usize;
        let mut tool_count = 0usize;
        let mut mcp_count = 0usize;
        for idx in &self.filtered {
            match self.entries[*idx].section {
                PaletteSection::Action => action_count += 1,
                PaletteSection::Command => command_count += 1,
                PaletteSection::Skill => skill_count += 1,
                PaletteSection::Tool => tool_count += 1,
                PaletteSection::Mcp => mcp_count += 1,
            }
        }
        if self.filtered.is_empty() {
            lines.push(Line::from(Span::styled(
                "No matches.",
                Style::default().fg(self.ui_theme.text_muted).italic(),
            )));
        } else {
            let label_width = 24.min(content_area.width.saturating_sub(26) as usize);
            let sections: Vec<PaletteSection> = self
                .filtered
                .iter()
                .map(|idx| self.entries[*idx].section)
                .collect();
            let (start, end) = visible_entry_window(&sections, self.selected, available);
            let mut active_section = None;
            for (slot, idx) in self.filtered[start..end].iter().enumerate() {
                let absolute = start + slot;
                let is_selected = absolute == self.selected;
                let entry = &self.entries[*idx];

                if active_section != Some(entry.section) {
                    if slot > 0 {
                        lines.push(Line::from(""));
                    }
                    let count = match entry.section {
                        PaletteSection::Action => action_count,
                        PaletteSection::Command => command_count,
                        PaletteSection::Skill => skill_count,
                        PaletteSection::Tool => tool_count,
                        PaletteSection::Mcp => mcp_count,
                    };
                    lines.push(self.format_section_label(entry.section, count));
                    active_section = Some(entry.section);
                }

                let style = if is_selected {
                    Style::default()
                        .fg(self.ui_theme.selection_text)
                        .bg(self.ui_theme.selection_bg)
                } else {
                    Style::default().fg(self.ui_theme.text_body)
                };

                let desc_capacity = (content_area.width as usize).saturating_sub(label_width + 4);
                let label = pad_to_display_width(
                    truncate_to_width(&entry.label, label_width),
                    label_width,
                );
                let desc = truncate_to_width(&entry.description, desc_capacity);
                let mut line = if is_selected {
                    format!("> {label}")
                } else {
                    format!("  {label}")
                };
                line.push_str("  ");
                line.push_str(&desc);
                lines.push(Line::from(Span::styled(line, style)));
            }
        }

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(content_area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn visible_window_keeps_selection_in_view_and_fits() {
        // Single large section, small budget: every selection must stay visible
        // and the rendered window must fit the available rows (#2590).
        let sections = vec![PaletteSection::Command; 30];
        let available = 10;
        for selected in 0..sections.len() {
            let (start, end) = visible_entry_window(&sections, selected, available);
            assert!(
                start <= selected && selected < end,
                "selected {selected} must lie within [{start}, {end})"
            );
            assert!(
                rendered_entry_rows(&sections, start, end) <= available,
                "window [{start}, {end}) must fit within {available} rows"
            );
        }
    }

    #[test]
    fn visible_window_scrolls_as_selection_advances() {
        let sections = vec![PaletteSection::Command; 30];
        let available = 8;
        let (start_near, _) = visible_entry_window(&sections, 0, available);
        assert_eq!(start_near, 0);
        // A far-down selection must advance the window start — the old code
        // left it pinned at 0 so the selection scrolled off-screen.
        let (start_far, end_far) = visible_entry_window(&sections, 25, available);
        assert!(start_far > 0, "window should scroll for a far selection");
        assert!(start_far <= 25 && 25 < end_far);
    }

    #[test]
    fn visible_window_accounts_for_section_overhead() {
        // Each entry is its own section, so each costs a label (plus a
        // separator after the first) on top of the entry row. Far fewer than
        // `available` entries fit, and the window must still respect the budget.
        let sections = vec![
            PaletteSection::Action,
            PaletteSection::Command,
            PaletteSection::Skill,
            PaletteSection::Tool,
            PaletteSection::Mcp,
        ];
        let available = 6;
        let (start, end) = visible_entry_window(&sections, 0, available);
        assert_eq!(start, 0);
        assert!(end >= 1, "at least the selected entry must render");
        assert!(rendered_entry_rows(&sections, start, end) <= available);
    }

    #[test]
    fn visible_window_handles_empty_and_zero_budget() {
        assert_eq!(visible_entry_window(&[], 0, 10), (0, 0));
        let sections = vec![PaletteSection::Command; 5];
        assert_eq!(visible_entry_window(&sections, 2, 0), (0, 0));
    }

    fn palette_entry(
        section: PaletteSection,
        label: &str,
        description: &str,
        command: &str,
    ) -> CommandPaletteEntry {
        CommandPaletteEntry {
            section,
            label: label.to_string(),
            description: description.to_string(),
            command: command.to_string(),
            action: CommandPaletteAction::InsertText {
                text: command.to_string(),
            },
        }
    }

    #[test]
    fn command_palette_filters_with_section_shortcuts() {
        let entries = vec![
            palette_entry(PaletteSection::Command, "/mode", "mode command", "/mode"),
            palette_entry(
                PaletteSection::Skill,
                "skill:search",
                "search skill",
                "/skill search",
            ),
            palette_entry(PaletteSection::Tool, "tool:git", "git tool", "git"),
            palette_entry(
                PaletteSection::Tool,
                "tool:search",
                "search utility",
                "search",
            ),
            palette_entry(PaletteSection::Mcp, "mcp:fs", "filesystem", "mcp_fs_read"),
        ];
        let mut view = CommandPaletteView::new(entries);

        view.query = "c:mode".to_string();
        view.refilter();
        assert_eq!(view.filtered, vec![0]);

        view.query = "s:search".to_string();
        view.refilter();
        assert_eq!(view.filtered, vec![1]);

        view.query = "t:search".to_string();
        view.refilter();
        assert_eq!(view.filtered, vec![3]);

        view.query = "m:fs".to_string();
        view.refilter();
        assert_eq!(view.filtered, vec![4]);
    }

    #[test]
    fn command_palette_ranks_label_matches_before_description_matches() {
        let entries = vec![
            palette_entry(
                PaletteSection::Command,
                "/git",
                "status summary for repository",
                "git",
            ),
            palette_entry(
                PaletteSection::Command,
                "/config",
                "configure git settings",
                "config",
            ),
            palette_entry(
                PaletteSection::Command,
                "/sync",
                "sync repository state",
                "sync",
            ),
        ];
        let mut view = CommandPaletteView::new(entries);

        view.query = "git".to_string();
        view.refilter();

        assert_eq!(view.entries[view.filtered[0]].label, "/git");
        assert_eq!(view.entries[view.filtered[1]].label, "/config");
    }

    #[test]
    fn command_palette_supports_multiple_terms() {
        let entries = vec![
            palette_entry(
                PaletteSection::Command,
                "/search-code",
                "search with ripgrep",
                "search code",
            ),
            palette_entry(
                PaletteSection::Tool,
                "tool:search",
                "search web and files",
                "search",
            ),
            palette_entry(
                PaletteSection::Skill,
                "skill:search",
                "search files and docs",
                "/skill search",
            ),
        ];
        let mut view = CommandPaletteView::new(entries);

        view.query = "search code".to_string();
        view.refilter();
        assert_eq!(view.filtered.len(), 1);
        assert_eq!(view.entries[view.filtered[0]].label, "/search-code");

        view.query = "s:search".to_string();
        view.refilter();
        assert_eq!(view.filtered.len(), 1);
        assert_eq!(view.entries[view.filtered[0]].label, "skill:search");
    }

    #[test]
    fn command_palette_skills_use_workspace_and_configured_directories() {
        let tmp = TempDir::new().expect("tempdir");
        let workspace = tmp.path().join("workspace");
        let workspace_skill_dir = workspace
            .join(".agents")
            .join("skills")
            .join("workspace-skill");
        std::fs::create_dir_all(&workspace_skill_dir).expect("create workspace skill dir");
        std::fs::write(
            workspace_skill_dir.join("SKILL.md"),
            "---\nname: workspace-skill\ndescription: Workspace skill\ngithub: https://example.com\n---\nbody",
        )
        .expect("write workspace skill");

        let configured_dir = tmp.path().join("configured-skills");
        let configured_skill_dir = configured_dir.join("configured-skill");
        std::fs::create_dir_all(&configured_skill_dir).expect("create configured skill dir");
        std::fs::write(
            configured_skill_dir.join("SKILL.md"),
            "---\nname: configured-skill\ndescription: Configured skill\n---\nbody",
        )
        .expect("write configured skill");

        let entries = build_entries(
            Locale::En,
            configured_dir.as_path(),
            workspace.as_path(),
            Path::new("mcp.json"),
            None,
        );
        let skill_labels = entries
            .iter()
            .filter(|entry| entry.section == PaletteSection::Skill)
            .map(|entry| entry.label.as_str())
            .collect::<Vec<_>>();

        assert!(skill_labels.contains(&"skill:workspace-skill"));
        assert!(skill_labels.contains(&"skill:configured-skill"));
    }

    #[test]
    fn command_palette_command_entries_include_links_and_config_but_not_removed_commands() {
        let entries = build_entries(
            Locale::En,
            Path::new("."),
            Path::new("."),
            Path::new("mcp.json"),
            None,
        );
        let command_labels = entries
            .iter()
            .filter(|entry| entry.section == PaletteSection::Command)
            .map(|entry| entry.label.as_str())
            .collect::<Vec<_>>();

        assert!(command_labels.contains(&"/config"));
        assert!(command_labels.contains(&"/links"));
        assert!(!command_labels.contains(&"/voice"));
        assert!(!command_labels.contains(&"/set"));
        assert!(!command_labels.contains(&"/deepseek"));
    }

    #[test]
    fn command_palette_inserts_model_command_for_argument_entry() {
        let entries = build_entries(
            Locale::En,
            Path::new("."),
            Path::new("."),
            Path::new("mcp.json"),
            None,
        );
        let model = entries
            .iter()
            .find(|entry| entry.section == PaletteSection::Command && entry.label == "/model")
            .expect("model command entry");

        assert_eq!(model.command, "/model ");
        assert!(matches!(
            &model.action,
            CommandPaletteAction::InsertText { text } if text == "/model "
        ));
    }

    #[test]
    fn command_palette_runs_change_without_requiring_version() {
        let entries = build_entries(
            Locale::En,
            Path::new("."),
            Path::new("."),
            Path::new("mcp.json"),
            None,
        );
        let change = entries
            .iter()
            .find(|entry| entry.section == PaletteSection::Command && entry.label == "/change")
            .expect("change command entry");

        assert!(matches!(
            &change.action,
            CommandPaletteAction::ExecuteCommand { command } if command == "/change"
        ));
    }

    #[test]
    fn command_palette_includes_mcp_discovery_and_failed_servers() {
        let snapshot = crate::mcp::McpManagerSnapshot {
            config_path: Path::new("mcp.json").to_path_buf(),
            config_exists: true,
            restart_required: false,
            servers: vec![
                crate::mcp::McpServerSnapshot {
                    name: "fs".to_string(),
                    enabled: true,
                    required: false,
                    transport: "stdio".to_string(),
                    command_or_url: "node server.js".to_string(),
                    connect_timeout: 10,
                    execute_timeout: 60,
                    read_timeout: 120,
                    connected: true,
                    error: None,
                    tools: vec![crate::mcp::McpDiscoveredItem {
                        name: "read".to_string(),
                        model_name: "mcp_fs_read".to_string(),
                        description: Some("Read files".to_string()),
                    }],
                    resources: Vec::new(),
                    prompts: Vec::new(),
                },
                crate::mcp::McpServerSnapshot {
                    name: "broken".to_string(),
                    enabled: true,
                    required: false,
                    transport: "http/sse".to_string(),
                    command_or_url: "https://example.invalid/mcp".to_string(),
                    connect_timeout: 10,
                    execute_timeout: 60,
                    read_timeout: 120,
                    connected: false,
                    error: Some("connect failed".to_string()),
                    tools: Vec::new(),
                    resources: Vec::new(),
                    prompts: Vec::new(),
                },
            ],
        };
        let entries = build_entries(
            Locale::En,
            Path::new("."),
            Path::new("."),
            Path::new("mcp.json"),
            Some(&snapshot),
        );

        assert!(entries.iter().any(|entry| entry.label == "mcp:manager"));
        assert!(entries.iter().any(|entry| entry.command == "mcp_fs_read"));
        let failed = entries
            .iter()
            .find(|entry| entry.label == "mcp:broken")
            .expect("failed server visible");
        assert!(failed.description.contains("failed"));

        // Verify the "use" insert entry for MCP tools
        let use_entry = entries
            .iter()
            .find(|entry| entry.label == "mcp:fs:tool:read > use")
            .expect("MCP tool use entry should exist");
        assert!(matches!(
            &use_entry.action,
            CommandPaletteAction::InsertText { text } if text == "mcp_fs_read"
        ));
        assert_eq!(use_entry.command, "mcp_fs_read");
    }

    #[test]
    fn command_palette_description_separator_has_ascii_fallback() {
        crate::palette::set_ascii_ui_for_tests(Some(false));
        assert_eq!(command_palette_description_separator(), " \u{2014} ");

        crate::palette::set_ascii_ui_for_tests(Some(true));
        assert_eq!(command_palette_description_separator(), " - ");

        crate::palette::set_ascii_ui_for_tests(None);
    }

    #[test]
    fn command_palette_marks_disabled_servers_visibly() {
        // The healthy/failed cases are covered above; disabled was the
        // remaining gap from #197's acceptance list. Disabled servers must
        // appear in the palette with a `[disabled]` state tag so users can
        // see them without opening the MCP manager.
        let snapshot = crate::mcp::McpManagerSnapshot {
            config_path: Path::new("mcp.json").to_path_buf(),
            config_exists: true,
            restart_required: false,
            servers: vec![crate::mcp::McpServerSnapshot {
                name: "muted".to_string(),
                enabled: false,
                required: false,
                transport: "stdio".to_string(),
                command_or_url: "node disabled.js".to_string(),
                connect_timeout: 10,
                execute_timeout: 60,
                read_timeout: 120,
                connected: false,
                error: None,
                tools: Vec::new(),
                resources: Vec::new(),
                prompts: Vec::new(),
            }],
        };
        let entries = build_entries(
            Locale::En,
            Path::new("."),
            Path::new("."),
            Path::new("mcp.json"),
            Some(&snapshot),
        );

        let muted = entries
            .iter()
            .find(|entry| entry.label == "mcp:muted")
            .expect("disabled server should still appear in the palette");
        assert!(
            muted.description.contains("[disabled]"),
            "expected `[disabled]` state tag in description, got: {}",
            muted.description
        );
    }

    #[test]
    fn command_palette_emits_actions_not_raw_insertions() {
        let entries = vec![CommandPaletteEntry {
            section: PaletteSection::Command,
            label: "/config".to_string(),
            description: "open config".to_string(),
            command: "/config".to_string(),
            action: CommandPaletteAction::ExecuteCommand {
                command: "/config".to_string(),
            },
        }];
        let mut view = CommandPaletteView::new(entries);

        let action = view.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()));
        assert!(matches!(
            action,
            ViewAction::EmitAndClose(ViewEvent::CommandPaletteSelected {
                action: CommandPaletteAction::ExecuteCommand { .. }
            })
        ));
    }

    #[test]
    fn command_palette_render_uses_injected_theme_colors() {
        let entries = vec![palette_entry(
            PaletteSection::Command,
            "/config",
            "open config",
            "/config",
        )];
        let mut theme = palette::UI_THEME;
        theme.border = ratatui::style::Color::Indexed(45);
        theme.selection_bg = ratatui::style::Color::Indexed(24);
        theme.selection_text = ratatui::style::Color::Indexed(231);
        let view = CommandPaletteView::new(entries).with_ui_theme(theme);
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));

        view.render(Rect::new(0, 0, 80, 24), &mut buf);

        assert_eq!(buf[(2, 2)].fg, theme.border);
        let has_selection_cell = buf
            .content()
            .iter()
            .any(|cell| cell.bg == theme.selection_bg && cell.fg == theme.selection_text);
        assert!(has_selection_cell, "selected row should use injected theme");
    }

    #[test]
    fn command_palette_cjk_rows_fit_80_columns() {
        let entries = vec![
            palette_entry(
                PaletteSection::Command,
                "/非常长的命令名称用于测试宽字符列对齐",
                "这是一段非常长的中文描述，用来确认命令面板不会因为宽字符补空格而超过八十列",
                "/wide",
            ),
            palette_entry(
                PaletteSection::Skill,
                "skill:中文宽字符技能名称",
                "另一个中文描述，覆盖不同 section 的渲染路径",
                "/skill cjk",
            ),
        ];
        let view = CommandPaletteView::new(entries).with_ui_theme(palette::DEEPSEEK_SHELL_UI_THEME);
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
                "command palette row overflowed 80 columns: {row:?}"
            );
        }
    }

    #[test]
    fn ascii_command_palette_chrome_uses_plain_border_chars() {
        let area = Rect::new(1, 1, 24, 8);
        let mut buf = Buffer::empty(Rect::new(0, 0, 28, 12));
        let inner = render_ascii_command_palette_chrome(
            area,
            &mut buf,
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
        let prefix = ascii_prefix(" 命令面板 ", 8);

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
