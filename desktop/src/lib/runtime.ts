import { invoke } from "@tauri-apps/api/core";
import type { Language } from "../i18n";

export type ThemeMode = "system" | "light" | "dark";

export type DesktopConfig = {
  provider: string;
  base_url: string;
  api_key_present: boolean;
  model: string;
  runtime_host: string;
  runtime_port: number;
  runtime_token: string;
  runtime_command: string;
  language: Language;
  theme: ThemeMode;
  env_path: string;
};

export type SaveDesktopConfigRequest = {
  provider: "deepseek" | "openai-compatible";
  base_url: string;
  api_key?: string;
  model: string;
  runtime_host: string;
  runtime_port: number;
  runtime_command: string;
  language: Language;
  theme: ThemeMode;
};

export type RuntimeLaunchResult = {
  attached_existing: boolean;
  pid: number | null;
  message: string;
};

export type DoctorResult = {
  success: boolean;
  exit_code: number | null;
  stdout: string;
  stderr: string;
};

export type RuntimeProbe<T = unknown> = {
  ok: boolean;
  status?: number;
  data?: T;
  error?: string;
};

export type ThreadRecord = {
  id: string;
  title?: string | null;
  created_at?: string;
  updated_at?: string;
  model?: string;
  workspace?: string;
  mode?: string;
  latest_turn_id?: string | null;
  archived?: boolean;
};

export type TurnRecord = {
  id: string;
  thread_id: string;
  status?: string;
  created_at?: string;
  updated_at?: string;
};

export type TurnItemRecord = {
  id: string;
  turn_id: string;
  kind: string;
  status?: string;
  created_at?: string;
  updated_at?: string;
  content?: unknown;
  metadata?: Record<string, unknown>;
};

export type ThreadDetail = {
  thread: ThreadRecord;
  turns: TurnRecord[];
  items: TurnItemRecord[];
  latest_seq: number;
};

export type StartTurnResponse = {
  thread: ThreadRecord;
  turn: TurnRecord;
};

export type SessionMetadata = {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  message_count: number;
  total_tokens: number;
  model: string;
  workspace: string;
  mode?: string | null;
};

export type SessionsResponse = {
  sessions: SessionMetadata[];
};

export type ResumeSessionResponse = {
  thread_id: string;
  session_id: string;
  message_count: number;
  summary: string;
};

export type RuntimeEvent = {
  schema_version?: number;
  seq: number;
  event: string;
  kind?: string;
  thread_id?: string;
  turn_id?: string;
  item_id?: string;
  timestamp?: string;
  created_at?: string;
  payload?: Record<string, unknown>;
};

export type WorkspaceStatus = {
  workspace: string;
  git_repo: boolean;
  branch?: string | null;
  staged: number;
  unstaged: number;
  untracked: number;
  ahead?: number | null;
  behind?: number | null;
};

export type DecideApprovalResponse = {
  ok: boolean;
  approval_id: string;
  decision: "allow" | "deny";
  delivered: boolean;
};

function runtimeBaseUrl(config: DesktopConfig): string {
  return `http://${config.runtime_host}:${config.runtime_port}`;
}

async function fetchJson<T>(
  url: string,
  init: RequestInit = {},
  timeoutMs = 2500,
): Promise<RuntimeProbe<T>> {
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(), timeoutMs);

  try {
    const response = await fetch(url, { ...init, signal: controller.signal });
    const text = await response.text();
    const data = text ? (JSON.parse(text) as T) : ({} as T);

    return {
      ok: response.ok,
      status: response.status,
      data,
      error: response.ok ? undefined : response.statusText,
    };
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    };
  } finally {
    window.clearTimeout(timeout);
  }
}

export function loadDesktopConfig(): Promise<DesktopConfig> {
  return invoke("load_desktop_config");
}

export function saveDesktopConfig(req: SaveDesktopConfigRequest): Promise<DesktopConfig> {
  return invoke("save_desktop_config", { req });
}

export function selectProjectDirectory(): Promise<string | null> {
  return invoke("select_project_directory");
}

export function startRuntime(): Promise<RuntimeLaunchResult> {
  return invoke("start_runtime");
}

export function stopRuntime(): Promise<void> {
  return invoke("stop_runtime");
}

export function runDoctor(): Promise<DoctorResult> {
  return invoke("run_doctor");
}

export function getHealth(config: DesktopConfig): Promise<RuntimeProbe> {
  return fetchJson(`${runtimeBaseUrl(config)}/health`);
}

