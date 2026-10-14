//! IPC commands for providers, accounts, identities, rules, repo bindings and the account
//! context of a repo. Each command is a thin async wrapper (off the IPC thread) around a
//! blocking `*_impl` function that tests call directly.
//!
//! Tokens arrive only through [`account_save_token`], go straight to the keychain inside a
//! `Zeroizing` buffer, and are never returned, logged or stored in SQLite.

use nanomgg_accounts::colors;
use nanomgg_accounts::{
    Account, AccountSecret, AuthMethod, BindingSource, ContextSource, Identity, MatchInput,
    Provider, ProviderKind, RemoteUrl, RepoBinding, Resolved, Rule, SigningMethod, Transport,
    new_id,
};
use nanomgg_credentials::{SecretKey, SecretKind};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use zeroize::Zeroizing;

use super::{
    KEY_WRITE_IDENTITY, KEY_WRITE_SSH_COMMAND, Snapshot, resolve_repo, storage_err,
    sync_open_repos, sync_repo_config, write_flags,
};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppState) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>()))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

fn keychain_err(e: nanomgg_credentials::KeychainError) -> AppError {
    AppError::Internal(e.to_string())
}

fn secret_kind(kind: AccountSecret) -> SecretKind {
    match kind {
        AccountSecret::Token => SecretKind::Token,
        AccountSecret::RefreshToken => SecretKind::RefreshToken,
    }
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

// ----- DTOs ----------------------------------------------------------------------------------

/// Create (no `id`) or update a provider. Omitted API URL / thresholds use the kind's defaults.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInput {
    #[serde(default)]
    #[specta(optional)]
    pub id: Option<String>,
    pub kind: ProviderKind,
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    #[specta(optional)]
    pub api_url: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub oauth_client_id: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub large_file_warn_mb: Option<u32>,
    #[serde(default)]
    #[specta(optional)]
    pub large_file_block_mb: Option<u32>,
}

/// Create (no `id`) or update an account profile. Tokens are saved separately.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountInput {
    #[serde(default)]
    #[specta(optional)]
    pub id: Option<String>,
    pub provider_id: String,
    pub username: String,
    #[serde(default)]
    #[specta(optional)]
    pub display_name: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub avatar_url: Option<String>,
    pub auth_method: AuthMethod,
    #[serde(default)]
    #[specta(optional)]
    pub scopes: Vec<String>,
    #[serde(default)]
    #[specta(optional)]
    pub token_expires_at: Option<i64>,
    /// `None` on create → next free palette color.
    #[serde(default)]
    #[specta(optional)]
    pub color: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub label: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub default_clone_dir: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IdentityInput {
    #[serde(default)]
    #[specta(optional)]
    pub id: Option<String>,
    pub account_id: String,
    pub name: String,
    pub email: String,
    #[serde(default)]
    #[specta(optional)]
    pub signing: SigningMethod,
    #[serde(default)]
    #[specta(optional)]
    pub signing_key_ref: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub is_default: bool,
}

/// Create (no `id`, appended last unless `priority` is given) or update a rule.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RuleInput {
    #[serde(default)]
    #[specta(optional)]
    pub id: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub priority: Option<i64>,
    #[serde(default)]
    #[specta(optional)]
    pub host: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub owner_or_group: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub path_glob: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub remote_regex: Option<String>,
    pub account_id: String,
    #[serde(default)]
    #[specta(optional)]
    pub identity_id: Option<String>,
    #[serde(default = "yes")]
    #[specta(optional)]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

/// "Use for this repo": binds the repo (or one remote) to an account.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BindingInput {
    /// `None` = repo-level default.
    #[serde(default)]
    #[specta(optional)]
    pub remote: Option<String>,
    pub account_id: String,
    #[serde(default)]
    #[specta(optional)]
    pub identity_id: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub transport: Option<Transport>,
    /// "Ghi nhớ": pin it above clone bindings and rules.
    #[serde(default)]
    #[specta(optional)]
    pub locked: bool,
    /// Defaults to `manual` (the clone flow passes `clone`).
    #[serde(default)]
    #[specta(optional)]
    pub source: Option<BindingSource>,
}

/// Account context of one remote, for the account pill / remote list.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAccountContext {
    pub remote: String,
    /// URL as configured, password removed.
    pub url: String,
    pub host: Option<String>,
    /// Transport implied by the URL (`None` for local paths).
    pub transport: Option<Transport>,
    pub account: Option<Account>,
    pub identity: Option<Identity>,
    pub source: ContextSource,
    pub matched_rule: Option<Rule>,
    /// Comes from the repo-level binding rather than one made for this remote.
    pub inherited: bool,
}

