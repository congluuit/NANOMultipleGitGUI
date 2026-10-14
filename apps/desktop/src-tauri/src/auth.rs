//! Sign-in and token lifecycle (PLAN §4.2): GitHub/GitLab device flow, GitLab Authorization
//! Code + PKCE over a fixed loopback port, personal access tokens, token health and the
//! background refresh of GitLab OAuth tokens.
//!
//! The commands are thin async wrappers around the functions in [`service`], which take the
//! [`AppState`] and the [`AuthRuntime`] directly so tests can drive them against a wiremock
//! server without the Tauri runtime.
//!
//! # Flows
//! - Device flow: `auth_device_start` → show the code (`DevicePrompt`) → `auth_device_wait`
//!   resolves with the new/updated [`Account`] (or `auth_cancel`).
//! - PKCE (GitLab): `auth_pkce_start` binds `127.0.0.1:<loopback port>` and returns the URL to
//!   open; `auth_pkce_wait` finishes the exchange. The port is configurable per provider
//!   (`auth_config_set`, default [`DEFAULT_LOOPBACK_PORT`]) because GitLab compares the redirect
//!   URI exactly: the user registers `http://127.0.0.1:<port>/callback` in the application.
//! - PAT: `auth_add_pat` / `auth_update_token` validate the token and store it.
//!
//! Every successful login upserts the account (unique provider + username), stores the token(s)
//! in the keychain through [`account_store_tokens`], and gives the account a default identity
//! from the provider's primary verified email when it has none.
//!
//! OAuth apps are public clients: the client id comes from the provider row (editable in the
//! UI) or, for github.com / gitlab.com, from `NANOMGG_GITHUB_OAUTH_CLIENT_ID` /
//! `NANOMGG_GITLAB_OAUTH_CLIENT_ID` at build time. Without one the commands fail with
//! [`AuthCommandError::MissingClientId`] and the UI offers a PAT instead.
//!
//! [`account_store_tokens`]: crate::accounts::commands::account_store_tokens

mod refresh;
pub mod service;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use nanomgg_accounts::{Account, Provider};
use nanomgg_auth::{
    AuthError, CancellationToken, DeviceAuthorization, DeviceFlowConfig, DevicePrompt,
    LoopbackLogin, MissingScope, TokenKind,
};
use nanomgg_providers::{HttpConfig, ProviderEmail, build_http_client};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use zeroize::Zeroizing;

use crate::error::AppError;
use crate::state::AppState;

/// Loopback port used for the GitLab PKCE redirect when the provider has none configured.
pub const DEFAULT_LOOPBACK_PORT: u16 = 47821;
/// Background refresh period.
pub const REFRESH_PERIOD: Duration = Duration::from_secs(5 * 60);
/// Refresh GitLab OAuth tokens expiring within this many seconds.
pub const REFRESH_MARGIN_SECS: i64 = 15 * 60;
/// Pending flows older than this are dropped (device codes and PKCE logins expire sooner).
const FLOW_TTL: Duration = Duration::from_secs(30 * 60);

/// Errors of the auth commands. Serialized as `{ kind, message, ... }` (like [`AppError`]) so
/// the generic `errorMessage()` helper still works; the UI branches on `kind`.
#[derive(Debug, thiserror::Error, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AuthCommandError {
    /// The provider has no OAuth client id (the project has not registered its OAuth apps yet,
    /// or a self-hosted instance needs its own): sign in with a PAT instead.
    #[error("{message}")]
    #[serde(rename_all = "camelCase")]
    MissingClientId {
        message: String,
        provider_id: String,
        /// Token creation page with the needed scopes pre-selected.
        pat_url: String,
        pat_scopes: Vec<String>,
    },
    /// The flow is not supported for this provider (e.g. PKCE on GitHub).
    #[error("{message}")]
    #[serde(rename_all = "camelCase")]
    Unsupported { message: String },
    /// A sign-in / token error from `nanomgg_auth` (denied, expired, cancelled, provider...).
    #[error("{message}")]
    #[serde(rename_all = "camelCase")]
    Auth { message: String, error: AuthError },
    /// The flow id is unknown (finished, cancelled or expired).
    #[error("{message}")]
    #[serde(rename_all = "camelCase")]
    UnknownFlow { message: String },
    /// Re-authentication produced a token of another user.
    #[error("{message}")]
    #[serde(rename_all = "camelCase")]
    WrongAccount {
        message: String,
        expected: String,
        actual: String,
    },
    /// Storage / keychain / validation failure.
    #[error("{message}")]
    #[serde(rename_all = "camelCase")]
    App { message: String, error: AppError },
}

