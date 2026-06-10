import {
  Activity,
  CheckCircle2,
  FolderOpen,
  ListRestart,
  MessageSquare,
  Play,
  Plus,
  RefreshCw,
  Send,
  Settings,
  ShieldCheck,
  Square,
  TerminalSquare,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { dictionaries, normalizeLanguage, type Language } from "./i18n";
import {
  createThread,
  getThread,
  getHealth,
  getRuntimeInfo,
  interruptTurn,
  listSessions,
  listThreads,
  loadDesktopConfig,
  resumeSessionThread,
  resumeThread,
  runDoctor,
  saveDesktopConfig,
  selectProjectDirectory,
  sendThreadTurn,
  startRuntime,
  stopRuntime,
  threadEventsUrl,
  type DesktopConfig,
  decideApproval,
  type DoctorResult,
  getWorkspaceStatus,
  type RuntimeEvent,
  type RuntimeProbe,
  type SaveDesktopConfigRequest,
  type SessionMetadata,
  type ThemeMode,
  type ThreadDetail,
  type ThreadRecord,
  type TurnItemRecord,
  type WorkspaceStatus,
} from "./lib/runtime";

type RuntimeState = "healthy" | "offline" | "starting" | "unknown";
type SettingsForm = SaveDesktopConfigRequest & { api_key: string };

const RECENT_PROJECTS_KEY = "codewhale.desktop.recentProjects";
const TRUSTED_PROJECTS_KEY = "codewhale.desktop.trustedProjects";
const LAST_PROJECT_KEY = "codewhale.desktop.lastProject";
const LANGUAGE_KEY = "codewhale.desktop.language";
const THEME_KEY = "codewhale.desktop.theme";

const EVENT_NAMES = [
  "thread.started",
  "thread.forked",
  "turn.started",
  "turn.lifecycle",
  "turn.steered",
  "turn.interrupt_requested",
  "turn.completed",
  "item.started",
  "item.delta",
  "item.completed",
  "item.failed",
  "item.interrupted",
  "approval.required",
  "approval.decided",
  "approval.timeout",
  "sandbox.denied",
  "coherence.state",
];

function readStoredList(key: string): string[] {
  try {
    const raw = window.localStorage.getItem(key);
    const parsed = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === "string") : [];
  } catch {
    return [];
  }
}

function writeStoredList(key: string, values: string[]) {
  window.localStorage.setItem(key, JSON.stringify(values));
}

function readStoredValue(key: string): string | null {
  return window.localStorage.getItem(key);
}

function writeStoredValue(key: string, value: string) {
  window.localStorage.setItem(key, value);
}

function rememberProject(project: string, projects: string[]): string[] {
  return [project, ...projects.filter((item) => item !== project)].slice(0, 8);
}

function formatJson(value: unknown): string {
  if (value === undefined || value === null) {
    return "";
  }

  return JSON.stringify(value, null, 2);
}

function formatThreadTitle(thread: ThreadRecord): string {
  return thread.title || thread.id;
}

function eventText(event: RuntimeEvent): string {
  const payload = event.payload ?? {};
  const delta = payload.delta;
  if (typeof delta === "string" && delta.trim()) return delta;

  const text = payload.text ?? payload.message ?? payload.summary;
  if (typeof text === "string" && text.trim()) return text;

  const content = payload.content;
  if (typeof content === "string" && content.trim()) return content;

  return event.event;
}

function payloadString(event: RuntimeEvent, key: string): string {
  const value = event.payload?.[key];
  return typeof value === "string" ? value : "";
}

function approvalId(event: RuntimeEvent): string {
  return payloadString(event, "approval_id") || payloadString(event, "id");
}

function statusLabel(state: RuntimeState, dictionary: typeof dictionaries["zh-CN"]): string {
  if (state === "healthy") return dictionary.runtimeHealthy;
  if (state === "offline") return dictionary.runtimeOffline;
  if (state === "starting") return dictionary.runtimeStarting;
  return dictionary.runtimeUnknown;
}

function applyTheme(theme: ThemeMode) {
  document.documentElement.dataset.theme = theme;
}

function mergeEvents(current: RuntimeEvent[], incoming: RuntimeEvent): RuntimeEvent[] {
  if (current.some((event) => event.seq === incoming.seq)) {
    return current;
  }

  return [...current, incoming].sort((a, b) => a.seq - b.seq).slice(-500);
}

