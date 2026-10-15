//! Accounts signed in with the `gh` / `glab` CLI (PLAN 3B.1).
//!
//! [`cli_accounts_scan`] reads gh's `hosts.yml` and glab's `config.yml` (names only, never the
//! token) and tells which accounts are already in the app (same provider host + user name,
//! case-insensitive) and whether the CLI executable is installed. It runs at startup and from
//! "Quét lại".
//!
//! [`cli_accounts_import`] runs only after the user ticked accounts and agreed: it asks the CLI
//! for each token (`nanomgg_import::cli_token`), validates it against the provider like a PAT
//! (owner must be the expected user; scopes, expiry), creates the GitHub Enterprise / GitLab
//! self-hosted provider when the host is new, and stores the account with
//! [`AuthMethod::ImportedCli`] and the token in the app keychain. The CLI's config is never
//! modified. Each account gets its own [`CliImportStatus`] so the UI can say how to fix it.
//! Importing an account that was already imported copies the CLI's current token again (the
//! fix after `gh auth refresh -s workflow`); accounts signed in another way are left alone.
//!
//! Setting [`KEY_DISMISSED`]: keys of new CLI accounts the user chose to ignore in the startup
//! notice; the notice comes back when a CLI account outside that set appears.

use std::path::{Path, PathBuf};
use std::time::Duration;

use nanomgg_accounts::{AuthMethod, Provider, ProviderKind};
use nanomgg_auth::{AuthError, Feature, token_health, validate_token};
use nanomgg_import::cli_hosts::{CliHostsScan, CliTool, SuggestedAccount, TokenStorage};
use nanomgg_import::cli_token::{self, CliTokenError, TokenRequest};
use nanomgg_import::{
    ImportWarning, ScanEnv, gh_hosts_candidates, glab_config_candidates, scan_gh_hosts,
    scan_glab_config,
};
use nanomgg_providers::{ProviderError, SecretToken, now_unix};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::accounts::commands::{ProviderInput, provider_save_impl};
use crate::accounts::storage_err;
use crate::auth::{AuthCommandError, AuthRuntime, service};
use crate::error::{AppError, AppResult};
use crate::import::{ImportProviderKind, real_locations};
use crate::state::AppState;

/// Setting: keys ([`CliAccountEntry::key`]) the user dismissed in the startup notice.
pub const KEY_DISMISSED: &str = "cliSync.dismissed";

/// Scopes worth reporting after an import: core features plus workflow edits (GitHub refuses
/// pushes touching `.github/workflows/` without it).
const CHECKED_FEATURES: [Feature; 4] = [
    Feature::GitHttps,
    Feature::ListRepositories,
    Feature::ReadEmails,
    Feature::EditWorkflows,
];

// ----- DTOs ----------------------------------------------------------------------------------

/// One account found in a CLI config.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CliAccountEntry {
    /// Stable id `tool|host|user` (lower case) used for selection and "dismiss".
    pub key: String,
    pub tool: CliTool,
    pub provider: ImportProviderKind,
    pub host: String,
    /// glab `api_host` when it differs from `host`.
    pub api_host: Option<String>,
    /// `None` when the config does not record the user (the CLI's active account is used).
    pub username: Option<String>,
    /// The CLI's active account for this host.
    pub active: bool,
    pub token_storage: TokenStorage,
    /// App account with the same provider host + user name.
    pub existing_account_id: Option<String>,
    /// The existing account was itself imported from the CLI: importing again copies the
    /// CLI's current token (after `gh auth refresh -s …`).
    pub can_update: bool,
    /// Provider row whose host matches (`None` → created on import).
    pub provider_id: Option<String>,
}

