//! Sub-agent and background-task routing helpers for the TUI loop.

use std::time::Instant;

use crate::task_manager::{TaskRecord, TaskStatus, TaskSummary, TaskToolStatus};
use crate::tools::subagent::{MailboxMessage, SubAgentResult, SubAgentStatus};
use crate::tui::app::{App, AppMode, TaskPanelEntry};
use crate::tui::history::{HistoryCell, SubAgentCell, summarize_tool_output};
use crate::tui::pager::PagerView;
use crate::tui::widgets::agent_card::{
    AgentLifecycle, DelegateCard, FanoutCard, apply_to_delegate, apply_to_fanout,
};

pub(super) fn running_agent_count(app: &App) -> usize {
    let mut ids: std::collections::HashSet<&str> =
        app.agent_progress.keys().map(String::as_str).collect();
    for agent in app
        .subagent_cache
        .iter()
        .filter(|agent| matches!(agent.status, SubAgentStatus::Running))
    {
        ids.insert(agent.agent_id.as_str());
    }
    ids.len()
}

pub(super) fn active_fanout_counts(app: &App) -> Option<(usize, usize)> {
    // Read running count from the canonical slot states on the active
    // FanoutCard, if one exists. Used by `rlm` and any future multi-child
    // dispatch the parent agent makes via repeated `agent_spawn`.
    if let Some(idx) = app.last_fanout_card_index
        && let Some(HistoryCell::SubAgent(SubAgentCell::Fanout(card))) = app.history.get(idx)
    {
        let running = card
            .workers
            .iter()
            .filter(|slot| matches!(slot.status, AgentLifecycle::Running))
            .count();
        return Some((running, card.worker_count()));
    }
    None
}

pub(super) fn reconcile_subagent_activity_state(app: &mut App) {
    let running_agents: Vec<(String, String)> = app
        .subagent_cache
        .iter()
        .filter(|agent| matches!(agent.status, SubAgentStatus::Running))
        .map(|agent| {
            (
                agent.agent_id.clone(),
                summarize_tool_output(&agent.assignment.objective),
            )
        })
        .collect();

    let running_ids: std::collections::HashSet<String> =
        running_agents.iter().map(|(id, _)| id.clone()).collect();
    app.agent_progress
        .retain(|id, _| running_ids.contains(id.as_str()));
    for (id, objective) in running_agents {
        app.agent_progress.entry(id).or_insert(objective);
    }

    if running_ids.is_empty() {
        app.agent_activity_started_at = None;
    } else if app.agent_activity_started_at.is_none() {
        app.agent_activity_started_at = Some(Instant::now());
    }
}

fn subagent_status_rank(status: &SubAgentStatus) -> u8 {
    match status {
        SubAgentStatus::Running => 0,
        SubAgentStatus::Interrupted(_) => 1,
        SubAgentStatus::Failed(_) => 2,
        SubAgentStatus::Completed => 3,
        SubAgentStatus::Cancelled => 4,
    }
}

pub(super) fn sort_subagents_in_place(agents: &mut [SubAgentResult]) {
    agents.sort_by(|a, b| {
        subagent_status_rank(&a.status)
            .cmp(&subagent_status_rank(&b.status))
            .then_with(|| a.agent_type.as_str().cmp(b.agent_type.as_str()))
            .then_with(|| a.agent_id.cmp(&b.agent_id))
    });
}