impl From<AuthError> for AuthCommandError {
    fn from(error: AuthError) -> Self {
        Self::Auth {
            message: nanomgg_logging::redact(&error.to_string()).into_owned(),
            error,
        }
    }
}

impl From<nanomgg_providers::ProviderError> for AuthCommandError {
    fn from(error: nanomgg_providers::ProviderError) -> Self {
        AuthError::from(error).into()
    }
}

impl From<AppError> for AuthCommandError {
    fn from(error: AppError) -> Self {
        Self::App {
            message: error.to_string(),
            error,
        }
    }
}

impl From<nanomgg_storage::StorageError> for AuthCommandError {
    fn from(error: nanomgg_storage::StorageError) -> Self {
        crate::accounts::storage_err(error).into()
    }
}

pub type AuthResult<T> = Result<T, AuthCommandError>;

/// Answer of `auth_device_start`.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStart {
    pub flow_id: String,
    pub prompt: DevicePrompt,
}

/// Answer of `auth_pkce_start`.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PkceStart {
    pub flow_id: String,
    /// Open in the system browser.
    pub authorize_url: String,
    /// `http://127.0.0.1:<port>/callback` — must be registered in the GitLab application.
    pub redirect_uri: String,
}

/// Sign-in options of a provider for the add-account wizard and the Providers tab.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAuthConfig {
    pub provider_id: String,
    /// A client id is available (provider row or built-in).
    pub has_client_id: bool,
    /// The client id comes from the app build, not from the provider row.
    pub builtin_client_id: bool,
    pub device_flow: bool,
    pub pkce: bool,
    pub loopback_port: u16,
    pub redirect_uri: String,
    /// Personal access token page with the scopes pre-selected.
    pub pat_url: String,
    /// Scopes to tick when creating a PAT.
    pub pat_scopes: Vec<String>,
    /// Where to register an OAuth application on this instance.
    pub oauth_apps_url: String,
}

/// Token health of an account (account card badge).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountHealth {
    pub account_id: String,
    /// A token is stored in the keychain (false for SSH-only accounts).
    pub has_token: bool,
    /// The app refreshes this token itself (GitLab OAuth with a refresh token).
    pub auto_refresh: bool,
    /// False for fine-grained PATs: permissions only show up as 403s.
    pub scopes_known: bool,
    /// Scopes missing for core features (git over HTTPS, repo list, emails, profile).
    pub missing_scopes: Vec<MissingScope>,
    /// Scopes missing for optional features (SSH key upload, workflow edits).
    pub optional_missing: Vec<MissingScope>,
    /// Milliseconds since the epoch.
    pub expires_at: Option<i64>,
    pub expired: bool,
    pub expires_soon: bool,
    pub days_left: Option<i64>,
    /// The token was rejected or the refresh token revoked: "Re-authenticate".
    pub needs_sign_in: bool,
    pub last_error: Option<String>,
    pub token_kind: Option<TokenKind>,
}

/// Per-account auth status kept in the settings table (`auth.status.<account id>`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    #[serde(default)]
    pub needs_sign_in: bool,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub token_kind: Option<TokenKind>,
    /// `None` = unknown (older accounts): assume known when scopes are stored.
    #[serde(default)]
    pub scopes_known: Option<bool>,
    /// Unix milliseconds of the last successful validation/refresh.
    #[serde(default)]
    pub checked_at: Option<i64>,
}

enum PendingFlow {
    Device {
        provider: Provider,
        config: DeviceFlowConfig,
        auth: DeviceAuthorization,
    },
    Pkce {
        provider: Provider,
        login: LoopbackLogin,
    },
}

struct FlowEntry {
    cancel: CancellationToken,
    started: Instant,
    /// Taken by the `*_wait` command; dropping it (cancel) closes the PKCE listener.
    pending: Option<PendingFlow>,
}