/// Which account/identity a repo uses and why (PLAN §4.3, §4.5).
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RepoAccountContext {
    pub repo_id: i64,
    /// `None` → the repo is unbound: show the "choose an account" banner.
    pub account: Option<Account>,
    pub identity: Option<Identity>,
    pub source: ContextSource,
    pub matched_rule: Option<Rule>,
    pub per_remote: Vec<RemoteAccountContext>,
    pub bindings: Vec<RepoBinding>,
}

/// Preview of the rule engine for a URL and/or folder ("Test rule").
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RuleTestResult {
    pub parsed: Option<RemoteUrl>,
    pub matched_rule: Option<Rule>,
    pub account: Option<Account>,
    pub identity: Option<Identity>,
    /// Ids of every matching rule, best first.
    pub all_matches: Vec<String>,
    /// Stored rules that no longer compile (skipped by the engine).
    pub invalid_rules: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountSettings {
    /// `identity.writeToRepoConfig` (default true).
    pub identity_write_to_repo_config: bool,
    /// `ssh.writeCoreSshCommand` (default false).
    pub ssh_write_core_ssh_command: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountSettingsPatch {
    #[serde(default)]
    #[specta(optional)]
    pub identity_write_to_repo_config: Option<bool>,
    #[serde(default)]
    #[specta(optional)]
    pub ssh_write_core_ssh_command: Option<bool>,
}

// ----- implementations -----------------------------------------------------------------------

pub(crate) fn provider_save_impl(st: &AppState, input: ProviderInput) -> AppResult<Provider> {
    let (warn, block) = input.kind.default_large_file_limits();
    let base_url = input.base_url.trim().trim_end_matches('/').to_owned();
    let provider = Provider {
        id: input.id.clone().unwrap_or_else(|| new_id("prv")),
        kind: input.kind,
        name: input.name.trim().to_owned(),
        api_url: non_empty(input.api_url)
            .map(|u| u.trim_end_matches('/').to_owned())
            .unwrap_or_else(|| input.kind.default_api_url(&base_url)),
        base_url,
        oauth_client_id: non_empty(input.oauth_client_id),
        large_file_warn_mb: input.large_file_warn_mb.unwrap_or(warn),
        large_file_block_mb: input.large_file_block_mb.or(block),
        builtin: false,
    };
    let saved = if input.id.is_some() {
        st.storage.update_provider(&provider)
    } else {
        st.storage.insert_provider(&provider)
    };
    saved.map_err(storage_err)
}

pub(crate) fn account_save_impl(st: &AppState, input: AccountInput) -> AppResult<Account> {
    let existing = match &input.id {
        Some(id) => Some(
            st.storage
                .account(id)
                .map_err(storage_err)?
                .ok_or_else(|| AppError::InvalidInput(format!("account `{id}` not found")))?,
        ),
        None => None,
    };
    let color = match input.color {
        Some(c) => c.trim().to_ascii_lowercase(),
        None => match &existing {
            Some(a) => a.color.clone(),
            None => {
                let accounts = st.storage.accounts().map_err(storage_err)?;
                colors::next_color(accounts.iter().map(|a| a.color.as_str())).to_owned()
            }
        },
    };
    let account = Account {
        id: input.id.clone().unwrap_or_else(|| new_id("acc")),
        provider_id: input.provider_id,
        username: input.username.trim().to_owned(),
        display_name: non_empty(input.display_name),
        avatar_url: non_empty(input.avatar_url),
        auth_method: input.auth_method,
        scopes: input.scopes,
        token_expires_at: input.token_expires_at,
        color,
        label: non_empty(input.label),
        default_clone_dir: non_empty(input.default_clone_dir),
        secrets: existing
            .as_ref()
            .map(|a| a.secrets.clone())
            .unwrap_or_default(),
        sort_order: existing.as_ref().map_or(0, |a| a.sort_order),
        created_at: 0,
        updated_at: 0,
    };
    let saved = if existing.is_some() {
        st.storage.update_account(&account)
    } else {
        st.storage.insert_account(&account)
    };
    saved.map_err(storage_err)
}

/// Writes the secret to the keychain, then records that it exists.
pub(crate) fn account_save_token_impl(
    st: &AppState,
    account_id: &str,
    kind: AccountSecret,
    token: Zeroizing<String>,
) -> AppResult<Account> {
    let account = st
        .storage
        .account(account_id)
        .map_err(storage_err)?
        .ok_or_else(|| AppError::InvalidInput(format!("account `{account_id}` not found")))?;
    let token = Zeroizing::new(token.trim().to_owned());
    if token.is_empty() || token.contains(['\n', '\r']) {
        return Err(AppError::InvalidInput(
            "the token is empty or malformed".into(),
        ));
    }
    st.accounts
        .secrets
        .set(&SecretKey::account(&account.id, secret_kind(kind)), &token)
        .map_err(keychain_err)?;
    let mut secrets = account.secrets.clone();
    secrets.push(kind);
    tracing::info!(account = %account.id, kind = kind.as_str(), "secret saved to keychain");
    st.storage
        .set_account_secrets(&account.id, &secrets)
        .map_err(storage_err)
}

pub(crate) fn account_delete_token_impl(
    st: &AppState,
    account_id: &str,
    kind: AccountSecret,
) -> AppResult<Account> {
    let account = st
        .storage
        .account(account_id)
        .map_err(storage_err)?
        .ok_or_else(|| AppError::InvalidInput(format!("account `{account_id}` not found")))?;
    st.accounts
        .secrets
        .delete(&SecretKey::account(&account.id, secret_kind(kind)))
        .map_err(keychain_err)?;
    let secrets: Vec<_> = account
        .secrets
        .iter()
        .copied()
        .filter(|s| *s != kind)
        .collect();
    st.storage
        .set_account_secrets(&account.id, &secrets)
        .map_err(storage_err)
}

/// Stores a login result (for the auth commands of wave 2, e.g. from `nanomgg_auth::TokenSet`):
/// access token and optional refresh token in the keychain, expiry and granted scopes in
/// SQLite. A missing refresh token removes a stale one.
pub fn account_store_tokens(
    st: &AppState,
    account_id: &str,
    access_token: Zeroizing<String>,
    refresh_token: Option<Zeroizing<String>>,
    expires_at: Option<i64>,
    scopes: Vec<String>,
) -> AppResult<Account> {
    let mut account = account_save_token_impl(st, account_id, AccountSecret::Token, access_token)?;
    account = match refresh_token {
        Some(refresh) => {
            account_save_token_impl(st, account_id, AccountSecret::RefreshToken, refresh)?
        }
        None if account.has_secret(AccountSecret::RefreshToken) => {
            account_delete_token_impl(st, account_id, AccountSecret::RefreshToken)?
        }
        None => account,
    };
    account.token_expires_at = expires_at;
    account.scopes = scopes;
    st.storage.update_account(&account).map_err(storage_err)
}

/// Reads an account's secret from the keychain (for API calls by the provider clients).
pub fn account_secret(
    st: &AppState,
    account_id: &str,
    kind: AccountSecret,
) -> AppResult<Option<Zeroizing<String>>> {
    st.accounts
        .secrets
        .get(&SecretKey::account(account_id, secret_kind(kind)))
        .map_err(keychain_err)
}

/// Deletes the account (identities, rules, bindings cascade) and its keychain entries.
pub(crate) fn account_delete_impl(st: &AppState, account_id: &str) -> AppResult<bool> {
    for kind in AccountSecret::ALL {
        if let Err(e) = st
            .accounts
            .secrets
            .delete(&SecretKey::account(account_id, secret_kind(*kind)))
        {
            tracing::warn!(account = account_id, error = %e, "could not remove keychain entry");
        }
    }
    st.storage.delete_account(account_id).map_err(storage_err)
}

pub(crate) fn identity_save_impl(st: &AppState, input: IdentityInput) -> AppResult<Identity> {
    let identity = Identity {
        id: input.id.clone().unwrap_or_else(|| new_id("idn")),
        account_id: input.account_id,
        name: input.name.trim().to_owned(),
        email: input.email.trim().to_owned(),
        signing: input.signing,
        signing_key_ref: non_empty(input.signing_key_ref),
        is_default: input.is_default,
        created_at: 0,
    };
    let saved = if input.id.is_some() {
        st.storage.update_identity(&identity)
    } else {
        st.storage.insert_identity(&identity)
    }
    .map_err(storage_err)?;
    sync_open_repos(st);
    Ok(saved)
}

pub(crate) fn rule_save_impl(st: &AppState, input: RuleInput) -> AppResult<Rule> {
    let existing = match &input.id {
        Some(id) => Some(
            st.storage
                .rule(id)
                .map_err(storage_err)?
                .ok_or_else(|| AppError::InvalidInput(format!("rule `{id}` not found")))?,
        ),
        None => None,
    };
    let priority = match (input.priority, &existing) {
        (Some(p), _) => p,
        (None, Some(r)) => r.priority,
        (None, None) => st.storage.next_rule_priority().map_err(storage_err)?,
    };
    let rule = Rule {
        id: input.id.clone().unwrap_or_else(|| new_id("rul")),
        priority,
        host: non_empty(input.host).map(|h| h.to_ascii_lowercase()),
        owner_or_group: non_empty(input.owner_or_group),
        path_glob: non_empty(input.path_glob),
        remote_regex: non_empty(input.remote_regex),
        account_id: input.account_id,
        identity_id: non_empty(input.identity_id),
        enabled: input.enabled,
        created_at: 0,
    };
    let saved = if existing.is_some() {
        st.storage.update_rule(&rule)
    } else {
        st.storage.insert_rule(&rule)
    }
    .map_err(storage_err)?;
    sync_open_repos(st);
    Ok(saved)
}

pub(crate) fn rule_test_impl(
    st: &AppState,
    url: Option<String>,
    path: Option<String>,
) -> AppResult<RuleTestResult> {
    let snap = Snapshot::load(&st.storage)?;
    let url = non_empty(url);
    let path = non_empty(path);
    let input = MatchInput {
        repo_path: path.as_deref(),
        remote_url: url.as_deref(),
    };
    let matches = snap.rules.matching(&input);
    let matched = matches.first().copied().cloned();
    let dir = snap.dir();
    let account = matched
        .as_ref()
        .and_then(|r| dir.account(&r.account_id).cloned());
    let identity = matched.as_ref().and_then(|r| {
        dir.identity_for(&r.account_id, r.identity_id.as_deref())
            .cloned()
    });
    Ok(RuleTestResult {
        parsed: url.as_deref().and_then(RemoteUrl::parse),
        all_matches: matches.iter().map(|r| r.id.clone()).collect(),
        invalid_rules: snap
            .rules
            .invalid()
            .iter()
            .map(|(id, _)| id.clone())
            .collect(),
        matched_rule: matched,
        account,
        identity,
    })
}

pub(crate) fn repo_account_context_impl(
    st: &AppState,
    repo_id: i64,
) -> AppResult<RepoAccountContext> {
    let root = crate::repos::repo_root(st, repo_id)?;
    let resolution = resolve_repo(st, repo_id, &root)?;
    let bindings = st.storage.repo_bindings(repo_id).map_err(storage_err)?;
    let snap = Snapshot::load(&st.storage)?;
    let dir = snap.dir();
    let rules: Vec<&Rule> = snap.rules.rules().collect();
    let describe = |r: Option<&Resolved>| {
        let account = r.and_then(|r| dir.account(&r.account_id).cloned());
        let identity = r.and_then(|r| {
            r.identity_id
                .as_deref()
                .and_then(|id| snap.identities.iter().find(|i| i.id == id).cloned())
        });
        let matched_rule = r.and_then(|r| r.rule_id.as_deref()).and_then(|id| {
            rules
                .iter()
                .find(|rule| rule.id == id)
                .map(|rule| (*rule).clone())
        });
        let source = r.map_or(ContextSource::None, |r| r.source);
        (account, identity, source, matched_rule)
    };
    let (account, identity, source, matched_rule) = describe(resolution.default.as_ref());
    let per_remote = resolution
        .remotes
        .iter()
        .map(|rr| {
            let (account, identity, source, matched_rule) = describe(rr.resolved.as_ref());
            RemoteAccountContext {
                remote: rr.remote.clone(),
                url: rr.url.clone(),
                host: rr
                    .parsed
                    .as_ref()
                    .filter(|u| !u.is_local())
                    .map(RemoteUrl::host_port),
                transport: rr.parsed.as_ref().and_then(|u| match u.scheme {
                    nanomgg_accounts::UrlScheme::Https | nanomgg_accounts::UrlScheme::Http => {
                        Some(Transport::Https)
                    }
                    nanomgg_accounts::UrlScheme::Ssh => Some(Transport::Ssh),
                    _ => None,
                }),
                account,
                identity,
                source,
                matched_rule,
                inherited: rr
                    .resolved
                    .as_ref()
                    .is_some_and(|r| r.from_binding && r.binding_remote.is_none()),
            }
        })
        .collect();
    Ok(RepoAccountContext {
        repo_id,
        account,
        identity,
        source,
        matched_rule,
        per_remote,
        bindings,
    })
}

pub(crate) fn repo_binding_set_impl(
    st: &AppState,
    repo_id: i64,
    input: BindingInput,
) -> AppResult<RepoAccountContext> {
    crate::repos::repo_root(st, repo_id)?;
    st.storage
        .set_repo_binding(&RepoBinding {
            repo_id,
            remote_name: non_empty(input.remote),
            account_id: input.account_id,
            identity_id: non_empty(input.identity_id),
            transport: input.transport,
            source: input.source.unwrap_or(BindingSource::Manual),
            locked: input.locked,
            updated_at: 0,
        })
        .map_err(storage_err)?;
    sync_repo_config(st, repo_id)?;
    repo_account_context_impl(st, repo_id)
}

pub(crate) fn repo_binding_clear_impl(
    st: &AppState,
    repo_id: i64,
    remote: Option<String>,
) -> AppResult<RepoAccountContext> {
    crate::repos::repo_root(st, repo_id)?;
    st.storage
        .clear_repo_binding(repo_id, non_empty(remote).as_deref())
        .map_err(storage_err)?;
    sync_repo_config(st, repo_id)?;
    repo_account_context_impl(st, repo_id)
}

pub(crate) fn account_settings_impl(st: &AppState) -> AppResult<AccountSettings> {
    let (identity, ssh) = write_flags(&st.storage)?;
    Ok(AccountSettings {
        identity_write_to_repo_config: identity,
        ssh_write_core_ssh_command: ssh,
    })
}

// ----- commands ------------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub async fn provider_list(app: AppHandle) -> AppResult<Vec<Provider>> {
    blocking(app, |st| st.storage.providers().map_err(storage_err)).await
}

#[tauri::command]
#[specta::specta]
pub async fn provider_save(app: AppHandle, provider: ProviderInput) -> AppResult<Provider> {
    blocking(app, move |st| provider_save_impl(st, provider)).await
}

#[tauri::command]
#[specta::specta]
pub async fn provider_delete(app: AppHandle, id: String) -> AppResult<bool> {
    blocking(app, move |st| {
        st.storage.delete_provider(&id).map_err(storage_err)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn account_list(app: AppHandle) -> AppResult<Vec<Account>> {
    blocking(app, |st| st.storage.accounts().map_err(storage_err)).await
}

#[tauri::command]
#[specta::specta]
pub async fn account_save(app: AppHandle, account: AccountInput) -> AppResult<Account> {
    blocking(app, move |st| account_save_impl(st, account)).await
}

#[tauri::command]
#[specta::specta]
pub async fn account_delete(app: AppHandle, id: String) -> AppResult<bool> {
    blocking(app, move |st| account_delete_impl(st, &id)).await
}

/// Reorders the switcher (`Ctrl/Cmd+Alt+1..9`).
#[tauri::command]
#[specta::specta]
pub async fn account_reorder(app: AppHandle, ids: Vec<String>) -> AppResult<Vec<Account>> {
    blocking(app, move |st| {
        st.storage.reorder_accounts(&ids).map_err(storage_err)?;
        st.storage.accounts().map_err(storage_err)
    })
    .await
}

/// Stores an already-obtained token (OAuth flow result or PAT) in the OS keychain.
#[tauri::command]
#[specta::specta]
pub async fn account_save_token(
    app: AppHandle,
    account_id: String,
    kind: AccountSecret,
    token: String,
) -> AppResult<Account> {
    let token = Zeroizing::new(token);
    blocking(app, move |st| {
        account_save_token_impl(st, &account_id, kind, token)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn account_delete_token(
    app: AppHandle,
    account_id: String,
    kind: AccountSecret,
) -> AppResult<Account> {
    blocking(app, move |st| {
        account_delete_token_impl(st, &account_id, kind)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn identity_list(app: AppHandle, account_id: Option<String>) -> AppResult<Vec<Identity>> {
    blocking(app, move |st| {
        st.storage
            .identities(account_id.as_deref())
            .map_err(storage_err)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn identity_save(app: AppHandle, identity: IdentityInput) -> AppResult<Identity> {
    blocking(app, move |st| identity_save_impl(st, identity)).await
}

#[tauri::command]
#[specta::specta]
pub async fn identity_delete(app: AppHandle, id: String) -> AppResult<bool> {
    blocking(app, move |st| {
        let deleted = st.storage.delete_identity(&id).map_err(storage_err)?;
        sync_open_repos(st);
        Ok(deleted)
    })
    .await
}

/// Makes the identity its account's default; returns that account's identities.
#[tauri::command]
#[specta::specta]
pub async fn identity_set_default(app: AppHandle, id: String) -> AppResult<Vec<Identity>> {
    blocking(app, move |st| {
        st.storage.set_default_identity(&id).map_err(storage_err)?;
        sync_open_repos(st);
        let identity = st.storage.identity(&id).map_err(storage_err)?;
        st.storage
            .identities(identity.as_ref().map(|i| i.account_id.as_str()))
            .map_err(storage_err)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn rule_list(app: AppHandle) -> AppResult<Vec<Rule>> {
    blocking(app, |st| st.storage.rules().map_err(storage_err)).await
}

#[tauri::command]
#[specta::specta]
pub async fn rule_save(app: AppHandle, rule: RuleInput) -> AppResult<Rule> {
    blocking(app, move |st| rule_save_impl(st, rule)).await
}

#[tauri::command]
#[specta::specta]
pub async fn rule_delete(app: AppHandle, id: String) -> AppResult<bool> {
    blocking(app, move |st| {
        let deleted = st.storage.delete_rule(&id).map_err(storage_err)?;
        sync_open_repos(st);
        Ok(deleted)
    })
    .await
}

/// New evaluation order: `ids` first (priority 0..), other rules after.
#[tauri::command]
#[specta::specta]
pub async fn rule_reorder(app: AppHandle, ids: Vec<String>) -> AppResult<Vec<Rule>> {
    blocking(app, move |st| {
        let rules = st.storage.reorder_rules(&ids).map_err(storage_err)?;
        sync_open_repos(st);
        Ok(rules)
    })
    .await
}

/// Which rule/account a URL and/or folder would get, without saving anything.
#[tauri::command]
#[specta::specta]
pub async fn rule_test(
    app: AppHandle,
    url: Option<String>,
    path: Option<String>,
) -> AppResult<RuleTestResult> {
    blocking(app, move |st| rule_test_impl(st, url, path)).await
}

#[tauri::command]
#[specta::specta]
pub async fn repo_binding_get(app: AppHandle, repo_id: i64) -> AppResult<Vec<RepoBinding>> {
    blocking(app, move |st| {
        st.storage.repo_bindings(repo_id).map_err(storage_err)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn repo_binding_set(
    app: AppHandle,
    repo_id: i64,
    binding: BindingInput,
) -> AppResult<RepoAccountContext> {
    blocking(app, move |st| repo_binding_set_impl(st, repo_id, binding)).await
}

/// Removes the binding of `remote` (or the repo-level one when `None`).
#[tauri::command]
#[specta::specta]
pub async fn repo_binding_clear(
    app: AppHandle,
    repo_id: i64,
    remote: Option<String>,
) -> AppResult<RepoAccountContext> {
    blocking(app, move |st| repo_binding_clear_impl(st, repo_id, remote)).await
}

/// Account pill data for an open repo. A pure read (every open tab polls it): the identity
/// copy in `.git/config` is written when a binding / identity / rule / setting changes and
/// right before a commit (`worktree::commit_create`), never here.
#[tauri::command]
#[specta::specta]
pub async fn repo_account_context(app: AppHandle, repo_id: i64) -> AppResult<RepoAccountContext> {
    blocking(app, move |st| repo_account_context_impl(st, repo_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn account_settings_get(app: AppHandle) -> AppResult<AccountSettings> {
    blocking(app, account_settings_impl).await
}

#[tauri::command]
#[specta::specta]
pub async fn account_settings_update(
    app: AppHandle,
    patch: AccountSettingsPatch,
) -> AppResult<AccountSettings> {
    blocking(app, move |st| {
        if let Some(v) = patch.identity_write_to_repo_config {
            st.storage.set_setting(KEY_WRITE_IDENTITY, &v)?;
        }
        if let Some(v) = patch.ssh_write_core_ssh_command {
            st.storage.set_setting(KEY_WRITE_SSH_COMMAND, &v)?;
        }
        sync_open_repos(st);
        account_settings_impl(st)
    })
    .await
}