/// Route a `MailboxMessage` envelope to the matching in-transcript card,
/// allocating a `DelegateCard` or `FanoutCard` on first sight (issue #128).
pub(super) fn handle_subagent_mailbox(app: &mut App, seq: u64, message: &MailboxMessage) {
    // Accumulate sub-agent token costs for the real-time footer counter (#166).
    if let MailboxMessage::TokenUsage { model, usage, .. } = message {
        if app.session.subagent_cost_event_seqs.insert(seq)
            && let Some(cost) =
                crate::pricing::calculate_turn_cost_estimate_from_usage(model, usage)
        {
            app.accrue_subagent_cost_estimate(cost);
        }
        return; // No card visual change needed; the footer handles display.
    }

    // Resolve (or allocate) the target cell for this envelope. ChildSpawned
    // is special — it always belongs to the active fanout card if one
    // exists; otherwise it seeds a new one.
    let agent_id = message.agent_id().to_string();

    if matches!(message, MailboxMessage::ChildSpawned { .. })
        && let Some(idx) = app.last_fanout_card_index
        && let Some(HistoryCell::SubAgent(SubAgentCell::Fanout(card))) = app.history.get_mut(idx)
    {
        apply_to_fanout(card, message);
        app.subagent_card_index.insert(agent_id, idx);
        app.mark_history_updated();
        return;
    }

    // Existing card for this agent_id? Mutate in place.
    if let Some(&idx) = app.subagent_card_index.get(&agent_id) {
        let updated = match app.history.get_mut(idx) {
            Some(HistoryCell::SubAgent(SubAgentCell::Delegate(card))) => {
                apply_to_delegate(card, message)
            }
            Some(HistoryCell::SubAgent(SubAgentCell::Fanout(card))) => {
                apply_to_fanout(card, message)
            }
            _ => false,
        };
        if updated {
            app.mark_history_updated();
        }
        return;
    }

    // No existing card — only `Started` reasonably opens one. Anything else
    // for an unknown agent_id is dropped (likely arrived after the cell was
    // cleared, e.g. session-resume edge cases).
    let MailboxMessage::Started { agent_type, .. } = message else {
        return;
    };

    let dispatch_kind = app.pending_subagent_dispatch.as_deref();
    let is_fanout = matches!(dispatch_kind, Some("rlm_open" | "rlm_eval" | "rlm"));

    if is_fanout {
        // Reuse the active fanout card for sibling spawns; otherwise create
        // one anchored at this position so subsequent siblings join it.
        if let Some(idx) = app.last_fanout_card_index
            && let Some(HistoryCell::SubAgent(SubAgentCell::Fanout(card))) =
                app.history.get_mut(idx)
        {
            card.claim_pending_worker(&agent_id, AgentLifecycle::Running);
            app.subagent_card_index.insert(agent_id, idx);
        } else {
            let mut card = FanoutCard::new(
                dispatch_kind.unwrap_or("rlm_eval").to_string(),
                app.ui_locale,
            );
            card.upsert_worker(&agent_id, AgentLifecycle::Running);
            app.add_message(HistoryCell::SubAgent(SubAgentCell::Fanout(card)));
            let idx = app.history.len().saturating_sub(1);
            app.last_fanout_card_index = Some(idx);
            app.subagent_card_index.insert(agent_id, idx);
        }
    } else {
        let card = DelegateCard::new(agent_id.clone(), agent_type.clone());
        app.add_message(HistoryCell::SubAgent(SubAgentCell::Delegate(card)));
        let idx = app.history.len().saturating_sub(1);
        app.subagent_card_index.insert(agent_id, idx);
        // Single delegate consumes the pending dispatch label so a follow-on
        // tool call doesn't accidentally inherit it.
        app.pending_subagent_dispatch = None;
    }

    app.mark_history_updated();
}

pub(super) fn task_mode_label(mode: AppMode) -> &'static str {
    mode.as_setting()
}

pub(super) fn task_summary_to_panel_entry(summary: TaskSummary) -> TaskPanelEntry {
    TaskPanelEntry {
        id: summary.id,
        status: task_status_label(summary.status).to_string(),
        prompt_summary: summary.prompt_summary,
        duration_ms: summary.duration_ms,
    }
}

fn task_status_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Queued => "queued",
        TaskStatus::Running => "running",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
        TaskStatus::Canceled => "canceled",
    }
}