/// Shared HTTP client + running sign-in flows. Managed Tauri state.
pub struct AuthRuntime {
    pub http: reqwest::Client,
    flows: Mutex<HashMap<String, FlowEntry>>,
    /// Serializes token refreshes: GitLab rotates refresh tokens, so two concurrent refreshes
    /// with the same token would make the second one fail with `invalid_grant`.
    refresh_lock: tokio::sync::Mutex<()>,
    /// Lower bound of the device-flow polling interval (tests use zero).
    pub min_poll_interval: Duration,
}

impl AuthRuntime {
    pub fn new() -> Result<Self, nanomgg_providers::ProviderError> {
        Ok(Self::with_client(
            build_http_client(&HttpConfig::default())?,
        ))
    }

    pub fn with_client(http: reqwest::Client) -> Self {
        Self {
            http,
            flows: Mutex::default(),
            refresh_lock: tokio::sync::Mutex::new(()),
            min_poll_interval: Duration::from_secs(1),
        }
    }

    fn flows(&self) -> std::sync::MutexGuard<'_, HashMap<String, FlowEntry>> {
        self.flows.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn register(&self, pending: PendingFlow) -> String {
        let id = nanomgg_accounts::new_id("flow");
        let mut flows = self.flows();
        flows.retain(|_, f| f.started.elapsed() < FLOW_TTL);
        flows.insert(
            id.clone(),
            FlowEntry {
                cancel: CancellationToken::new(),
                started: Instant::now(),
                pending: Some(pending),
            },
        );
        id
    }

    /// Takes the pending flow for `*_wait`; the entry stays so `auth_cancel` can fire.
    fn take(&self, flow_id: &str) -> AuthResult<(PendingFlow, CancellationToken)> {
        let mut flows = self.flows();
        let entry = flows.get_mut(flow_id).ok_or_else(unknown_flow)?;
        let pending = entry.pending.take().ok_or_else(unknown_flow)?;
        Ok((pending, entry.cancel.clone()))
    }

    fn finish(&self, flow_id: &str) {
        self.flows().remove(flow_id);
    }

    /// Cancels a running flow. `false` when it is unknown (already finished).
    pub fn cancel(&self, flow_id: &str) -> bool {
        match self.flows().remove(flow_id) {
            Some(entry) => {
                entry.cancel.cancel();
                true
            }
            None => false,
        }
    }

    /// Number of flows still registered (tests).
    #[cfg(test)]
    pub fn flow_count(&self) -> usize {
        self.flows().len()
    }
}

fn unknown_flow() -> AuthCommandError {
    AuthCommandError::UnknownFlow {
        message: "this sign-in attempt is no longer running".into(),
    }
}

/// Registers the runtime as managed state and starts the background refresh task. Call once
/// from `setup`, after [`AppState`] is managed.
pub fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    app.manage(AuthRuntime::new()?);
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        refresh::run(handle).await;
    });
    Ok(())
}

// ----- commands ------------------------------------------------------------------------------

/// Starts a device flow (GitHub, GitHub Enterprise, GitLab where enabled). `scopes = None` →
/// the defaults (`repo read:user user:email` / `read_user read_api read_repository
/// write_repository`).
#[tauri::command]
#[specta::specta]
pub async fn auth_device_start(
    app: AppHandle,
    provider_id: String,
    scopes: Option<Vec<String>>,
) -> AuthResult<DeviceStart> {
    let st = app.state::<AppState>();
    let rt = app.state::<AuthRuntime>();
    service::device_start(&st, &rt, &provider_id, scopes).await
}

/// Waits until the user approves the device code; resolves with the signed-in account.
#[tauri::command]
#[specta::specta]
pub async fn auth_device_wait(app: AppHandle, flow_id: String) -> AuthResult<Account> {
    let st = app.state::<AppState>();
    let rt = app.state::<AuthRuntime>();
    service::flow_wait(&st, &rt, &flow_id).await
}

/// Starts a GitLab PKCE login on the provider's loopback port.
#[tauri::command]
#[specta::specta]
pub async fn auth_pkce_start(
    app: AppHandle,
    provider_id: String,
    scopes: Option<Vec<String>>,
) -> AuthResult<PkceStart> {
    let st = app.state::<AppState>();
    let rt = app.state::<AuthRuntime>();
    service::pkce_start(&st, &rt, &provider_id, scopes).await
}

