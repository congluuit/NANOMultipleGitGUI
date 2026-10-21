//! Phase 6.4: commit message suggestions by AI (optional, off by default).
//!
//! - Services: Ollama on this computer (free, code stays local), OpenAI, Anthropic, Google
//!   Gemini and any OpenAI-compatible endpoint. Settings in `ai.config` ([`config::AiConfig`]);
//!   API keys in the keychain under fixed owners (`ai_openai`, `ai_anthropic`, `ai_gemini`,
//!   `ai_compatible`, kind `token`) and never sent back to the UI.
//! - `ai_generate` reads the staged diff (lock files / binaries by name only, cut to ~14 KB, see
//!   [`context`]), the branch, an issue number in it and recent subjects, asks the model for JSON
//!   and reads the answer forgivingly ([`prompt`]).
//! - Ollama helpers: status, "install for me" (winget / brew whitelist shared with the first-run
//!   setup), start the server, download / delete models with progress ([`ollama`]).
//! - Every network call can be cancelled with `ai_cancel(requestId)`.

pub mod client;
pub mod config;
pub mod context;
pub mod ollama;
pub mod prompt;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use nanomgg_credentials::SecretStore;
use nanomgg_git_engine::GitRunner;
use nanomgg_providers::CancellationToken;
use nanomgg_storage::Storage;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use zeroize::Zeroizing;

use crate::auth::AuthRuntime;
use crate::error::AppError;
use crate::onboarding::{Managers, ProbeEnv, SetupInstallOutcome, SetupPackageManager};
use crate::repos::repo_root;
use crate::state::AppState;
use client::{Endpoints, Target};
use config::{AiConfig, AiKeyStatus, AiProvider, KEY_AI_CONFIG};
use context::AiDiffSource;
use ollama::{AiInstallResult, OllamaStatus};

/// Errors of the AI commands, serialized as `{ kind, message, ... }` (like [`AppError`]) so the
/// generic `errorMessage()` helper works; the UI explains each kind in plain words.
#[derive(Debug, Clone, thiserror::Error, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AiError {
    #[error("{message}")]
    NotEnabled { message: String },
    #[error("{message}")]
    MissingKey { message: String },
    #[error("{message}")]
    MissingModel { message: String },
    #[error("{message}")]
    MissingBaseUrl { message: String },
    /// Nothing staged; `hasUnstaged` offers "use the unstaged changes".
    #[error("{message}")]
    #[serde(rename_all = "camelCase")]
    NothingStaged { message: String, has_unstaged: bool },
    /// 401 or an "API key not valid" answer.
    #[error("{message}")]
    InvalidKey { message: String },
    /// 403: the key may not use this model / region.
    #[error("{message}")]
    AccessDenied { message: String },
    /// Out of credits / billing.
    #[error("{message}")]
    QuotaExceeded { message: String },
    /// 429 / overloaded: try again later.
    #[error("{message}")]
    RateLimited { message: String },
    #[error("{message}")]
    ModelNotFound { message: String, model: String },
    #[error("{message}")]
    OllamaNotRunning { message: String },
    #[error("{message}")]
    OllamaNotInstalled { message: String },
    #[error("{message}")]
    Network { message: String },
    #[error("{message}")]
    Timeout { message: String },
    #[error("{message}")]
    Cancelled { message: String },
    /// Any other HTTP error of the service.
    #[error("{message}")]
    Service { message: String, status: u16 },
    #[error("{message}")]
    EmptyAnswer { message: String },
    #[error("{message}")]
    Busy { message: String },
    #[error("{message}")]
    InvalidInput { message: String },
    #[error("{message}")]
    Git { message: String },
    #[error("{message}")]
    Internal { message: String },
}

impl AiError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidInput {
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }

    pub fn git(message: impl Into<String>) -> Self {
        Self::Git {
            message: nanomgg_logging::redact(&message.into()).into_owned(),
        }
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::Network {
            message: nanomgg_logging::redact(&message.into()).into_owned(),
        }
    }
}

impl From<AppError> for AiError {
    fn from(e: AppError) -> Self {
        match e {
            AppError::Git(m) => Self::Git { message: m },
            AppError::InvalidInput(m) => Self::InvalidInput { message: m },
            other => Self::internal(other.to_string()),
        }
    }
}