fn task_status_marker(status: TaskStatus) -> &'static str {
    task_status_marker_for_ascii(status, crate::palette::ascii_ui_enabled())
}

fn task_status_marker_for_ascii(status: TaskStatus, ascii: bool) -> &'static str {
    if ascii {
        return match status {
            TaskStatus::Queued => "o",
            TaskStatus::Running => ">",
            TaskStatus::Completed => "+",
            TaskStatus::Failed => "x",
            TaskStatus::Canceled => "-",
        };
    }
    match status {
        TaskStatus::Queued => "\u{25CB}",
        TaskStatus::Running => "\u{25D0}",
        TaskStatus::Completed => "\u{2713}",
        TaskStatus::Failed => "\u{2717}",
        TaskStatus::Canceled => "\u{2298}",
    }
}

fn task_status_text(status: TaskStatus) -> String {
    format!("{} {}", task_status_marker(status), task_status_label(status))
}

fn task_tool_status_label(status: TaskToolStatus) -> &'static str {
    match status {
        TaskToolStatus::Running => "running",
        TaskToolStatus::Success => "success",
        TaskToolStatus::Failed => "failed",
        TaskToolStatus::Canceled => "canceled",
    }
}

fn task_tool_status_marker_for_ascii(status: TaskToolStatus, ascii: bool) -> &'static str {
    if ascii {
        return match status {
            TaskToolStatus::Running => ">",
            TaskToolStatus::Success => "+",
            TaskToolStatus::Failed => "x",
            TaskToolStatus::Canceled => "-",
        };
    }
    match status {
        TaskToolStatus::Running => "\u{25D0}",
        TaskToolStatus::Success => "\u{2713}",
        TaskToolStatus::Failed => "\u{2717}",
        TaskToolStatus::Canceled => "\u{2298}",
    }
}

fn task_tool_status_marker(status: TaskToolStatus) -> &'static str {
    task_tool_status_marker_for_ascii(status, crate::palette::ascii_ui_enabled())
}

fn task_tool_status_text(status: TaskToolStatus) -> String {
    format!(
        "{} {}",
        task_tool_status_marker(status),
        task_tool_status_label(status)
    )
}

pub(super) fn format_task_list(tasks: &[TaskSummary]) -> String {
    if tasks.is_empty() {
        return "No tasks found.".to_string();
    }

    let mut lines = vec![
        format!("Tasks ({})", tasks.len()),
        "ID             State          Time  Title".to_string(),
        "------------------------------------------------------------".to_string(),
    ];
    for task in tasks {
        let duration = task
            .duration_ms
            .map(|ms| format!("{:.2}s", ms as f64 / 1000.0))
            .unwrap_or_else(|| "-".to_string());
        lines.push(format!(
            "{:<13}  {:<11}  {:>8}  {}",
            task.id,
            task_status_text(task.status),
            duration,
            task.prompt_summary
        ));
    }
    lines.push("Use /task show <id> for timeline details.".to_string());
    lines.join("\n")
}

pub(super) fn open_task_pager(app: &mut App, task: &TaskRecord) {
    let width = app
        .viewport
        .last_transcript_area
        .map(|area| area.width)
        .unwrap_or(100)
        .saturating_sub(4);
    app.view_stack.push(
        PagerView::from_text(
            format!("Task {}", task.id),
            &format_task_detail(task),
            width.max(60),
        )
        .with_ui_theme(app.ui_theme),
    );
}

