//! Ollama on this computer: find it, install it (whitelisted winget / brew command, same runner
//! as the first-run setup), start its server in the background, list / download / delete models.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use nanomgg_providers::CancellationToken;
use serde::Serialize;
use serde_json::{Value, json};
use specta::Type;

use super::AiError;
use crate::onboarding::{
    InstallPlan, Managers, ProbeEnv, ProgressTracker, SetupInstallOutcome, SetupInstallPhase,
    SetupPackageManager, TargetOs, classify, clean_output, command as no_window_command, find_in,
    run_tracked, which,
};

pub const DOWNLOAD_URL: &str = "https://ollama.com/download";
const VERSION_TIMEOUT: Duration = Duration::from_secs(3);
const START_WAIT: Duration = Duration::from_secs(30);
/// No byte from `/api/pull` for this long: the download is stuck.
const PULL_IDLE: Duration = Duration::from_secs(10 * 60);
const PULL_EMIT_EVERY: Duration = Duration::from_millis(200);
pub const INSTALL_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AiHostOs {
    Windows,
    Macos,
    Linux,
}

impl From<TargetOs> for AiHostOs {
    fn from(os: TargetOs) -> Self {
        match os {
            TargetOs::Windows => Self::Windows,
            TargetOs::Macos => Self::Macos,
            TargetOs::Linux => Self::Linux,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OllamaModel {
    pub name: String,
    /// Bytes on disk.
    pub size: u64,
    pub parameter_size: Option<String>,
    pub quantization: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OllamaStatus {
    /// The server answers on the configured address.
    pub running: bool,
    pub version: Option<String>,
    /// The program was found (CLI or, on macOS, Ollama.app).
    pub installed: bool,
    pub cli_path: Option<String>,
    /// macOS `Ollama.app`.
    pub app_path: Option<String>,
    /// Models already downloaded (empty while the server is down).
    pub models: Vec<OllamaModel>,
    /// "Cài Ollama giúp tôi" is possible with this package manager.
    pub installer: Option<SetupPackageManager>,
    pub os: AiHostOs,
    pub download_url: String,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiInstallResult {
    pub outcome: SetupInstallOutcome,
    pub manager: Option<SetupPackageManager>,
    pub command: Option<String>,
    pub exit_code: Option<i32>,
    pub output: String,
}

/// Download progress of `ai_ollama_pull` (event `ai-pull-progress`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct AiPullProgress {
    pub request_id: String,
    pub model: String,
    /// Ollama's status line (`pulling manifest`, `pulling 2bada8a74506`, `verifying sha256 digest`,
    /// `success`).
    pub status: String,
    /// Bytes over every layer seen so far.
    pub completed: u64,
    pub total: u64,
    pub percent: Option<u8>,
    pub done: bool,
}

// ----- finding ---------------------------------------------------------------------------------

/// The CLI (`PATH`, then the default install folders) and, on macOS, the app bundle.
pub fn locate(env: &ProbeEnv, mac_apps: &[PathBuf]) -> (Option<PathBuf>, Option<PathBuf>) {
    let mut dirs: Vec<PathBuf> = Vec::new();
    match env.os {
        TargetOs::Windows => {
            if let Some(l) = &env.local_app_data {
                dirs.push(l.join("Programs").join("Ollama"));
            }
            dirs.extend(env.program_files.iter().map(|p| p.join("Ollama")));
        }
        TargetOs::Macos => {
            dirs.extend(env.unix_dirs.iter().cloned());
            dirs.extend(
                mac_apps
                    .iter()
                    .map(|a| a.join("Contents").join("Resources")),
            );
        }
        TargetOs::Linux => dirs.extend(env.unix_dirs.iter().cloned()),
    }
    let cli = which("ollama", env).or_else(|| dirs.iter().find_map(|d| find_in(d, "ollama", env)));
    let app = (env.os == TargetOs::Macos)
        .then(|| mac_apps.iter().find(|a| a.is_dir()).cloned())
        .flatten();
    (cli, app)
}

pub fn default_mac_apps() -> Vec<PathBuf> {
    let mut apps = vec![PathBuf::from("/Applications/Ollama.app")];
    if let Some(home) = std::env::var_os("HOME") {
        apps.push(PathBuf::from(home).join("Applications").join("Ollama.app"));
    }
    apps
}

pub fn install_plan(os: TargetOs, m: &Managers) -> Option<InstallPlan> {
    match os {
        TargetOs::Windows => Some(InstallPlan {
            manager: SetupPackageManager::Winget,
            program: m.winget.clone()?,
            args: vec![
                "install",
                "--id",
                "Ollama.Ollama",
                "-e",
                "--source",
                "winget",
                "--accept-source-agreements",
                "--accept-package-agreements",
            ],
        }),
        TargetOs::Macos => Some(InstallPlan {
            manager: SetupPackageManager::Brew,
            program: m.brew.clone()?,
            args: vec!["install", "--cask", "ollama"],
        }),
        TargetOs::Linux => None,
    }
}

// ----- server ----------------------------------------------------------------------------------

/// Server version, or `None` when nothing answers.
pub async fn version(http: &reqwest::Client, base: &str) -> Option<String> {
    let resp = http
        .get(format!("{base}/api/version"))
        .timeout(VERSION_TIMEOUT)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: Value = serde_json::from_slice(&resp.bytes().await.ok()?).ok()?;
    Some(
        v.get("version")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_owned(),
    )
}

pub async fn models(http: &reqwest::Client, base: &str) -> Result<Vec<OllamaModel>, AiError> {
    let resp = http
        .get(format!("{base}/api/tags"))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| AiError::OllamaNotRunning {
            message: e.without_url().to_string(),
        })?;
    let status = resp.status().as_u16();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AiError::network(e.without_url().to_string()))?;
    if !(200..300).contains(&status) {
        return Err(AiError::Service {
            message: super::client::error_detail(&String::from_utf8_lossy(&bytes), None),
            status,
        });
    }
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    let mut out: Vec<OllamaModel> = v
        .get("models")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|m| {
                    Some(OllamaModel {
                        name: m
                            .get("name")
                            .or_else(|| m.get("model"))?
                            .as_str()?
                            .to_owned(),
                        size: m.get("size").and_then(Value::as_u64).unwrap_or(0),
                        parameter_size: m
                            .pointer("/details/parameter_size")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        quantization: m
                            .pointer("/details/quantization_level")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub async fn status(http: &reqwest::Client, base: &str, env: &ProbeEnv) -> OllamaStatus {
    let (cli, app) = locate(env, &default_mac_apps());
    let version = version(http, base).await;
    let models = if version.is_some() {
        models(http, base).await.unwrap_or_default()
    } else {
        Vec::new()
    };
    let managers = Managers::detect(env);
    OllamaStatus {
        running: version.is_some(),
        version,
        // A running server counts as installed even if its program is somewhere unusual.
        installed: cli.is_some() || app.is_some(),
        cli_path: cli.map(|p| p.display().to_string()),
        app_path: app.map(|p| p.display().to_string()),
        models,
        installer: install_plan(env.os, &managers).map(|p| p.manager),
        os: env.os.into(),
        download_url: DOWNLOAD_URL.into(),
    }
}

/// Starts the server detached from the app (no console window; it keeps running after the app
/// closes, as when Ollama starts itself at login). macOS prefers opening Ollama.app.
pub fn spawn_server(os: TargetOs, cli: Option<&Path>, app: Option<&Path>) -> Result<(), AiError> {
    if os == TargetOs::Macos
        && let Some(app) = app
    {
        let mut cmd = no_window_command(Path::new("/usr/bin/open"));
        cmd.arg("-a")
            .arg(app)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let status = cmd
            .status()
            .map_err(|e| AiError::internal(format!("cannot open Ollama.app: {e}")))?;
        if status.success() {
            return Ok(());
        }
    }
    let cli = cli.ok_or_else(|| AiError::OllamaNotInstalled {
        message: "Ollama is not installed".into(),
    })?;
    let mut cmd = no_window_command(cli);
    cmd.arg("serve").stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    // The child is not waited for: dropping it does not kill it.
    cmd.spawn()
        .map_err(|e| AiError::internal(format!("cannot start `ollama serve`: {e}")))?;
    tracing::info!(cli = %cli.display(), "started ollama serve");
    Ok(())
}

/// Polls `/api/version` until the server answers (or `wait` passes).
pub async fn wait_until_up(
    http: &reqwest::Client,
    base: &str,
    wait: Duration,
    cancel: &CancellationToken,
) -> Result<String, AiError> {
    let start = Instant::now();
    loop {
        if let Some(v) = version(http, base).await {
            return Ok(v);
        }
        if cancel.is_cancelled() {
            return Err(AiError::Cancelled {
                message: "cancelled".into(),
            });
        }
        if start.elapsed() > wait {
            return Err(AiError::OllamaNotRunning {
                message: format!("Ollama did not answer within {} seconds", wait.as_secs()),
            });
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

pub fn start_wait() -> Duration {
    START_WAIT
}

// ----- install ---------------------------------------------------------------------------------

/// Live progress of `ai_ollama_install` (event `ai-install-progress`); same reading of the
/// installer's output as the setup's "Install it for me".
#[derive(Debug, Clone, PartialEq, Serialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct AiInstallProgress {
    pub phase: SetupInstallPhase,
    pub percent: Option<u8>,
    /// Last few readable output lines, oldest first.
    pub recent: Vec<String>,
}

impl AiInstallProgress {
    pub(crate) fn of(t: &ProgressTracker) -> Self {
        Self {
            phase: t.phase(),
            percent: t.percent(),
            recent: t.recent(),
        }
    }
}

pub fn run_install(
    plan: &InstallPlan,
    cancel: &AtomicBool,
    timeout: Duration,
    emit: &mut dyn FnMut(AiInstallProgress),
) -> AiInstallResult {
    let result = |outcome, exit_code, output: String| AiInstallResult {
        outcome,
        manager: Some(plan.manager),
        command: Some(plan.display()),
        exit_code,
        output,
    };
    let mut on_progress = |t: &ProgressTracker| emit(AiInstallProgress::of(t));
    match run_tracked(plan, cancel, timeout, &mut on_progress) {
        Err(e) => result(SetupInstallOutcome::ManagerMissing, None, e.to_string()),
        Ok(out) => {
            let outcome = if out.cancelled {
                SetupInstallOutcome::Cancelled
            } else if out.timed_out {
                SetupInstallOutcome::TimedOut
            } else {
                classify(plan.manager, out.exit_code, &out.text)
            };
            tracing::info!(?outcome, code = ?out.exit_code, "ollama install finished");
            result(outcome, out.exit_code, clean_output(&out.text))
        }
    }
}

// ----- models ----------------------------------------------------------------------------------

/// `qwen2.5-coder:1.5b`, `library/llama3.2:3b`, `hf.co/user/repo:Q4_K_M`.
pub fn valid_model_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.starts_with(['-', '.', '/', ':'])
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | ':'))
}

/// Sums the layers of a pull: `(completed, total)`.
#[derive(Default)]
pub struct PullTally {
    layers: HashMap<String, (u64, u64)>,
}

impl PullTally {
    pub fn update(&mut self, line: &Value) {
        if let (Some(d), Some(t)) = (
            line.get("digest").and_then(Value::as_str),
            line.get("total").and_then(Value::as_u64),
        ) {
            let c = line.get("completed").and_then(Value::as_u64).unwrap_or(0);
            self.layers.insert(d.to_owned(), (c.min(t), t));
        }
    }

    pub fn sum(&self) -> (u64, u64) {
        self.layers
            .values()
            .fold((0, 0), |(c, t), (lc, lt)| (c + lc, t + lt))
    }
}

fn pull_error(msg: &str, model: &str) -> AiError {
    let lower = msg.to_ascii_lowercase();
    let message = super::client::error_detail(msg, None);
    if lower.contains("file does not exist") || lower.contains("not found") {
        AiError::ModelNotFound {
            message,
            model: model.to_owned(),
        }
    } else if lower.contains("dial tcp")
        || lower.contains("no such host")
        || lower.contains("connection")
        || lower.contains("timeout")
    {
        AiError::Network { message }
    } else {
        AiError::Service {
            message,
            status: 500,
        }
    }
}

/// Downloads `model` with `POST /api/pull` (streamed JSON lines), reporting progress.
pub async fn pull(
    http: &reqwest::Client,
    base: &str,
    model: &str,
    request_id: &str,
    cancel: &CancellationToken,
    mut emit: impl FnMut(AiPullProgress),
) -> Result<(), AiError> {
    if !valid_model_name(model) {
        return Err(AiError::invalid("model name is not valid"));
    }
    let body = json!({ "model": model, "name": model, "stream": true });
    let send = http
        .post(format!("{base}/api/pull"))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body.to_string())
        // Big models take long; stalls are caught by `PULL_IDLE` instead.
        .timeout(Duration::from_secs(24 * 3600))
        .send();
    let mut resp = super::client::with_cancel(cancel, async {
        send.await.map_err(|e| AiError::OllamaNotRunning {
            message: e.without_url().to_string(),
        })
    })
    .await?;
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        let text = resp.text().await.unwrap_or_default();
        return Err(pull_error(&super::client::error_detail(&text, None), model));
    }
    let mut tally = PullTally::default();
    let mut buf: Vec<u8> = Vec::new();
    let mut last_emit: Option<Instant> = None;
    let mut last_status = String::new();
    let progress = |status: &str, tally: &PullTally, done: bool| {
        let (completed, total) = tally.sum();
        AiPullProgress {
            request_id: request_id.to_owned(),
            model: model.to_owned(),
            status: status.to_owned(),
            completed,
            total,
            percent: (total > 0).then(|| ((completed * 100) / total).min(100) as u8),
            done,
        }
    };
    loop {
        let chunk = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                return Err(AiError::Cancelled { message: "cancelled".into() });
            }
            c = tokio::time::timeout(PULL_IDLE, resp.chunk()) => c,
        };
        let chunk = match chunk {
            Err(_) => {
                return Err(AiError::Timeout {
                    message: "the download stopped sending data".into(),
                });
            }
            Ok(Err(e)) => return Err(AiError::network(e.without_url().to_string())),
            Ok(Ok(None)) => break,
            Ok(Ok(Some(c))) => c,
        };
        buf.extend_from_slice(&chunk);
        while let Some(nl) = buf.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = buf.drain(..=nl).collect();
            let Ok(v) = serde_json::from_slice::<Value>(&line) else {
                continue;
            };
            if let Some(err) = v.get("error").and_then(Value::as_str) {
                return Err(pull_error(err, model));
            }
            tally.update(&v);
            let st = v.get("status").and_then(Value::as_str).unwrap_or_default();
            if st == "success" {
                emit(progress(st, &tally, true));
                tracing::info!(model, "ollama model downloaded");
                return Ok(());
            }
            let due = last_emit.is_none_or(|t| t.elapsed() >= PULL_EMIT_EVERY);
            if due || st != last_status {
                emit(progress(st, &tally, false));
                last_emit = Some(Instant::now());
                last_status = st.to_owned();
            }
        }
    }
    Err(AiError::Service {
        message: "the download ended before it finished".into(),
        status,
    })
}

/// `DELETE /api/delete`.
pub async fn delete(http: &reqwest::Client, base: &str, model: &str) -> Result<(), AiError> {
    if !valid_model_name(model) {
        return Err(AiError::invalid("model name is not valid"));
    }
    let resp = http
        .delete(format!("{base}/api/delete"))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(json!({ "model": model, "name": model }).to_string())
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| AiError::OllamaNotRunning {
            message: e.without_url().to_string(),
        })?;
    let status = resp.status().as_u16();
    if (200..300).contains(&status) {
        tracing::info!(model, "ollama model deleted");
        return Ok(());
    }
    let text = resp.text().await.unwrap_or_default();
    let message = super::client::error_detail(&text, None);
    if status == 404 {
        Err(AiError::ModelNotFound {
            message,
            model: model.to_owned(),
        })
    } else {
        Err(AiError::Service { message, status })
    }
}
