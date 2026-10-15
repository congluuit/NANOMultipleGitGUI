//! Which hosted project (GitHub repository / GitLab project) each remote of an open repository
//! points at, and an API client of the account resolved for it (PLAN §5.8, Phase 4).
//!
//! Every provider feature (pull/merge requests, issues, CI status, forks) starts here so the
//! account choice is always the one the resolver uses for git itself (§4.3).

use nanomgg_accounts::AccountSecret;
use nanomgg_providers::{ProviderClient, ProviderFamily, SecretToken};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::accounts::commands::account_secret;
use crate::accounts::resolve_repo;
use crate::auth::AuthRuntime;
use crate::error::{AppError, AppResult};
use crate::repos::repo_root;
use crate::state::AppState;

/// A remote of an open repository that points at a hosting provider.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Type)]
#[serde(rename_all = "camelCase")]
pub struct HostedRemote {
    pub remote: String,
    /// Lower-cased host name, e.g. `github.com`.
    pub host: String,
    /// `owner/repo` (GitHub) or `group/sub/project` (GitLab).
    pub full_name: String,
    /// Account the resolver uses for this remote; `None` when unbound.
    pub account_id: Option<String>,
    /// Provider family of that account (`None` when unbound).
    pub family: Option<ProviderFamily>,
    /// Whether that account has a token stored (API calls need one).
    pub has_token: bool,
}

/// Remotes of `repo_id` that parse as hosted projects, `origin` first.
pub(crate) fn hosted_remotes(st: &AppState, repo_id: i64) -> AppResult<Vec<HostedRemote>> {
    let root = repo_root(st, repo_id)?;
    let resolution = resolve_repo(st, repo_id, &root)?;
    let mut out: Vec<HostedRemote> = resolution
        .remotes
        .iter()
        .filter_map(|r| {
            let parsed = r.parsed.as_ref()?;
            if parsed.host.is_empty() || !parsed.path.contains('/') {
                return None;
            }
            let account_id = r.resolved.as_ref().map(|x| x.account_id.clone());
            let family = account_id.as_deref().and_then(|id| family_of(st, id));
            let has_token = account_id.as_deref().is_some_and(|id| {
                matches!(account_secret(st, id, AccountSecret::Token), Ok(Some(_)))
            });
            Some(HostedRemote {
                remote: r.remote.clone(),
                host: parsed.host.clone(),
                full_name: parsed.path.clone(),
                account_id,
                family,
                has_token,
            })
        })
        .collect();
    out.sort_by_key(|r| {
        (
            r.remote != "origin",
            r.remote != "upstream",
            r.remote.clone(),
        )
    });
    Ok(out)
}

/// The hosted remote `remote` of `repo_id` (or the first one, `origin` preferred, when `None`).
pub(crate) fn hosted_remote(
    st: &AppState,
    repo_id: i64,
    remote: Option<&str>,
) -> AppResult<HostedRemote> {
    let all = hosted_remotes(st, repo_id)?;
    let found = match remote {
        Some(name) => all.into_iter().find(|r| r.remote == name),
        None => all.into_iter().next(),
    };
    found.ok_or_else(|| {
        AppError::Unsupported(match remote {
            Some(name) => format!("remote `{name}` does not point at GitHub or GitLab"),
            None => "this repository has no GitHub / GitLab remote".into(),
        })
    })
}

fn family_of(st: &AppState, account_id: &str) -> Option<ProviderFamily> {
    let account = st.storage.account(account_id).ok()??;
    let provider = st.storage.provider(&account.provider_id).ok()??;
    Some(crate::auth::service::family(&provider))
}

/// API client of an account: its provider endpoint + keychain token, on the shared HTTP client.
pub(crate) fn account_client(app: &AppHandle, account_id: &str) -> AppResult<ProviderClient> {
    let st = app.state::<AppState>();
    let account = st
        .storage
        .account(account_id)?
        .ok_or_else(|| AppError::InvalidInput(format!("account `{account_id}` not found")))?;
    let provider = st
        .storage
        .provider(&account.provider_id)?
        .ok_or_else(|| AppError::Internal("the account's provider is missing".into()))?;
    let endpoint = crate::auth::service::endpoint_for(&provider)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let token = account_secret(&st, account_id, AccountSecret::Token)?.ok_or_else(|| {
        AppError::Unsupported(format!(
            "no token stored for {} — sign in again in Manage accounts",
            account.username
        ))
    })?;
    let http = app.state::<AuthRuntime>().http.clone();
    Ok(ProviderClient::new(
        http,
        endpoint,
        SecretToken::from(token),
    ))
}

/// Client of the account resolved for a hosted remote (`override_account` = "only for this
/// action", PLAN §4.5), plus that remote.
pub(crate) fn remote_client(
    app: &AppHandle,
    repo_id: i64,
    remote: Option<&str>,
    override_account: Option<&str>,
) -> AppResult<(ProviderClient, HostedRemote)> {
    let st = app.state::<AppState>();
    let hosted = hosted_remote(&st, repo_id, remote)?;
    let account_id = override_account
        .map(str::to_owned)
        .or_else(|| hosted.account_id.clone())
        .ok_or_else(|| {
            AppError::Unsupported(format!(
                "remote `{}` is not bound to an account — choose one with the account pill",
                hosted.remote
            ))
        })?;
    Ok((account_client(app, &account_id)?, hosted))
}

/// Hosted remotes of an open repository with the account each one uses.
#[tauri::command]
#[specta::specta]
pub async fn hosted_remote_list(app: AppHandle, repo_id: i64) -> AppResult<Vec<HostedRemote>> {
    tauri::async_runtime::spawn_blocking(move || hosted_remotes(&app.state::<AppState>(), repo_id))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}