/// Waits for the browser redirect of a PKCE login; resolves with the signed-in account.
#[tauri::command]
#[specta::specta]
pub async fn auth_pkce_wait(app: AppHandle, flow_id: String) -> AuthResult<Account> {
    let st = app.state::<AppState>();
    let rt = app.state::<AuthRuntime>();
    service::flow_wait(&st, &rt, &flow_id).await
}

/// Cancels a device/PKCE flow (the pending `*_wait` fails with `cancelled`).
#[tauri::command]
#[specta::specta]
pub async fn auth_cancel(app: AppHandle, flow_id: String) -> bool {
    app.state::<AuthRuntime>().cancel(&flow_id)
}

/// Adds (or updates, same provider + username) an account from a personal access token.
#[tauri::command]
#[specta::specta]
pub async fn auth_add_pat(
    app: AppHandle,
    provider_id: String,
    token: String,
    label: Option<String>,
) -> AuthResult<Account> {
    let token = Zeroizing::new(token);
    let st = app.state::<AppState>();
    let rt = app.state::<AuthRuntime>();
    service::add_pat(&st, &rt, &provider_id, token, label).await
}

/// Replaces the token of an existing account (re-authentication with a PAT). The token must
/// belong to the same user.
#[tauri::command]
#[specta::specta]
pub async fn auth_update_token(
    app: AppHandle,
    account_id: String,
    token: String,
) -> AuthResult<Account> {
    let token = Zeroizing::new(token);
    let st = app.state::<AppState>();
    let rt = app.state::<AuthRuntime>();
    service::update_token(&st, &rt, &account_id, token).await
}

/// Sign-in options of a provider: client id availability, loopback port / redirect URI, PAT
/// page and scopes.
#[tauri::command]
#[specta::specta]
pub async fn auth_config(app: AppHandle, provider_id: String) -> AuthResult<ProviderAuthConfig> {
    let st = app.state::<AppState>();
    service::auth_config(&st, &provider_id)
}

/// Sets the loopback port of a provider's PKCE redirect (1024..=65535).
#[tauri::command]
#[specta::specta]
pub async fn auth_config_set(
    app: AppHandle,
    provider_id: String,
    loopback_port: u16,
) -> AuthResult<ProviderAuthConfig> {
    let st = app.state::<AppState>();
    service::set_loopback_port(&st, &provider_id, loopback_port)?;
    service::auth_config(&st, &provider_id)
}

/// Token health (no network): missing scopes, expiry, "needs sign-in".
#[tauri::command]
#[specta::specta]
pub async fn account_health(app: AppHandle, account_id: String) -> AuthResult<AccountHealth> {
    let st = app.state::<AppState>();
    service::health(&st, &account_id)
}

/// Health of every account (account manager list).
#[tauri::command]
#[specta::specta]
pub async fn account_health_all(app: AppHandle) -> AuthResult<Vec<AccountHealth>> {
    let st = app.state::<AppState>();
    service::health_all(&st)
}

/// Refreshes a GitLab OAuth token now, or re-validates any other token (updates scopes,
/// expiry and profile; a rejected token marks the account "needs sign-in").
#[tauri::command]
#[specta::specta]
pub async fn account_refresh(app: AppHandle, account_id: String) -> AuthResult<AccountHealth> {
    let st = app.state::<AppState>();
    let rt = app.state::<AuthRuntime>();
    service::refresh_account(&st, &rt, &account_id).await
}

/// Email addresses of the account on the provider (identity suggestions, Commit Guard).
#[tauri::command]
#[specta::specta]
pub async fn account_emails(app: AppHandle, account_id: String) -> AuthResult<Vec<ProviderEmail>> {
    let st = app.state::<AppState>();
    let rt = app.state::<AuthRuntime>();
    service::emails(&st, &rt, &account_id).await
}

/// Default account color palette (`crates/accounts/src/colors.rs`).
#[tauri::command]
#[specta::specta]
pub async fn account_colors() -> Vec<String> {
    nanomgg_accounts::colors::ACCOUNT_COLORS
        .iter()
        .map(|c| (*c).to_owned())
        .collect()
}