impl From<nanomgg_storage::StorageError> for AiError {
    fn from(e: nanomgg_storage::StorageError) -> Self {
        AppError::from(e).into()
    }
}

pub type AiResult<T> = Result<T, AiError>;

// ----- DTOs ----------------------------------------------------------------------------------

/// Settings as the UI sees them: the config plus which keys are stored.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiSettingsView {
    pub config: AiConfig,
    pub keys: AiKeyStatus,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiTestResult {
    /// Models the service offers (sorted, at most 500).
    pub models: Vec<String>,
    /// The chosen model is among them (`None` when no model is chosen).
    pub model_found: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiSuggestion {
    pub summary: String,
    pub body: String,
    pub source: AiDiffSource,
    /// Part of the diff was left out to keep the request small.
    pub truncated: bool,
    pub provider: AiProvider,
    pub model: String,
}

// ----- running requests ----------------------------------------------------------------------

static RUNNING: Mutex<Option<HashMap<String, CancellationToken>>> = Mutex::new(None);

/// Registers `request_id` for `ai_cancel` while alive.
struct Running(String);

impl Running {
    fn start(request_id: &str) -> AiResult<(Self, CancellationToken)> {
        if request_id.is_empty() || request_id.len() > 100 {
            return Err(AiError::invalid("bad request id"));
        }
        let token = CancellationToken::new();
        let mut map = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
        map.get_or_insert_with(HashMap::new)
            .insert(request_id.to_owned(), token.clone());
        Ok((Self(request_id.to_owned()), token))
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(map) = RUNNING.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            map.remove(&self.0);
        }
    }
}

fn cancel_request(request_id: &str) -> bool {
    RUNNING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|m| m.get(request_id))
        .map(|t| t.cancel())
        .is_some()
}

/// Cancel flag of the Ollama install in progress (one at a time).
static INSTALLING: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

struct InstallSlot;

impl InstallSlot {
    fn take() -> AiResult<(Self, Arc<AtomicBool>)> {
        let mut slot = INSTALLING.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_some() {
            return Err(AiError::Busy {
                message: "Ollama is already being installed".into(),
            });
        }
        let flag = Arc::new(AtomicBool::new(false));
        *slot = Some(flag.clone());
        Ok((Self, flag))
    }
}