fn format_task_detail(task: &TaskRecord) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Task: {}", task.id));
    lines.push(format!("Status: {}", task_status_text(task.status)));
    lines.push(format!("Mode: {}", task.mode));
    lines.push(format!("Model: {}", task.model));
    lines.push(format!(
        "Workspace: {}",
        crate::utils::display_path(&task.workspace)
    ));
    if let Some(thread_id) = task.thread_id.as_ref() {
        lines.push(format!("Runtime Thread: {thread_id}"));
    }
    if let Some(turn_id) = task.turn_id.as_ref() {
        lines.push(format!("Runtime Turn: {turn_id}"));
    }
    if task.runtime_event_count > 0 {
        lines.push(format!("Runtime Events: {}", task.runtime_event_count));
    }
    lines.push(format!("Created: {}", task.created_at));
    if let Some(started_at) = task.started_at {
        lines.push(format!("Started: {started_at}"));
    }
    if let Some(ended_at) = task.ended_at {
        lines.push(format!("Ended: {ended_at}"));
    }
    if let Some(duration) = task.duration_ms {
        lines.push(format!("Duration: {:.2}s", duration as f64 / 1000.0));
    }
    lines.push(String::new());
    lines.push("Prompt:".to_string());
    lines.push(task.prompt.clone());

    if let Some(summary) = task.result_summary.as_ref() {
        lines.push(String::new());
        lines.push("Result Summary:".to_string());
        lines.push(summary.clone());
    }
    if let Some(path) = task.result_detail_path.as_ref() {
        lines.push(format!("Result Artifact: {}", path.display()));
    }
    if let Some(error) = task.error.as_ref() {
        lines.push(String::new());
        lines.push(format!("Error: {error}"));
    }

    lines.push(String::new());
    lines.push("Tool Calls:".to_string());
    if task.tool_calls.is_empty() {
        lines.push("- (none)".to_string());
    } else {
        for tool in &task.tool_calls {
            let mut line = format!(
                "- {} [{}] {}",
                tool.name,
                task_tool_status_text(tool.status),
                tool.output_summary.as_deref().unwrap_or("(no summary)")
            );
            if let Some(duration) = tool.duration_ms {
                line.push_str(&format!(" ({:.2}s)", duration as f64 / 1000.0));
            }
            lines.push(line);
            if let Some(path) = tool.detail_path.as_ref() {
                lines.push(format!("  detail: {}", path.display()));
            }
            if let Some(path) = tool.patch_ref.as_ref() {
                lines.push(format!("  patch: {}", path.display()));
            }
        }
    }

    lines.push(String::new());
    lines.push("Timeline:".to_string());
    if task.timeline.is_empty() {
        lines.push("- (none)".to_string());
    } else {
        for entry in &task.timeline {
            lines.push(format!(
                "- [{}] {}: {}",
                entry.timestamp, entry.kind, entry.summary
            ));
            if let Some(path) = entry.detail_path.as_ref() {
                lines.push(format!("  detail: {}", path.display()));
            }
        }
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_manager::{
        TaskChecklistState, TaskRecord, TaskStatus, TaskSummary, TaskToolCallSummary,
        TaskToolStatus,
    };
    use crate::test_support::EnvVarGuard;
    use chrono::Utc;
    use std::path::PathBuf;

    fn task_summary(id: &str, status: TaskStatus, duration_ms: Option<u64>) -> TaskSummary {
        TaskSummary {
            id: id.to_string(),
            status,
            prompt_summary: "Fix task list output".to_string(),
            model: "deepseek-v4-pro".to_string(),
            mode: "agent".to_string(),
            created_at: Utc::now(),
            started_at: None,
            ended_at: None,
            duration_ms,
            error: None,
            thread_id: None,
            turn_id: None,
        }
    }

    #[test]
    fn task_list_includes_title_header_and_time_column() {
        let _ascii = EnvVarGuard::set("CODEWHALE_ASCII_UI", "1");
        let output = format_task_list(&[
            task_summary("task_12345678", TaskStatus::Running, None),
            task_summary("task_abcdef12", TaskStatus::Completed, Some(1234)),
        ]);

        assert!(output.contains("ID             State          Time  Title"));
        assert!(output.contains("task_12345678  > running"));
        assert!(output.contains("task_abcdef12  + completed"));
        assert!(output.contains("1.23s  Fix task list output"));
    }

    #[test]
    fn task_status_markers_have_ascii_fallbacks() {
        assert_eq!(task_status_marker_for_ascii(TaskStatus::Queued, true), "o");
        assert_eq!(task_status_marker_for_ascii(TaskStatus::Running, true), ">");
        assert_eq!(task_status_marker_for_ascii(TaskStatus::Completed, true), "+");
        assert_eq!(task_status_marker_for_ascii(TaskStatus::Failed, true), "x");
        assert_eq!(task_status_marker_for_ascii(TaskStatus::Canceled, true), "-");

        assert_eq!(task_status_marker_for_ascii(TaskStatus::Queued, false), "\u{25CB}");
        assert_eq!(
            task_status_marker_for_ascii(TaskStatus::Running, false),
            "\u{25D0}"
        );
        assert_eq!(
            task_status_marker_for_ascii(TaskStatus::Completed, false),
            "\u{2713}"
        );
        assert_eq!(
            task_status_marker_for_ascii(TaskStatus::Failed, false),
            "\u{2717}"
        );
        assert_eq!(
            task_status_marker_for_ascii(TaskStatus::Canceled, false),
            "\u{2298}"
        );
    }

    #[test]
    fn task_tool_status_markers_have_ascii_fallbacks() {
        assert_eq!(
            task_tool_status_marker_for_ascii(TaskToolStatus::Running, true),
            ">"
        );
        assert_eq!(
            task_tool_status_marker_for_ascii(TaskToolStatus::Success, true),
            "+"
        );
        assert_eq!(
            task_tool_status_marker_for_ascii(TaskToolStatus::Failed, true),
            "x"
        );
        assert_eq!(
            task_tool_status_marker_for_ascii(TaskToolStatus::Canceled, true),
            "-"
        );

        assert_eq!(
            task_tool_status_marker_for_ascii(TaskToolStatus::Running, false),
            "\u{25D0}"
        );
        assert_eq!(
            task_tool_status_marker_for_ascii(TaskToolStatus::Success, false),
            "\u{2713}"
        );
        assert_eq!(
            task_tool_status_marker_for_ascii(TaskToolStatus::Failed, false),
            "\u{2717}"
        );
        assert_eq!(
            task_tool_status_marker_for_ascii(TaskToolStatus::Canceled, false),
            "\u{2298}"
        );
    }

    #[test]
    fn task_detail_marks_task_and_tool_status_in_ascii_mode() {
        let _ascii = EnvVarGuard::set("CODEWHALE_ASCII_UI", "1");
        let now = Utc::now();
        let task = TaskRecord {
            schema_version: 1,
            id: "task_12345678".to_string(),
            prompt: "Fix task detail output".to_string(),
            model: "deepseek-v4-pro".to_string(),
            workspace: PathBuf::from("."),
            mode: "agent".to_string(),
            allow_shell: true,
            trust_mode: false,
            auto_approve: false,
            status: TaskStatus::Failed,
            created_at: now,
            started_at: Some(now),
            ended_at: Some(now),
            duration_ms: Some(2500),
            result_summary: None,
            result_detail_path: None,
            error: Some("command failed".to_string()),
            thread_id: None,
            turn_id: None,
            runtime_event_count: 0,
            checklist: TaskChecklistState::default(),
            gates: Vec::new(),
            attempts: Vec::new(),
            artifacts: Vec::new(),
            github_events: Vec::new(),
            tool_calls: vec![TaskToolCallSummary {
                id: "tool_1".to_string(),
                name: "run_tests".to_string(),
                status: TaskToolStatus::Failed,
                started_at: now,
                ended_at: Some(now),
                duration_ms: Some(500),
                input_summary: None,
                output_summary: Some("cargo test failed".to_string()),
                detail_path: None,
                patch_ref: None,
            }],
            timeline: Vec::new(),
        };

        let output = format_task_detail(&task);
        assert!(output.contains("Status: x failed"));
        assert!(output.contains("- run_tests [x failed] cargo test failed"));
    }
}