/// Whether a CLI is installed (its token can only be read through the executable).
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CliToolStatus {
    pub tool: CliTool,
    pub installed: bool,
    /// Executable found.
    pub path: Option<String>,
    /// Config file that was read.
    pub config_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CliAccountsScan {
    pub accounts: Vec<CliAccountEntry>,
    pub tools: Vec<CliToolStatus>,
    pub warnings: Vec<ImportWarning>,
    /// Number of accounts not in the app yet.
    pub new_count: u32,
    /// Some new account is outside the dismissed set: show the startup notice.
    pub should_notify: bool,
}

/// An account the user ticked.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CliAccountRef {
    pub tool: CliTool,
    pub host: String,
    #[serde(default)]
    #[specta(optional)]
    pub username: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CliImportStatus {
    /// Added with every checked scope.
    Added,
    /// Added, but some scopes are missing (`missingScopes`, `fixCommand`).
    MissingScopes,
    /// The provider rejected the token (revoked / expired): sign in to the CLI again.
    InvalidToken,
    /// The token belongs to another user than the CLI config says.
    WrongUser,
    /// The CLI did not give a token (signed out, old gh with several accounts, timeout).
    NoToken,
    /// gh / glab is not installed: its token cannot be read.
    CliMissing,
    /// Already in the app (signed in another way); nothing changed.
    AlreadyExists,
    /// The account is no longer in the CLI config (scan again).
    NotFound,
    /// Network / storage error (`message`).
    Failed,
}

/// Result of one ticked account.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CliImportItem {
    pub key: String,
    pub tool: CliTool,
    pub host: String,
    /// User name from the CLI config, or the token owner once known.
    pub username: Option<String>,
    pub status: CliImportStatus,
    /// Created / existing account.
    pub account_id: Option<String>,
    /// Scope names to add (one per missing feature).
    pub missing_scopes: Vec<String>,
    /// Command that adds the missing scopes (gh only).
    pub fix_command: Option<String>,
    /// Owner of the token for [`CliImportStatus::WrongUser`].
    pub actual_username: Option<String>,
    /// Technical detail (English, redacted).
    pub message: Option<String>,
}

// ----- scan ----------------------------------------------------------------------------------

/// Where to look for configs and executables (tests point everything at temp folders).
#[derive(Debug, Clone, Default)]
pub struct CliEnv {
    pub home: PathBuf,
    pub appdata: Option<PathBuf>,
    pub scan: ScanEnv,
    pub gh: Option<PathBuf>,
    pub glab: Option<PathBuf>,
    pub timeout: Duration,
}

impl CliEnv {
    fn program(&self, tool: CliTool) -> Option<&Path> {
        match tool {
            CliTool::Gh => self.gh.as_deref(),
            CliTool::Glab => self.glab.as_deref(),
        }
    }
}

fn tool_str(tool: CliTool) -> &'static str {
    tool.program_name()
}

pub fn entry_key(tool: CliTool, host: &str, username: Option<&str>) -> String {
    format!(
        "{}|{}|{}",
        tool_str(tool),
        host.trim().to_ascii_lowercase(),
        username.unwrap_or("").trim().to_ascii_lowercase()
    )
}

fn read_config(
    candidates: Vec<PathBuf>,
    scanner: fn(&Path) -> CliHostsScan,
) -> (CliHostsScan, Option<PathBuf>) {
    match candidates.into_iter().find(|p| p.is_file()) {
        Some(path) => (scanner(&path), Some(path)),
        None => (CliHostsScan::default(), None),
    }
}

fn provider_for_host<'a>(providers: &'a [Provider], host: &str) -> Option<&'a Provider> {
    let host = host.trim().trim_end_matches('/').to_ascii_lowercase();
    providers
        .iter()
        .find(|p| p.host().is_some_and(|h| h.eq_ignore_ascii_case(&host)))
}

fn entry(
    a: SuggestedAccount,
    providers: &[Provider],
    accounts: &[nanomgg_accounts::Account],
) -> CliAccountEntry {
    let provider = provider_for_host(providers, &a.host);
    let existing = provider.and_then(|p| {
        let user = a.username.as_deref()?;
        accounts
            .iter()
            .find(|acc| acc.provider_id == p.id && acc.username.eq_ignore_ascii_case(user))
    });
    CliAccountEntry {
        key: entry_key(a.tool, &a.host, a.username.as_deref()),
        tool: a.tool,
        provider: a.provider.into(),
        host: a.host,
        api_host: a.api_host,
        username: a.username,
        active: a.active,
        token_storage: a.token_storage,
        existing_account_id: existing.map(|acc| acc.id.clone()),
        can_update: existing.is_some_and(|acc| acc.auth_method == AuthMethod::ImportedCli),
        provider_id: provider.map(|p| p.id.clone()),
    }
}