export function getRuntimeInfo(config: DesktopConfig): Promise<RuntimeProbe> {
  return fetchJson(`${runtimeBaseUrl(config)}/v1/runtime/info`, {
    headers: {
      Authorization: `Bearer ${config.runtime_token}`,
    },
  });
}

export function getWorkspaceStatus(config: DesktopConfig): Promise<RuntimeProbe<WorkspaceStatus>> {
  return fetchJson(`${runtimeBaseUrl(config)}/v1/workspace/status`, {
    headers: authHeaders(config),
  });
}

export function decideApproval(
  config: DesktopConfig,
  approvalId: string,
  decision: "allow" | "deny",
  remember = false,
): Promise<RuntimeProbe<DecideApprovalResponse>> {
  return fetchJson(
    `${runtimeBaseUrl(config)}/v1/approvals/${encodeURIComponent(approvalId)}`,
    {
      method: "POST",
      headers: authHeaders(config),
      body: JSON.stringify({ decision, remember }),
    },
    5000,
  );
}

function authHeaders(config: DesktopConfig): HeadersInit {
  return {
    Authorization: `Bearer ${config.runtime_token}`,
    "Content-Type": "application/json",
  };
}

export function listThreads(config: DesktopConfig): Promise<RuntimeProbe<ThreadRecord[]>> {
  return fetchJson(`${runtimeBaseUrl(config)}/v1/threads?limit=50&include_archived=false`, {
    headers: authHeaders(config),
  });
}

export function getThread(config: DesktopConfig, threadId: string): Promise<RuntimeProbe<ThreadDetail>> {
  return fetchJson(`${runtimeBaseUrl(config)}/v1/threads/${encodeURIComponent(threadId)}`, {
    headers: authHeaders(config),
  });
}

export function createThread(
  config: DesktopConfig,
  workspace: string,
): Promise<RuntimeProbe<ThreadRecord>> {
  return fetchJson(
    `${runtimeBaseUrl(config)}/v1/threads`,
    {
      method: "POST",
      headers: authHeaders(config),
      body: JSON.stringify({
        workspace,
        model: config.model,
        mode: "agent",
        trust_mode: true,
        auto_approve: false,
      }),
    },
    5000,
  );
}

export function resumeThread(config: DesktopConfig, threadId: string): Promise<RuntimeProbe<ThreadRecord>> {
  return fetchJson(
    `${runtimeBaseUrl(config)}/v1/threads/${encodeURIComponent(threadId)}/resume`,
    {
      method: "POST",
      headers: authHeaders(config),
      body: "{}",
    },
    5000,
  );
}

export function listSessions(config: DesktopConfig): Promise<RuntimeProbe<SessionsResponse>> {
  return fetchJson(`${runtimeBaseUrl(config)}/v1/sessions?limit=20`, {
    headers: authHeaders(config),
  });
}

export function resumeSessionThread(
  config: DesktopConfig,
  sessionId: string,
): Promise<RuntimeProbe<ResumeSessionResponse>> {
  return fetchJson(
    `${runtimeBaseUrl(config)}/v1/sessions/${encodeURIComponent(sessionId)}/resume-thread`,
    {
      method: "POST",
      headers: authHeaders(config),
      body: JSON.stringify({
        model: config.model,
        mode: "agent",
      }),
    },
    5000,
  );
}

export function sendThreadTurn(
  config: DesktopConfig,
  threadId: string,
  prompt: string,
): Promise<RuntimeProbe<StartTurnResponse>> {
  return fetchJson(
    `${runtimeBaseUrl(config)}/v1/threads/${encodeURIComponent(threadId)}/turns`,
    {
      method: "POST",
      headers: authHeaders(config),
      body: JSON.stringify({
        prompt,
        model: config.model,
        mode: "agent",
        trust_mode: true,
        auto_approve: false,
      }),
    },
    5000,
  );
}

export function interruptTurn(
  config: DesktopConfig,
  threadId: string,
  turnId: string,
): Promise<RuntimeProbe<StartTurnResponse>> {
  return fetchJson(
    `${runtimeBaseUrl(config)}/v1/threads/${encodeURIComponent(threadId)}/turns/${encodeURIComponent(turnId)}/interrupt`,
    {
      method: "POST",
      headers: authHeaders(config),
      body: "{}",
    },
    5000,
  );
}

export function threadEventsUrl(config: DesktopConfig, threadId: string, sinceSeq = 0): string {
  const url = new URL(`${runtimeBaseUrl(config)}/v1/threads/${encodeURIComponent(threadId)}/events`);
  url.searchParams.set("since_seq", String(sinceSeq));
  url.searchParams.set("token", config.runtime_token);
  return url.toString();
}