function itemToEvent(item: TurnItemRecord, index: number, total: number, threadId: string): RuntimeEvent {
  return {
    seq: index - total,
    event: item.kind,
    kind: item.kind,
    thread_id: threadId,
    turn_id: item.turn_id,
    item_id: item.id,
    timestamp: item.created_at,
    created_at: item.created_at,
    payload: {
      content: item.content,
      metadata: item.metadata,
      status: item.status,
    },
  };
}

function configToSettings(config: DesktopConfig): SettingsForm {
  return {
    provider: config.provider === "openai-compatible" ? "openai-compatible" : "deepseek",
    base_url: config.base_url,
    api_key: "",
    model: config.model,
    runtime_host: config.runtime_host,
    runtime_port: config.runtime_port,
    runtime_command: config.runtime_command,
    language: normalizeLanguage(config.language),
    theme: config.theme,
  };
}

export function App() {
  const [config, setConfig] = useState<DesktopConfig | null>(null);
  const [language, setLanguage] = useState<Language>("zh-CN");
  const [theme, setTheme] = useState<ThemeMode>("system");
  const [runtimeState, setRuntimeState] = useState<RuntimeState>("unknown");
  const [health, setHealth] = useState<RuntimeProbe | null>(null);
  const [runtimeInfo, setRuntimeInfo] = useState<RuntimeProbe | null>(null);
  const [doctor, setDoctor] = useState<DoctorResult | null>(null);
  const [workspaceStatus, setWorkspaceStatus] = useState<WorkspaceStatus | null>(null);
  const [notice, setNotice] = useState("");
  const [updatedAt, setUpdatedAt] = useState<string | null>(null);
  const [projectInput, setProjectInput] = useState("");
  const [projectPath, setProjectPath] = useState("");
  const [recentProjects, setRecentProjects] = useState<string[]>([]);
  const [trustedProjects, setTrustedProjects] = useState<string[]>([]);
  const [threads, setThreads] = useState<ThreadRecord[]>([]);
  const [sessions, setSessions] = useState<SessionMetadata[]>([]);
  const [selectedThreadId, setSelectedThreadId] = useState("");
  const [eventsByThread, setEventsByThread] = useState<Record<string, RuntimeEvent[]>>({});
  const [threadDetails, setThreadDetails] = useState<Record<string, ThreadDetail>>({});
  const [composer, setComposer] = useState("");
  const [activeTurnId, setActiveTurnId] = useState<string | null>(null);
  const [settingsForm, setSettingsForm] = useState<SettingsForm | null>(null);

  const dictionary = useMemo(() => dictionaries[language], [language]);
  const selectedThread = useMemo(
    () => threads.find((thread) => thread.id === selectedThreadId) ?? null,
    [threads, selectedThreadId],
  );
  const selectedThreadDetail = selectedThreadId ? threadDetails[selectedThreadId] : undefined;
  const selectedEvents = selectedThreadId ? eventsByThread[selectedThreadId] ?? [] : [];
  const decidedApprovalIds = new Set(
    selectedEvents
      .filter((event) => event.event === "approval.decided")
      .map(approvalId)
      .filter(Boolean),
  );
  const pendingApprovals = selectedEvents.filter(
    (event) => event.event === "approval.required" && approvalId(event) && !decidedApprovalIds.has(approvalId(event)),
  );
  const projectTrusted = projectPath ? trustedProjects.includes(projectPath) : false;

  const refreshThreads = useCallback(
    async (nextConfig = config) => {
      if (!nextConfig) return;
      const response = await listThreads(nextConfig);
      if (response.ok && response.data) {
        setThreads(response.data);
        if (!selectedThreadId && response.data[0]) {
          setSelectedThreadId(response.data[0].id);
        }
      } else if (response.error) {
        setNotice(response.error);
      }
    },
    [config, selectedThreadId],
  );

  const refreshSessions = useCallback(
    async (nextConfig = config) => {
      if (!nextConfig) return;
      const response = await listSessions(nextConfig);
      if (response.ok && response.data) {
        setSessions(response.data.sessions);
      } else if (response.error) {
        setNotice(`${dictionary.loadSessionsFailed}: ${response.error}`);
      }
    },
    [config, dictionary.loadSessionsFailed],
  );

  const refresh = useCallback(
    async (nextConfig = config) => {
      if (!nextConfig) return;

      const [nextHealth, nextRuntimeInfo, nextDoctor] = await Promise.all([
        getHealth(nextConfig),
        getRuntimeInfo(nextConfig),
        runDoctor(),
      ]);

      setHealth(nextHealth);
      setRuntimeInfo(nextRuntimeInfo);
      setDoctor(nextDoctor);
      setRuntimeState(nextHealth.ok ? "healthy" : "offline");
      setUpdatedAt(new Date().toLocaleTimeString());

      if (nextHealth.ok) {
        await refreshThreads(nextConfig);
        await refreshSessions(nextConfig);
        const nextWorkspaceStatus = await getWorkspaceStatus(nextConfig);
        if (nextWorkspaceStatus.ok && nextWorkspaceStatus.data) {
          setWorkspaceStatus(nextWorkspaceStatus.data);
        }
      }
    },
    [config, refreshSessions, refreshThreads],
  );

  useEffect(() => {
    const storedRecentProjects = readStoredList(RECENT_PROJECTS_KEY);
    const storedTrustedProjects = readStoredList(TRUSTED_PROJECTS_KEY);
    const storedProject = readStoredValue(LAST_PROJECT_KEY) ?? storedRecentProjects[0] ?? "";

    setRecentProjects(storedRecentProjects);
    setTrustedProjects(storedTrustedProjects);
    setProjectPath(storedProject);
    setProjectInput(storedProject);

    let mounted = true;

    loadDesktopConfig().then((nextConfig) => {
      if (!mounted) return;
      setConfig(nextConfig);
      const nextLanguage = normalizeLanguage(readStoredValue(LANGUAGE_KEY) ?? nextConfig.language);
      const nextTheme = (readStoredValue(THEME_KEY) as ThemeMode | null) ?? nextConfig.theme;
      setLanguage(nextLanguage);
      setTheme(nextTheme);
      setSettingsForm({ ...configToSettings(nextConfig), language: nextLanguage, theme: nextTheme });
      applyTheme(nextTheme);
      void refresh(nextConfig);
    });

    return () => {
      mounted = false;
    };
  }, [refresh]);

  useEffect(() => {
    if (!config || !selectedThreadId) return;

    let active = true;
    getThread(config, selectedThreadId).then((response) => {
      if (!active) return;
      if (!response.ok || !response.data) {
        setNotice(`${dictionary.loadThreadFailed}: ${response.error ?? response.status ?? dictionary.unavailable}`);
        return;
      }

      const detail = response.data;
      setThreadDetails((current) => ({ ...current, [selectedThreadId]: detail }));
      setEventsByThread((current) => {
        const itemEvents = detail.items.map((item, index) =>
          itemToEvent(item, index, detail.items.length, detail.thread.id),
        );
        const merged = itemEvents.reduce(
          (events, event) => mergeEvents(events, event),
          current[selectedThreadId] ?? [],
        );
        return { ...current, [selectedThreadId]: merged };
      });
    });

    return () => {
      active = false;
    };
  }, [config, dictionary.loadThreadFailed, dictionary.unavailable, selectedThreadId]);

  useEffect(() => {
    const interval = window.setInterval(() => {
      void refresh();
    }, 5000);

    return () => window.clearInterval(interval);
  }, [refresh]);

  useEffect(() => {
    if (!config || !selectedThreadId) return;

    const source = new EventSource(threadEventsUrl(config, selectedThreadId, 0));

    const handleMessage = (message: MessageEvent<string>) => {
      try {
        const event = JSON.parse(message.data) as RuntimeEvent;
        setEventsByThread((current) => ({
          ...current,
          [selectedThreadId]: mergeEvents(current[selectedThreadId] ?? [], event),
        }));
      } catch {
        setNotice(message.data);
      }
    };

    source.onmessage = handleMessage;
    EVENT_NAMES.forEach((eventName) => source.addEventListener(eventName, handleMessage));
    source.onerror = () => source.close();

    return () => {
      EVENT_NAMES.forEach((eventName) => source.removeEventListener(eventName, handleMessage));
      source.close();
    };
  }, [config, selectedThreadId]);

  function handleLanguage(nextLanguage: Language) {
    setLanguage(nextLanguage);
    document.documentElement.lang = nextLanguage;
    writeStoredValue(LANGUAGE_KEY, nextLanguage);
    setSettingsForm((current) => (current ? { ...current, language: nextLanguage } : current));
  }

  function handleTheme(nextTheme: ThemeMode) {
    setTheme(nextTheme);
    applyTheme(nextTheme);
    writeStoredValue(THEME_KEY, nextTheme);
    setSettingsForm((current) => (current ? { ...current, theme: nextTheme } : current));
  }

  function updateSettings<K extends keyof SettingsForm>(key: K, value: SettingsForm[K]) {
    setSettingsForm((current) => (current ? { ...current, [key]: value } : current));
  }

  function handleOpenProject(path = projectInput.trim()) {
    if (!path) {
      setNotice(dictionary.projectRequired);
      return;
    }

    setProjectPath(path);
    setProjectInput(path);
    setNotice("");

    const nextProjects = rememberProject(path, recentProjects);
    setRecentProjects(nextProjects);
    writeStoredList(RECENT_PROJECTS_KEY, nextProjects);
    writeStoredValue(LAST_PROJECT_KEY, path);
  }

  async function handleSelectProjectDirectory() {
    try {
      const path = await selectProjectDirectory();
      if (!path) return;
      handleOpenProject(path);
    } catch (error) {
      setNotice(`${dictionary.selectDirectoryFailed}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  function handleTrustProject() {
    if (!projectPath) {
      setNotice(dictionary.projectRequired);
      return;
    }

    const nextTrusted = rememberProject(projectPath, trustedProjects);
    setTrustedProjects(nextTrusted);
    writeStoredList(TRUSTED_PROJECTS_KEY, nextTrusted);
  }

  async function handleStart() {
    setRuntimeState("starting");
    setNotice("");

    try {
      const result = await startRuntime();
      setNotice(result.attached_existing ? dictionary.connectedExisting : dictionary.startedRuntime);
      const nextConfig = await loadDesktopConfig();
      setConfig(nextConfig);
      await refresh(nextConfig);
    } catch (error) {
      setRuntimeState("offline");
      setNotice(`${dictionary.startFailed}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  async function handleStop() {
    setNotice("");

    try {
      await stopRuntime();
      setRuntimeState("offline");
      setNotice(dictionary.stoppedRuntime);
      await refresh();
    } catch (error) {
      setNotice(`${dictionary.stopFailed}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  async function handleSaveSettings() {
    if (!settingsForm) return;

    try {
      const nextConfig = await saveDesktopConfig({
        ...settingsForm,
        api_key: settingsForm.api_key.trim() || undefined,
      });
      setConfig(nextConfig);
      setSettingsForm({ ...configToSettings(nextConfig), api_key: "" });
      handleLanguage(normalizeLanguage(nextConfig.language));
      handleTheme(nextConfig.theme);
      setNotice(dictionary.settingsSaved);
      await refresh(nextConfig);
    } catch (error) {
      setNotice(`${dictionary.saveSettingsFailed}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  async function handleCreateThread() {
    if (!config || runtimeState !== "healthy") {
      setNotice(dictionary.runtimeRequired);
      return;
    }
    if (!projectPath) {
      setNotice(dictionary.projectRequired);
      return;
    }
    if (!projectTrusted) {
      setNotice(dictionary.trustRequired);
      return;
    }

    const response = await createThread(config, projectPath);
    if (!response.ok || !response.data) {
      setNotice(`${dictionary.createThreadFailed}: ${response.error ?? response.status ?? dictionary.unavailable}`);
      return;
    }

    setThreads((current) => [response.data!, ...current.filter((thread) => thread.id !== response.data!.id)]);
    setSelectedThreadId(response.data.id);
    setEventsByThread((current) => ({ ...current, [response.data!.id]: [] }));
    setNotice("");
  }

  async function handleResumeThread() {
    if (!config || runtimeState !== "healthy" || !selectedThreadId) {
      setNotice(dictionary.runtimeRequired);
      return;
    }

    const response = await resumeThread(config, selectedThreadId);
    if (!response.ok || !response.data) {
      setNotice(`${dictionary.resumeThreadFailed}: ${response.error ?? response.status ?? dictionary.unavailable}`);
      return;
    }

    setThreads((current) => [response.data!, ...current.filter((thread) => thread.id !== response.data!.id)]);
    setSelectedThreadId(response.data.id);
    setNotice("");
  }

  async function handleResumeSession(sessionId: string) {
    if (!config || runtimeState !== "healthy") {
      setNotice(dictionary.runtimeRequired);
      return;
    }

    const response = await resumeSessionThread(config, sessionId);
    if (!response.ok || !response.data) {
      setNotice(`${dictionary.resumeSessionFailed}: ${response.error ?? response.status ?? dictionary.unavailable}`);
      return;
    }

    await refreshThreads(config);
    setSelectedThreadId(response.data.thread_id);
    setNotice(response.data.summary);
  }

  async function handleSend() {
    const prompt = composer.trim();
    if (!config || runtimeState !== "healthy") {
      setNotice(dictionary.runtimeRequired);
      return;
    }
    if (!selectedThreadId) {
      setNotice(dictionary.selectThread);
      return;
    }
    if (!prompt) return;

    setComposer("");
    const response = await sendThreadTurn(config, selectedThreadId, prompt);
    if (!response.ok || !response.data) {
      setNotice(`${dictionary.sendFailed}: ${response.error ?? response.status ?? dictionary.unavailable}`);
      setComposer(prompt);
      return;
    }

    setActiveTurnId(response.data.turn.id);
    setEventsByThread((current) => ({
      ...current,
      [selectedThreadId]: mergeEvents(current[selectedThreadId] ?? [], {
        seq: Date.now(),
        event: "user_message.local",
        thread_id: selectedThreadId,
        turn_id: response.data!.turn.id,
        timestamp: new Date().toISOString(),
        payload: { text: prompt },
      }),
    }));
  }

  async function handleInterrupt() {
    if (!config || !selectedThreadId || !activeTurnId) return;

    const response = await interruptTurn(config, selectedThreadId, activeTurnId);
    if (!response.ok) {
      setNotice(`${dictionary.interruptFailed}: ${response.error ?? response.status ?? dictionary.unavailable}`);
      return;
    }
    setActiveTurnId(null);
  }

  async function handleApprovalDecision(event: RuntimeEvent, decision: "allow" | "deny", remember = false) {
    if (!config) return;

    const id = approvalId(event);
    if (!id) return;

    const response = await decideApproval(config, id, decision, remember);
    if (!response.ok) {
      setNotice(`${dictionary.approvalFailed}: ${response.error ?? response.status ?? dictionary.unavailable}`);
      return;
    }

    setEventsByThread((current) => ({
      ...current,
      [selectedThreadId]: mergeEvents(current[selectedThreadId] ?? [], {
        seq: Date.now(),
        event: "approval.decided",
        thread_id: selectedThreadId,
        turn_id: event.turn_id,
        timestamp: new Date().toISOString(),
        payload: { approval_id: id, decision, remember },
      }),
    }));
  }

  const runtimeStatus = statusLabel(runtimeState, dictionary);

  return (
    <div className="app-shell">
      <header className="topbar">
        <div className="brand">
          <div className="brand-mark">CW</div>
          <div>
            <h1>{dictionary.appName}</h1>
            <span>{projectPath || dictionary.workspace}</span>
          </div>
        </div>

        <div className={`status-pill status-${runtimeState}`}>
          <CheckCircle2 aria-hidden="true" size={18} />
          <span>{runtimeStatus}</span>
        </div>

        <div className="topbar-actions">
          <button className="icon-button" type="button" onClick={() => void refresh()} title={dictionary.refresh}>
            <RefreshCw aria-hidden="true" size={19} />
          </button>
          <button className="primary-button" type="button" onClick={() => void handleStart()}>
            <Play aria-hidden="true" size={18} />
            <span>{dictionary.startRuntime}</span>
          </button>
          <button className="secondary-button" type="button" onClick={() => void handleStop()}>
            <Square aria-hidden="true" size={16} />
            <span>{dictionary.stopRuntime}</span>
          </button>
        </div>
      </header>

      <main className="workbench">
        <aside className="sidebar">
          <section className="panel project-panel">
            <div className="panel-title">
              <FolderOpen aria-hidden="true" size={18} />
              <h2>{dictionary.project}</h2>
            </div>

            <label className="text-label">
              <span>{dictionary.projectPath}</span>
              <input
                value={projectInput}
                onChange={(event) => setProjectInput(event.target.value)}
                placeholder={dictionary.projectPathPlaceholder}
              />
            </label>
            <div className="project-actions">
              <button className="primary-button" type="button" onClick={() => handleOpenProject()}>
                <FolderOpen aria-hidden="true" size={18} />
                <span>{dictionary.openProject}</span>
              </button>
              <button className="secondary-button" type="button" onClick={() => void handleSelectProjectDirectory()}>
                <FolderOpen aria-hidden="true" size={18} />
                <span>{dictionary.selectDirectory}</span>
              </button>
              <button className="secondary-button" type="button" onClick={handleTrustProject}>
                <ShieldCheck aria-hidden="true" size={18} />
                <span>{dictionary.trustProject}</span>
              </button>
            </div>

            <div className={`trust-state ${projectTrusted ? "trusted" : "untrusted"}`}>
              <ShieldCheck aria-hidden="true" size={16} />
              <span>{projectTrusted ? dictionary.trustedProject : dictionary.untrustedProject}</span>
            </div>
            <p className="help-text">{dictionary.trustProjectHelp}</p>

            <h3>{dictionary.recentProjects}</h3>
            <div className="recent-list">
              {recentProjects.length ? (
                recentProjects.map((project) => (
                  <button key={project} type="button" onClick={() => handleOpenProject(project)}>
                    {project}
                  </button>
                ))
              ) : (
                <span>{dictionary.noData}</span>
              )}
            </div>
          </section>

          <section className="panel threads-panel">
            <div className="panel-title split-title">
              <div>
                <MessageSquare aria-hidden="true" size={18} />
                <h2>{dictionary.threads}</h2>
              </div>
              <button className="icon-button small-icon" type="button" onClick={() => void refreshThreads()} title={dictionary.refreshThreads}>
                <ListRestart aria-hidden="true" size={17} />
              </button>
            </div>
            <button className="primary-button full-button" type="button" onClick={() => void handleCreateThread()}>
              <Plus aria-hidden="true" size={18} />
              <span>{dictionary.newThread}</span>
            </button>
            <button className="secondary-button full-button" type="button" onClick={() => void handleResumeThread()}>
              <Play aria-hidden="true" size={18} />
              <span>{dictionary.resumeThread}</span>
            </button>
            <div className="thread-list">
              {threads.length ? (
                threads.map((thread) => (
                  <button
                    key={thread.id}
                    type="button"
                    className={thread.id === selectedThreadId ? "selected" : ""}
                    onClick={() => setSelectedThreadId(thread.id)}
                  >
                    <span>{formatThreadTitle(thread)}</span>
                    <small>{thread.updated_at ?? thread.model ?? thread.mode ?? ""}</small>
                  </button>
                ))
              ) : (
                <span>{dictionary.noThreads}</span>
              )}
            </div>
          </section>

          <section className="panel threads-panel">
            <div className="panel-title split-title">
              <div>
                <ListRestart aria-hidden="true" size={18} />
                <h2>{dictionary.sessions}</h2>
              </div>
              <button className="icon-button small-icon" type="button" onClick={() => void refreshSessions()} title={dictionary.refreshSessions}>
                <RefreshCw aria-hidden="true" size={17} />
              </button>
            </div>
            <div className="thread-list">
              {sessions.length ? (
                sessions.map((session) => (
                  <button key={session.id} type="button" onClick={() => void handleResumeSession(session.id)}>
                    <span>{session.title || session.id}</span>
                    <small>
                      {session.message_count} · {session.model}
                    </small>
                  </button>
                ))
              ) : (
                <span>{dictionary.noSessions}</span>
              )}
            </div>
          </section>
        </aside>

        <section className="conversation-surface">
          <div className="conversation-header">
            <div>
              <h2>{selectedThread ? formatThreadTitle(selectedThread) : dictionary.messageStream}</h2>
              <span>
                {selectedThread
                  ? `${selectedThread.id} · ${selectedThreadDetail?.items.length ?? 0} items`
                  : dictionary.emptyConversation}
              </span>
            </div>
            {activeTurnId ? (
              <button className="secondary-button" type="button" onClick={() => void handleInterrupt()}>
                <Square aria-hidden="true" size={16} />
                <span>{dictionary.stop}</span>
              </button>
            ) : null}
          </div>

          <div className="message-list">
            {selectedEvents.length ? (
              selectedEvents.map((event) => (
                <article
                  key={`${event.seq}-${event.event}`}
                  className={event.event === "user_message.local" ? "message-bubble user-message" : "message-bubble agent-message"}
                >
                  <small>{event.event}</small>
                  <p>{eventText(event)}</p>
                </article>
              ))
            ) : (
              <div className="workspace-empty">
                <Activity aria-hidden="true" size={24} />
                <h2>{dictionary.workspace}</h2>
                <p>{dictionary.emptyConversation}</p>
                {notice ? <div className="notice">{notice}</div> : null}
              </div>
            )}
          </div>

          <form
            className="composer"
            onSubmit={(event) => {
              event.preventDefault();
              void handleSend();
            }}
          >
            <textarea
              value={composer}
              onChange={(event) => setComposer(event.target.value)}
              placeholder={dictionary.promptPlaceholder}
              rows={3}
            />
            <button className="primary-button send-button" type="submit">
              <Send aria-hidden="true" size={18} />
              <span>{dictionary.send}</span>
            </button>
          </form>
        </section>

        <aside className="activity-rail">
          <section className="panel runtime-panel">
            <div className="panel-title">
              <TerminalSquare aria-hidden="true" size={18} />
              <h2>{dictionary.runtime}</h2>
            </div>

            <dl className="meta-list">
              <div>
                <dt>{dictionary.health}</dt>
                <dd>{health?.ok ? dictionary.runtimeHealthy : health?.error ?? dictionary.unavailable}</dd>
              </div>
              <div>
                <dt>{dictionary.provider}</dt>
                <dd>{config?.provider ?? dictionary.loading}</dd>
              </div>
              <div>
                <dt>{dictionary.model}</dt>
                <dd>{config?.model || dictionary.missing}</dd>
              </div>
              <div>
                <dt>{dictionary.runtimePort}</dt>
                <dd>{config?.runtime_port ?? dictionary.loading}</dd>
              </div>
              <div>
                <dt>{dictionary.activeTurn}</dt>
                <dd>{activeTurnId ?? dictionary.noData}</dd>
              </div>
              <div>
                <dt>{dictionary.lastUpdated}</dt>
                <dd>{updatedAt ?? dictionary.noData}</dd>
              </div>
            </dl>
          </section>

          <section className="panel activity-panel">
            <div className="panel-title">
              <Activity aria-hidden="true" size={18} />
              <h2>{dictionary.gitChanges}</h2>
            </div>
            {workspaceStatus?.git_repo ? (
              <dl className="meta-list compact-meta">
                <div>
                  <dt>{dictionary.branch}</dt>
                  <dd>{workspaceStatus.branch ?? dictionary.noData}</dd>
                </div>
                <div>
                  <dt>{dictionary.staged}</dt>
                  <dd>{workspaceStatus.staged}</dd>
                </div>
                <div>
                  <dt>{dictionary.unstaged}</dt>
                  <dd>{workspaceStatus.unstaged}</dd>
                </div>
                <div>
                  <dt>{dictionary.untracked}</dt>
                  <dd>{workspaceStatus.untracked}</dd>
                </div>
                <div>
                  <dt>{dictionary.aheadBehind}</dt>
                  <dd>
                    {workspaceStatus.ahead ?? 0}/{workspaceStatus.behind ?? 0}
                  </dd>
                </div>
              </dl>
            ) : (
              <span className="help-text">{workspaceStatus ? dictionary.notGitRepo : dictionary.noData}</span>
            )}
          </section>

          <section className="panel activity-panel">
            <div className="panel-title">
              <ShieldCheck aria-hidden="true" size={18} />
              <h2>{dictionary.approvals}</h2>
            </div>
            <div className="approval-list">
              {pendingApprovals.length ? (
                pendingApprovals.map((event) => (
                  <article className="approval-card" key={`${event.seq}-${approvalId(event)}`}>
                    <strong>{payloadString(event, "tool_name") || approvalId(event)}</strong>
                    <p>{payloadString(event, "intent_summary") || payloadString(event, "description") || dictionary.noData}</p>
                    <div className="approval-actions">
                      <button className="primary-button" type="button" onClick={() => void handleApprovalDecision(event, "allow")}>
                        {dictionary.allowOnce}
                      </button>
                      <button className="secondary-button" type="button" onClick={() => void handleApprovalDecision(event, "deny")}>
                        {dictionary.deny}
                      </button>
                      <button className="secondary-button" type="button" onClick={() => void handleApprovalDecision(event, "allow", true)}>
                        {dictionary.rememberDecision}
                      </button>
                    </div>
                  </article>
                ))
              ) : (
                <span className="help-text">{dictionary.noApprovals}</span>
              )}
            </div>
          </section>

          <section className="panel settings-panel">
            <div className="panel-title">
              <Settings aria-hidden="true" size={18} />
              <h2>{dictionary.settings}</h2>
            </div>
            {settingsForm ? (
              <div className="settings-grid">
                <label className="select-label">
                  <span>{dictionary.provider}</span>
                  <select
                    value={settingsForm.provider}
                    onChange={(event) => updateSettings("provider", event.target.value as SettingsForm["provider"])}
                  >
                    <option value="deepseek">{dictionary.deepseekProvider}</option>
                    <option value="openai-compatible">{dictionary.openAICompatibleProvider}</option>
                  </select>
                </label>
                <label className="text-label">
                  <span>{dictionary.baseUrl}</span>
                  <input
                    value={settingsForm.base_url}
                    onChange={(event) => updateSettings("base_url", event.target.value)}
                  />
                </label>
                <label className="text-label">
                  <span>{dictionary.apiKey}</span>
                  <input
                    value={settingsForm.api_key}
                    onChange={(event) => updateSettings("api_key", event.target.value)}
                    placeholder={dictionary.apiKeyPlaceholder}
                    type="password"
                  />
                </label>
                <label className="text-label">
                  <span>{dictionary.model}</span>
                  <input value={settingsForm.model} onChange={(event) => updateSettings("model", event.target.value)} />
                </label>
                <label className="text-label">
                  <span>{dictionary.runtimeHost}</span>
                  <input
                    value={settingsForm.runtime_host}
                    onChange={(event) => updateSettings("runtime_host", event.target.value)}
                  />
                </label>
                <label className="text-label">
                  <span>{dictionary.runtimePort}</span>
                  <input
                    min={1}
                    max={65535}
                    type="number"
                    value={settingsForm.runtime_port}
                    onChange={(event) => updateSettings("runtime_port", Number(event.target.value))}
                  />
                </label>
                <label className="text-label">
                  <span>{dictionary.runtimeCommand}</span>
                  <input
                    value={settingsForm.runtime_command}
                    onChange={(event) => updateSettings("runtime_command", event.target.value)}
                  />
                </label>
                <div>
                  <span className="field-caption">{dictionary.language}</span>
                  <div className="segmented">
                    <button
                      type="button"
                      className={language === "zh-CN" ? "selected" : ""}
                      onClick={() => handleLanguage("zh-CN")}
                    >
                      {dictionary.zh}
                    </button>
                    <button
                      type="button"
                      className={language === "en-US" ? "selected" : ""}
                      onClick={() => handleLanguage("en-US")}
                    >
                      {dictionary.en}
                    </button>
                  </div>
                </div>
                <label className="select-label">
                  <span>{dictionary.theme}</span>
                  <select value={theme} onChange={(event) => handleTheme(event.target.value as ThemeMode)}>
                    <option value="system">{dictionary.system}</option>
                    <option value="light">{dictionary.light}</option>
                    <option value="dark">{dictionary.dark}</option>
                  </select>
                </label>
                <button className="primary-button full-button" type="button" onClick={() => void handleSaveSettings()}>
                  <Settings aria-hidden="true" size={18} />
                  <span>{dictionary.saveSettings}</span>
                </button>
              </div>
            ) : (
              <span className="help-text">{dictionary.loading}</span>
            )}
          </section>

          <section className="panel diagnostics-panel">
            <div className="panel-title">
              <Settings aria-hidden="true" size={18} />
              <h2>{dictionary.diagnostics}</h2>
            </div>
            {notice ? <div className="notice">{notice}</div> : null}
            <h3>{dictionary.eventStream}</h3>
            <pre>{formatJson(selectedEvents.at(-1)) || dictionary.noData}</pre>
            <h3>{dictionary.runtimeInfo}</h3>
            <pre>{formatJson(runtimeInfo?.data) || runtimeInfo?.error || dictionary.noData}</pre>
            <h3>{dictionary.doctor}</h3>
            <pre>{doctor?.stdout || doctor?.stderr || dictionary.noData}</pre>
          </section>
        </aside>
      </main>
    </div>
  );
}