fn dismissed(st: &AppState) -> Vec<String> {
    st.storage
        .get_setting::<Vec<String>>(KEY_DISMISSED)
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(crate) fn scan_impl(st: &AppState, env: &CliEnv) -> AppResult<CliAccountsScan> {
    let (gh, gh_file) = read_config(
        gh_hosts_candidates(&env.home, env.appdata.as_deref(), &env.scan),
        scan_gh_hosts,
    );
    let (glab, glab_file) = read_config(
        glab_config_candidates(&env.home, env.appdata.as_deref(), &env.scan),
        scan_glab_config,
    );
    let providers = st.storage.providers().map_err(storage_err)?;
    let accounts = st.storage.accounts().map_err(storage_err)?;
    let mut warnings = gh.warnings;
    warnings.extend(glab.warnings);
    let list: Vec<CliAccountEntry> = gh
        .accounts
        .into_iter()
        .chain(glab.accounts)
        .map(|a| entry(a, &providers, &accounts))
        .collect();
    let new: Vec<&CliAccountEntry> = list
        .iter()
        .filter(|a| a.existing_account_id.is_none())
        .collect();
    let ignored = dismissed(st);
    let should_notify = new.iter().any(|a| !ignored.contains(&a.key));
    let tool = |tool: CliTool, file: Option<PathBuf>| CliToolStatus {
        tool,
        installed: env.program(tool).is_some(),
        path: env.program(tool).map(|p| p.to_string_lossy().into_owned()),
        config_file: file.map(|f| f.to_string_lossy().into_owned()),
    };
    Ok(CliAccountsScan {
        new_count: new.len() as u32,
        should_notify,
        accounts: list,
        tools: vec![tool(CliTool::Gh, gh_file), tool(CliTool::Glab, glab_file)],
        warnings,
    })
}

/// Adds `keys` to the dismissed set (the notice stays hidden until another account appears).
pub(crate) fn dismiss_impl(st: &AppState, keys: Vec<String>) -> AppResult<()> {
    let mut list = dismissed(st);
    for k in keys {
        let k = k.trim().to_ascii_lowercase();
        if !k.is_empty() && !list.contains(&k) {
            list.push(k);
        }
    }
    list.sort();
    st.storage.set_setting(KEY_DISMISSED, &list)?;
    Ok(())
}

// ----- import --------------------------------------------------------------------------------

fn item(e: &CliAccountEntry, status: CliImportStatus) -> CliImportItem {
    CliImportItem {
        key: e.key.clone(),
        tool: e.tool,
        host: e.host.clone(),
        username: e.username.clone(),
        status,
        account_id: e.existing_account_id.clone(),
        missing_scopes: Vec::new(),
        fix_command: None,
        actual_username: None,
        message: None,
    }
}

fn with_message(mut it: CliImportItem, message: impl AsRef<str>) -> CliImportItem {
    it.message = Some(nanomgg_logging::redact(message.as_ref()).into_owned());
    it
}

/// Unsaved provider row for a host the app does not know yet.
fn new_provider_input(e: &CliAccountEntry) -> ProviderInput {
    let (kind, api) = match e.tool {
        CliTool::Gh => (ProviderKind::GithubEnterprise, None),
        CliTool::Glab => (
            ProviderKind::GitlabSelfHosted,
            e.api_host
                .as_ref()
                .map(|a| format!("https://{}/api/v4", a.trim_end_matches('/'))),
        ),
    };
    ProviderInput {
        id: None,
        kind,
        name: e.host.clone(),
        base_url: format!("https://{}", e.host.trim_end_matches('/')),
        api_url: api,
        oauth_client_id: None,
        large_file_warn_mb: None,
        large_file_block_mb: None,
    }
}

/// The provider a token is validated against: the matching row, or an in-memory draft that
/// is only saved once the token proved valid.
fn target_provider(
    st: &AppState,
    e: &CliAccountEntry,
) -> AppResult<(Provider, Option<ProviderInput>)> {
    // Looked up again: an earlier account of this run may have created the provider.
    let providers = st.storage.providers().map_err(storage_err)?;
    if let Some(p) = provider_for_host(&providers, &e.host) {
        return Ok((p.clone(), None));
    }
    let input = new_provider_input(e);
    let base_url = input.base_url.clone();
    let draft = Provider {
        id: String::new(),
        kind: input.kind,
        name: input.name.clone(),
        api_url: input
            .api_url
            .clone()
            .unwrap_or_else(|| input.kind.default_api_url(&base_url)),
        base_url,
        oauth_client_id: None,
        large_file_warn_mb: 1,
        large_file_block_mb: None,
        builtin: false,
    };
    Ok((draft, Some(input)))
}

/// Scope names missing from `scopes` (`None` = unknown, e.g. fine-grained PAT → nothing).
fn missing_scopes(provider: &Provider, scopes: Option<&[String]>) -> Vec<String> {
    let health = token_health(
        service::family(provider),
        scopes,
        None,
        &CHECKED_FEATURES,
        now_unix(),
    );
    let mut out: Vec<String> = Vec::new();
    for m in health.missing {
        if let Some(s) = m.any_of.first()
            && !out.contains(s)
        {
            out.push(s.clone());
        }
    }
    out
}

/// `gh auth refresh` acts on the active account: switch first for the others.
fn gh_fix_command(e: &CliAccountEntry, user: &str, scopes: &[String]) -> String {
    let refresh = format!("gh auth refresh -h {} -s {}", e.host, scopes.join(","));
    if e.active {
        refresh
    } else {
        format!("gh auth switch -h {} -u {user} && {refresh}", e.host)
    }
}

fn is_unauthorized(e: &AuthError) -> bool {
    matches!(
        e,
        AuthError::TokenRevoked
            | AuthError::Provider {
                error: ProviderError::Unauthorized { .. }
            }
    )
}

async fn fetch_token(
    env: &CliEnv,
    e: &CliAccountEntry,
    program: PathBuf,
    single: bool,
) -> Result<zeroize::Zeroizing<String>, CliTokenError> {
    let (tool, host, user, timeout) = (e.tool, e.host.clone(), e.username.clone(), env.timeout);
    tauri::async_runtime::spawn_blocking(move || {
        cli_token::fetch_token(
            tool,
            &program,
            TokenRequest {
                host: &host,
                username: user.as_deref(),
                single_account_on_host: single,
            },
            timeout,
        )
    })
    .await
    .unwrap_or_else(|e| {
        Err(CliTokenError::Spawn {
            detail: e.to_string(),
        })
    })
}

async fn import_one(
    st: &AppState,
    rt: &AuthRuntime,
    env: &CliEnv,
    scan: &CliAccountsScan,
    e: &CliAccountEntry,
) -> CliImportItem {
    if e.existing_account_id.is_some() && !e.can_update {
        return item(e, CliImportStatus::AlreadyExists);
    }
    let Some(program) = env.program(e.tool).map(Path::to_path_buf) else {
        return item(e, CliImportStatus::CliMissing);
    };
    let single = scan
        .accounts
        .iter()
        .filter(|a| a.tool == e.tool && a.host.eq_ignore_ascii_case(&e.host))
        .count()
        == 1;
    let token = match fetch_token(env, e, program, single).await {
        Ok(t) => SecretToken::from(t),
        Err(err) => return with_message(item(e, CliImportStatus::NoToken), err.to_string()),
    };
    let (provider, draft) = match target_provider(st, e) {
        Ok(v) => v,
        Err(err) => return with_message(item(e, CliImportStatus::Failed), err.to_string()),
    };
    let info = match service::endpoint_for(&provider) {
        Ok(endpoint) => validate_token(&rt.http, &endpoint, &token).await,
        Err(err) => return with_message(item(e, CliImportStatus::Failed), err.to_string()),
    };
    let info = match info {
        Ok(info) => info,
        Err(err) => {
            let status = if is_unauthorized(&err) {
                CliImportStatus::InvalidToken
            } else {
                CliImportStatus::Failed
            };
            return with_message(item(e, status), err.to_string());
        }
    };
    let owner = info.user.username.clone();
    if let Some(expected) = &e.username
        && !expected.eq_ignore_ascii_case(&owner)
    {
        let mut it = item(e, CliImportStatus::WrongUser);
        it.actual_username = Some(owner.clone());
        return with_message(
            it,
            format!("the CLI token belongs to `{owner}`, not `{expected}`"),
        );
    }
    let provider = match draft {
        None => {
            // The owner may already be in the app when the config had no user name; only an
            // account imported from the CLI gets its token replaced.
            if let Ok(accounts) = st.storage.accounts()
                && let Some(acc) = accounts.iter().find(|a| {
                    a.provider_id == provider.id && a.username.eq_ignore_ascii_case(&owner)
                })
                && acc.auth_method != AuthMethod::ImportedCli
            {
                let mut it = item(e, CliImportStatus::AlreadyExists);
                it.username = Some(acc.username.clone());
                it.account_id = Some(acc.id.clone());
                return it;
            }
            provider
        }
        Some(input) => match provider_save_impl(st, input) {
            Ok(p) => p,
            Err(err) => return with_message(item(e, CliImportStatus::Failed), err.to_string()),
        },
    };
    let account = match service::save_imported(st, rt, &provider, &info, &token).await {
        Ok(a) => a,
        Err(err) => {
            let msg = match &err {
                AuthCommandError::App { message, .. } => message.clone(),
                other => other.to_string(),
            };
            return with_message(item(e, CliImportStatus::Failed), msg);
        }
    };
    let missing = missing_scopes(&provider, info.scopes.as_deref());
    let mut it = item(
        e,
        if missing.is_empty() {
            CliImportStatus::Added
        } else {
            CliImportStatus::MissingScopes
        },
    );
    it.username = Some(account.username.clone());
    it.account_id = Some(account.id.clone());
    if !missing.is_empty() && e.tool == CliTool::Gh {
        it.fix_command = Some(gh_fix_command(e, &account.username, &missing));
    }
    it.missing_scopes = missing;
    it
}

/// Imports the ticked accounts one by one; one failure does not stop the others.
pub(crate) async fn import_impl(
    st: &AppState,
    rt: &AuthRuntime,
    env: &CliEnv,
    refs: Vec<CliAccountRef>,
) -> AppResult<Vec<CliImportItem>> {
    let scan = scan_impl(st, env)?;
    let mut items = Vec::with_capacity(refs.len());
    for r in refs {
        let key = entry_key(r.tool, &r.host, r.username.as_deref());
        let Some(e) = scan.accounts.iter().find(|a| a.key == key) else {
            items.push(CliImportItem {
                key,
                tool: r.tool,
                host: r.host,
                username: r.username,
                status: CliImportStatus::NotFound,
                account_id: None,
                missing_scopes: Vec::new(),
                fix_command: None,
                actual_username: None,
                message: None,
            });
            continue;
        };
        let it = import_one(st, rt, env, &scan, e).await;
        tracing::info!(tool = tool_str(it.tool), host = %it.host, status = ?it.status, "CLI account import");
        items.push(it);
    }
    Ok(items)
}

// ----- commands ------------------------------------------------------------------------------

fn real_env(app: &AppHandle) -> AppResult<CliEnv> {
    let (home, appdata) = real_locations(app)?;
    Ok(CliEnv {
        gh: cli_token::locate_cli(CliTool::Gh, Some(&home)),
        glab: cli_token::locate_cli(CliTool::Glab, Some(&home)),
        home,
        appdata,
        scan: ScanEnv::from_process(),
        timeout: cli_token::DEFAULT_TIMEOUT,
    })
}

/// Lists gh / glab accounts on this machine (config files only, no token). Read-only.
#[tauri::command]
#[specta::specta]
pub async fn cli_accounts_scan(app: AppHandle) -> AppResult<CliAccountsScan> {
    tauri::async_runtime::spawn_blocking(move || {
        let env = real_env(&app)?;
        scan_impl(&app.state::<AppState>(), &env)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Copies the tokens of the ticked accounts from gh / glab into the app (user consent given
/// in the UI). Never modifies the CLI configuration.
#[tauri::command]
#[specta::specta]
pub async fn cli_accounts_import(
    app: AppHandle,
    accounts: Vec<CliAccountRef>,
) -> AppResult<Vec<CliImportItem>> {
    let env = {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || real_env(&app))
            .await
            .map_err(|e| AppError::Internal(e.to_string()))??
    };
    let st = app.state::<AppState>();
    let rt = app.state::<AuthRuntime>();
    import_impl(&st, &rt, &env, accounts).await
}

/// "Bỏ qua" in the startup notice: hide it until another CLI account appears.
#[tauri::command]
#[specta::specta]
pub async fn cli_accounts_dismiss(app: AppHandle, keys: Vec<String>) -> AppResult<()> {
    dismiss_impl(&app.state::<AppState>(), keys)
}

#[cfg(test)]
mod tests;
