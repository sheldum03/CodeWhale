#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env, fs,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::Duration,
};
use tauri::State;
use uuid::Uuid;

struct RuntimeState {
    child: Mutex<Option<Child>>,
    token: Mutex<Option<String>>,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            child: Mutex::new(None),
            token: Mutex::new(None),
        }
    }
}

#[derive(Clone, Serialize)]
struct DesktopConfig {
    provider: String,
    base_url: String,
    api_key_present: bool,
    model: String,
    runtime_host: String,
    runtime_port: u16,
    runtime_token: String,
    runtime_command: String,
    language: String,
    theme: String,
    env_path: String,
}

#[derive(Deserialize)]
struct SaveDesktopConfigRequest {
    provider: String,
    base_url: String,
    api_key: Option<String>,
    model: String,
    runtime_host: String,
    runtime_port: u16,
    runtime_command: String,
    language: String,
    theme: String,
}

#[derive(Serialize)]
struct RuntimeLaunchResult {
    attached_existing: bool,
    pid: Option<u32>,
    message: String,
}

#[derive(Serialize)]
struct DoctorResult {
    success: bool,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn desktop_dir() -> PathBuf {
    if cfg!(debug_assertions) {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        return manifest_dir
            .parent()
            .map(PathBuf::from)
            .unwrap_or(manifest_dir);
    }

    env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("CodeWhale")
}

fn env_path() -> PathBuf {
    desktop_dir().join(".env")
}

fn read_dotenv() -> HashMap<String, String> {
    let path = env_path();
    let Ok(contents) = fs::read_to_string(path) else {
        return HashMap::new();
    };

    contents
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }

            let (key, value) = trimmed.split_once('=')?;
            let value = value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            Some((key.trim().to_string(), value))
        })
        .collect()
}

fn dotenv_value(envs: &HashMap<String, String>, key: &str, default: &str) -> String {
    envs.get(key)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| default.to_string())
}

fn validate_provider(provider: &str) -> Result<(), String> {
    match provider {
        "deepseek" | "openai-compatible" => Ok(()),
        _ => Err(format!("unsupported provider: {provider}")),
    }
}

fn validate_theme(theme: &str) -> Result<(), String> {
    match theme {
        "system" | "light" | "dark" => Ok(()),
        _ => Err(format!("unsupported theme: {theme}")),
    }
}

fn validate_language(language: &str) -> Result<(), String> {
    match language {
        "zh-CN" | "en-US" => Ok(()),
        _ => Err(format!("unsupported language: {language}")),
    }
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

fn dotenv_escape(value: &str) -> String {
    value
        .replace('\r', "")
        .replace('\n', "")
        .replace('"', "\\\"")
}

fn render_dotenv(req: &SaveDesktopConfigRequest, api_key: &str) -> String {
    format!(
        "CODEWHALE_PROVIDER=\"{}\"\n\
         CODEWHALE_BASE_URL=\"{}\"\n\
         CODEWHALE_API_KEY=\"{}\"\n\
         CODEWHALE_MODEL=\"{}\"\n\
         CODEWHALE_RUNTIME_HOST=\"{}\"\n\
         CODEWHALE_RUNTIME_PORT={}\n\
         CODEWHALE_RUNTIME_COMMAND=\"{}\"\n\
         CODEWHALE_LANGUAGE=\"{}\"\n\
         CODEWHALE_THEME=\"{}\"\n",
        dotenv_escape(&req.provider),
        dotenv_escape(&req.base_url),
        dotenv_escape(api_key),
        dotenv_escape(&req.model),
        dotenv_escape(&req.runtime_host),
        req.runtime_port,
        dotenv_escape(&req.runtime_command),
        dotenv_escape(&req.language),
        dotenv_escape(&req.theme),
    )
}

fn runtime_token(state: &RuntimeState, envs: &HashMap<String, String>) -> Result<String, String> {
    if let Some(token) = envs
        .get("CODEWHALE_RUNTIME_TOKEN")
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(token.clone());
    }

    let mut token = state
        .token
        .lock()
        .map_err(|_| "runtime token lock poisoned".to_string())?;

    if token.is_none() {
        *token = Some(format!("cw-{}", Uuid::new_v4()));
    }

    token
        .clone()
        .ok_or_else(|| "failed to allocate runtime token".to_string())
}