impl Drop for InstallSlot {
    fn drop(&mut self) {
        *INSTALLING.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

// ----- implementation (testable without Tauri) ------------------------------------------------

pub fn load_config(storage: &Storage) -> AiResult<AiConfig> {
    Ok(storage.get_setting(KEY_AI_CONFIG)?.unwrap_or_default())
}

pub fn settings_view(storage: &Storage, secrets: &dyn SecretStore) -> AiResult<AiSettingsView> {
    Ok(AiSettingsView {
        config: load_config(storage)?,
        keys: config::key_status(secrets),
    })
}

pub fn save_config(
    storage: &Storage,
    secrets: &dyn SecretStore,
    cfg: AiConfig,
) -> AiResult<AiSettingsView> {
    let cfg = cfg.normalized()?;
    storage.set_setting(KEY_AI_CONFIG, &cfg)?;
    settings_view(storage, secrets)
}

/// Lists the models of the service in `cfg` (unsaved edits are fine) with its stored key.
pub async fn test_impl(
    http: &reqwest::Client,
    secrets: &dyn SecretStore,
    endpoints: &Endpoints,
    cfg: AiConfig,
    cancel: &CancellationToken,
) -> AiResult<AiTestResult> {
    let cfg = cfg.normalized()?;
    let key = config::read_key(secrets, cfg.provider)?;
    let target = Target::new(&cfg, key, endpoints, false)?;
    let models = client::list_models(http, &target, cancel).await?;
    let model_found =
        (!target.model.is_empty()).then(|| client::model_listed(&models, &target.model));
    Ok(AiTestResult {
        models,
        model_found,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn generate_impl(
    http: &reqwest::Client,
    storage: &Storage,
    secrets: &dyn SecretStore,
    endpoints: &Endpoints,
    git: GitRunner,
    root: PathBuf,
    app_language: &str,
    unstaged: bool,
    cancel: &CancellationToken,
) -> AiResult<AiSuggestion> {
    let cfg = load_config(storage)?;
    if !cfg.enabled {
        return Err(AiError::NotEnabled {
            message: "AI suggestions are turned off".into(),
        });
    }
    let key = config::read_key(secrets, cfg.provider)?;
    let target = Target::new(&cfg, key, endpoints, true)?;
    let ctx = tauri::async_runtime::spawn_blocking(move || context::collect(&git, &root, unstaged))
        .await
        .map_err(|e| AiError::internal(e.to_string()))??;
    let language = prompt::resolve_language(cfg.language, app_language);
    let p = prompt::build_prompt(&ctx, language, cfg.style, cfg.include_body);
    tracing::info!(
        provider = ?target.provider,
        model = %target.model,
        files = ctx.files.len(),
        untracked = ctx.untracked.len(),
        bytes = p.user.len(),
        truncated = ctx.truncated,
        "ai commit message requested"
    );
    let raw = client::complete(http, &target, &p, cancel).await?;
    let msg = prompt::parse_answer(&raw, cfg.include_body).ok_or_else(|| AiError::EmptyAnswer {
        message: "the answer had no usable commit message".into(),
    })?;
    Ok(AiSuggestion {
        summary: msg.summary,
        body: msg.body,
        source: ctx.source,
        truncated: ctx.truncated,
        provider: target.provider,
        model: target.model,
    })
}

// ----- commands ------------------------------------------------------------------------------

fn http(app: &AppHandle) -> reqwest::Client {
    app.state::<AuthRuntime>().http.clone()
}

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppState) -> AiResult<T> + Send + 'static,
) -> AiResult<T> {
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>()))
        .await
        .map_err(|e| AiError::internal(e.to_string()))?
}

fn ollama_url(app: &AppHandle) -> AiResult<String> {
    Ok(load_config(&app.state::<AppState>().storage)?.ollama_url)
}

#[tauri::command]
#[specta::specta]
pub async fn ai_settings(app: AppHandle) -> AiResult<AiSettingsView> {
    blocking(app, |st| {
        settings_view(&st.storage, st.accounts.secrets.as_ref())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn ai_settings_save(app: AppHandle, config: AiConfig) -> AiResult<AiSettingsView> {
    blocking(app, move |st| {
        save_config(&st.storage, st.accounts.secrets.as_ref(), config)
    })
    .await
}

/// Stores the API key of a cloud service in the keychain.
#[tauri::command]
#[specta::specta]
pub async fn ai_key_set(
    app: AppHandle,
    provider: AiProvider,
    key: String,
) -> AiResult<AiSettingsView> {
    let key = Zeroizing::new(key);
    blocking(app, move |st| {
        config::store_key(st.accounts.secrets.as_ref(), provider, &key)?;
        tracing::info!(?provider, "ai api key stored");
        settings_view(&st.storage, st.accounts.secrets.as_ref())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn ai_key_delete(app: AppHandle, provider: AiProvider) -> AiResult<AiSettingsView> {
    blocking(app, move |st| {
        if config::delete_key(st.accounts.secrets.as_ref(), provider)? {
            tracing::info!(?provider, "ai api key deleted");
        }
        settings_view(&st.storage, st.accounts.secrets.as_ref())
    })
    .await
}

/// "Kiểm tra kết nối": lists the service's models with `config` (may be unsaved).
#[tauri::command]
#[specta::specta]
pub async fn ai_test(
    app: AppHandle,
    config: AiConfig,
    request_id: String,
) -> AiResult<AiTestResult> {
    let (_running, cancel) = Running::start(&request_id)?;
    let secrets = app.state::<AppState>().accounts.secrets.clone();
    test_impl(
        &http(&app),
        secrets.as_ref(),
        &Endpoints::default(),
        config,
        &cancel,
    )
    .await
}

/// Suggests a commit message for the staged changes of `repo_id` (or, with `unstaged`, the
/// working-tree changes when nothing is staged). `app_language`: `vi` / `en`.
#[tauri::command]
#[specta::specta]
pub async fn ai_generate(
    app: AppHandle,
    repo_id: i64,
    request_id: String,
    app_language: String,
    unstaged: bool,
) -> AiResult<AiSuggestion> {
    let (_running, cancel) = Running::start(&request_id)?;
    let st = app.state::<AppState>();
    let root = repo_root(&st, repo_id)?;
    generate_impl(
        &http(&app),
        &st.storage,
        st.accounts.secrets.as_ref(),
        &Endpoints::default(),
        st.git.clone(),
        root,
        &app_language,
        unstaged,
        &cancel,
    )
    .await
}

/// Cancels a running `ai_generate` / `ai_test` / `ai_ollama_pull` / `ai_ollama_start`.
#[tauri::command]
#[specta::specta]
pub async fn ai_cancel(request_id: String) -> bool {
    cancel_request(&request_id)
}

#[tauri::command]
#[specta::specta]
pub async fn ai_ollama_status(app: AppHandle) -> AiResult<OllamaStatus> {
    let base = ollama_url(&app)?;
    let env = tauri::async_runtime::spawn_blocking(ProbeEnv::current)
        .await
        .map_err(|e| AiError::internal(e.to_string()))?;
    Ok(ollama::status(&http(&app), &base, &env).await)
}

/// Installs Ollama with winget (Windows) / Homebrew (macOS). Linux / no manager →
/// `managerMissing` (the UI opens the download page). Progress arrives as
/// [`ollama::AiInstallProgress`].
#[tauri::command]
#[specta::specta]
pub async fn ai_ollama_install(app: AppHandle) -> AiResult<AiInstallResult> {
    tauri::async_runtime::spawn_blocking(move || {
        let env = ProbeEnv::current();
        let Some(plan) = ollama::install_plan(env.os, &Managers::detect(&env)) else {
            return Ok(AiInstallResult {
                outcome: SetupInstallOutcome::ManagerMissing,
                manager: match env.os {
                    crate::onboarding::TargetOs::Windows => Some(SetupPackageManager::Winget),
                    crate::onboarding::TargetOs::Macos => Some(SetupPackageManager::Brew),
                    crate::onboarding::TargetOs::Linux => None,
                },
                command: None,
                exit_code: None,
                output: String::new(),
            });
        };
        let (_slot, cancel) = InstallSlot::take()?;
        tracing::info!(command = %plan.display(), "ollama install started");
        let mut emit = |p: ollama::AiInstallProgress| {
            if let Err(e) = p.emit(&app) {
                tracing::warn!(error = %e, "failed to emit ai install progress");
            }
        };
        Ok(ollama::run_install(
            &plan,
            &cancel,
            ollama::INSTALL_TIMEOUT,
            &mut emit,
        ))
    })
    .await
    .map_err(|e| AiError::internal(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn ai_ollama_install_cancel() -> bool {
    match INSTALLING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
    {
        Some(flag) => {
            flag.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

/// Starts `ollama serve` in the background (macOS: opens Ollama.app) and waits for it.
#[tauri::command]
#[specta::specta]
pub async fn ai_ollama_start(app: AppHandle, request_id: String) -> AiResult<OllamaStatus> {
    let (_running, cancel) = Running::start(&request_id)?;
    let base = ollama_url(&app)?;
    let http = http(&app);
    if ollama::version(&http, &base).await.is_none() {
        let env = tauri::async_runtime::spawn_blocking(move || {
            let env = ProbeEnv::current();
            let (cli, app_bundle) = ollama::locate(&env, &ollama::default_mac_apps());
            ollama::spawn_server(env.os, cli.as_deref(), app_bundle.as_deref()).map(|()| env)
        })
        .await
        .map_err(|e| AiError::internal(e.to_string()))??;
        ollama::wait_until_up(&http, &base, ollama::start_wait(), &cancel).await?;
        return Ok(ollama::status(&http, &base, &env).await);
    }
    let env = ProbeEnv::current();
    Ok(ollama::status(&http, &base, &env).await)
}

/// Downloads an Ollama model; progress arrives as [`AiPullProgress`].
#[tauri::command]
#[specta::specta]
pub async fn ai_ollama_pull(app: AppHandle, model: String, request_id: String) -> AiResult<()> {
    let (_running, cancel) = Running::start(&request_id)?;
    let base = ollama_url(&app)?;
    let handle = app.clone();
    ollama::pull(
        &http(&app),
        &base,
        model.trim(),
        &request_id,
        &cancel,
        |p| {
            if let Err(e) = p.emit(&handle) {
                tracing::warn!(error = %e, "failed to emit ai pull progress");
            }
        },
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn ai_ollama_delete(app: AppHandle, model: String) -> AiResult<()> {
    let base = ollama_url(&app)?;
    ollama::delete(&http(&app), &base, model.trim()).await
}