fn read_config(state: &RuntimeState) -> Result<DesktopConfig, String> {
    let envs = read_dotenv();
    let provider = dotenv_value(&envs, "CODEWHALE_PROVIDER", "deepseek");
    let base_url = dotenv_value(&envs, "CODEWHALE_BASE_URL", "https://api.deepseek.com");
    let model = dotenv_value(&envs, "CODEWHALE_MODEL", "deepseek-v4-pro");
    let runtime_host = dotenv_value(&envs, "CODEWHALE_RUNTIME_HOST", "127.0.0.1");
    if !is_loopback_host(&runtime_host) {
        return Err(format!(
            "CODEWHALE_RUNTIME_HOST must be localhost for P0, got {runtime_host}"
        ));
    }

    let runtime_port = dotenv_value(&envs, "CODEWHALE_RUNTIME_PORT", "7878")
        .parse::<u16>()
        .map_err(|error| format!("invalid CODEWHALE_RUNTIME_PORT: {error}"))?;
    let runtime_command = dotenv_value(&envs, "CODEWHALE_RUNTIME_COMMAND", "codewhale.cmd");
    let language = dotenv_value(&envs, "CODEWHALE_LANGUAGE", "zh-CN");
    let theme = dotenv_value(&envs, "CODEWHALE_THEME", "system");
    let runtime_token = runtime_token(state, &envs)?;

    Ok(DesktopConfig {
        provider,
        base_url,
        api_key_present: envs
            .get("CODEWHALE_API_KEY")
            .is_some_and(|value| !value.trim().is_empty()),
        model,
        runtime_host,
        runtime_port,
        runtime_token,
        runtime_command,
        language,
        theme,
        env_path: env_path().display().to_string(),
    })
}

fn save_config(state: &RuntimeState, req: SaveDesktopConfigRequest) -> Result<DesktopConfig, String> {
    validate_provider(&req.provider)?;
    validate_language(&req.language)?;
    validate_theme(&req.theme)?;

    if req.base_url.trim().is_empty() {
        return Err("CODEWHALE_BASE_URL is required".to_string());
    }
    if req.model.trim().is_empty() {
        return Err("CODEWHALE_MODEL is required".to_string());
    }
    if !is_loopback_host(req.runtime_host.trim()) {
        return Err(format!(
            "CODEWHALE_RUNTIME_HOST must be localhost, got {}",
            req.runtime_host
        ));
    }
    if req.runtime_command.trim().is_empty() {
        return Err("CODEWHALE_RUNTIME_COMMAND is required".to_string());
    }

    let existing = read_dotenv();
    let api_key = req
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| existing.get("CODEWHALE_API_KEY").cloned())
        .unwrap_or_default();

    let dir = desktop_dir();
    fs::create_dir_all(&dir)
        .map_err(|error| format!("failed to create {}: {error}", dir.display()))?;
    fs::write(env_path(), render_dotenv(&req, &api_key))
        .map_err(|error| format!("failed to write .env: {error}"))?;

    read_config(state)
}

fn health_check(config: &DesktopConfig) -> bool {
    let Ok(mut addrs) = (config.runtime_host.as_str(), config.runtime_port).to_socket_addrs()
    else {
        return false;
    };

    let Some(addr) = addrs.next() else {
        return false;
    };

    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(450)) else {
        return false;
    };

    let _ = stream.set_read_timeout(Some(Duration::from_millis(450)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(450)));

    let request = format!(
        "GET /health HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n\r\n",
        config.runtime_host, config.runtime_port
    );

    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }

    let mut buffer = [0_u8; 64];
    let Ok(size) = stream.read(&mut buffer) else {
        return false;
    };

    String::from_utf8_lossy(&buffer[..size]).starts_with("HTTP/1.1 200")
        || String::from_utf8_lossy(&buffer[..size]).starts_with("HTTP/1.0 200")
}

fn apply_provider_env(command: &mut Command, envs: &HashMap<String, String>, config: &DesktopConfig) {
    command.env("CODEWHALE_PROVIDER", &config.provider);
    command.env("CODEWHALE_BASE_URL", &config.base_url);
    command.env("CODEWHALE_MODEL", &config.model);
    command.env("DEEPSEEK_RUNTIME_TOKEN", &config.runtime_token);

    if let Some(api_key) = envs
        .get("CODEWHALE_API_KEY")
        .filter(|value| !value.trim().is_empty())
    {
        if config.provider == "openai-compatible" || config.provider == "openai" {
            command.env("CODEWHALE_PROVIDER", "openai");
            command.env("OPENAI_API_KEY", api_key);
            command.env("OPENAI_BASE_URL", &config.base_url);
            command.env("OPENAI_MODEL", &config.model);
        } else {
            command.env("DEEPSEEK_API_KEY", api_key);
            command.env("DEEPSEEK_BASE_URL", &config.base_url);
            command.env("DEEPSEEK_MODEL", &config.model);
        }
    }
}

#[tauri::command]
fn load_desktop_config(state: State<'_, RuntimeState>) -> Result<DesktopConfig, String> {
    read_config(&state)
}

#[tauri::command]
fn save_desktop_config(
    state: State<'_, RuntimeState>,
    req: SaveDesktopConfigRequest,
) -> Result<DesktopConfig, String> {
    save_config(&state, req)
}

#[tauri::command]
fn select_project_directory() -> Result<Option<String>, String> {
    Ok(rfd::FileDialog::new()
        .set_title("Select CodeWhale project")
        .pick_folder()
        .map(|path| path.display().to_string()))
}

#[tauri::command]
fn start_runtime(state: State<'_, RuntimeState>) -> Result<RuntimeLaunchResult, String> {
    let config = read_config(&state)?;
    let envs = read_dotenv();

    if health_check(&config) {
        return Ok(RuntimeLaunchResult {
            attached_existing: true,
            pid: None,
            message: "runtime health endpoint is already available".to_string(),
        });
    }

    let mut child_guard = state
        .child
        .lock()
        .map_err(|_| "runtime child lock poisoned".to_string())?;

    if let Some(child) = child_guard.as_mut() {
        if child.try_wait().map_err(|error| error.to_string())?.is_none() {
            return Ok(RuntimeLaunchResult {
                attached_existing: false,
                pid: Some(child.id()),
                message: "runtime already started by desktop".to_string(),
            });
        }
        *child_guard = None;
    }

    let mut command = Command::new(&config.runtime_command);
    command
        .arg("serve")
        .arg("--http")
        .arg("--host")
        .arg(&config.runtime_host)
        .arg("--port")
        .arg(config.runtime_port.to_string())
        .arg("--auth-token")
        .arg(&config.runtime_token)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    apply_provider_env(&mut command, &envs, &config);

    let child = command
        .spawn()
        .map_err(|error| format!("failed to start {}: {error}", config.runtime_command))?;
    let pid = child.id();
    *child_guard = Some(child);

    Ok(RuntimeLaunchResult {
        attached_existing: false,
        pid: Some(pid),
        message: "runtime start requested".to_string(),
    })
}

#[tauri::command]
fn stop_runtime(state: State<'_, RuntimeState>) -> Result<(), String> {
    let mut child_guard = state
        .child
        .lock()
        .map_err(|_| "runtime child lock poisoned".to_string())?;

    if let Some(mut child) = child_guard.take() {
        let _ = child.kill();
        let _ = child.wait();
    }

    Ok(())
}

#[tauri::command]
fn run_doctor(state: State<'_, RuntimeState>) -> Result<DoctorResult, String> {
    let config = read_config(&state)?;
    let envs = read_dotenv();
    let mut command = Command::new(&config.runtime_command);
    command.arg("doctor").arg("--json");
    apply_provider_env(&mut command, &envs, &config);

    let output = command
        .output()
        .map_err(|error| format!("failed to run {} doctor --json: {error}", config.runtime_command))?;

    Ok(DoctorResult {
        success: output.status.success(),
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn main() {
    tauri::Builder::default()
        .manage(RuntimeState::default())
        .invoke_handler(tauri::generate_handler![
            load_desktop_config,
            save_desktop_config,
            select_project_directory,
            start_runtime,
            stop_runtime,
            run_doctor
        ])
        .run(tauri::generate_context!())
        .expect("error while running CodeWhale desktop");
}
