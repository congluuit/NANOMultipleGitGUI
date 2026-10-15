//! SSH keys per account (PLAN §4.4, §10 Phase 2 "Tạo/nhập SSH key, upload public key lên provider
//! qua API").
//!
//! - Private keys live in the app key store `<app data>/ssh/<key id>` (user-only ACL / `0600`,
//!   see `nanomgg_ssh_keys::KeyStore`). Importing **copies** the user's file there; the original
//!   is never modified.
//! - Passphrases go to the OS keychain as `nanomgg:<key id>:ssh_passphrase`, so the
//!   `nanomgg-credential` sidecar can answer ssh's `SSH_ASKPASS` prompt.
//! - Each account has one **default key** (setting `ssh.defaultKey.<account id>`), the one
//!   [`crate::accounts::context_for`] injects with `GIT_SSH_COMMAND` for repos bound with SSH.
//!   Without an explicit choice (or when the chosen key file vanished) the account's first key
//!   whose file exists is used.
//! - Upload uses the account's token (`account_secret`) and the provider API; missing scopes come
//!   back as a typed result the UI turns into an actionable message.
//! - "Test connection" runs `ssh -T git@<host>` with the account's key and tells which user the
//!   provider authenticated (GitHub "Hi <user>!", GitLab "Welcome to GitLab, @<user>!").

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use nanomgg_accounts::{
    Account, AccountSecret, Provider, ProviderKind as AccountProviderKind, SshKey, new_id,
};
use nanomgg_credentials::SecretKey;
use nanomgg_git_engine::AccountContext;
use nanomgg_providers::{
    AccessHint, HttpConfig, Provider as _, ProviderClient, ProviderEndpoint, ProviderError,
    ProviderKind, SecretToken, build_http_client,
};
use nanomgg_ssh_keys::{
    HostKeyPolicy, KeyStore, SshCommandOptions, SshKeyError, check_permissions, generate_ed25519,
    import_private_key_file, ssh_args,
};
use nanomgg_storage::Storage;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};
use zeroize::Zeroizing;

use crate::accounts::commands::account_secret;
use crate::accounts::storage_err;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Setting prefix: `ssh.defaultKey.<account id>` = key id.
pub const KEY_DEFAULT_PREFIX: &str = "ssh.defaultKey.";

fn default_setting(account_id: &str) -> String {
    format!("{KEY_DEFAULT_PREFIX}{account_id}")
}

// ----- DTOs ----------------------------------------------------------------------------------

/// An SSH key as shown in the account's "SSH keys" dialog. Never contains secret material.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SshKeyInfo {
    pub id: String,
    pub account_id: Option<String>,
    /// `ed25519`, `rsa`, `ecdsa`…
    pub key_type: String,
    /// `SHA256:…`.
    pub fingerprint: String,
    /// OpenSSH public key line (what "Copy public key" copies).
    pub public_key: String,
    pub comment: Option<String>,
    pub private_key_path: Option<String>,
    pub has_passphrase: bool,
    /// The passphrase is stored in the OS keychain (git can use the key without prompting).
    pub passphrase_saved: bool,
    pub uploaded_to_provider: bool,
    pub provider_key_id: Option<String>,
    pub created_at: i64,
    /// The key `GIT_SSH_COMMAND` uses for this account (explicit or implicit default).
    pub is_default: bool,
    /// The user picked it with "Set default".
    pub explicit_default: bool,
    /// The private key file exists.
    pub file_exists: bool,
    /// Stored in the app key store (permissions can be repaired, files removed on delete).
    pub managed: bool,
}

/// Outcome of "Import from file".
#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SshImportResult {
    /// Copied into the app key store. `publicFileMismatch`: the `.pub` next to the file belongs
    /// to another key (the private key wins).
    #[serde(rename_all = "camelCase")]
    Imported {
        key: SshKeyInfo,
        public_file_mismatch: bool,
    },
    /// The key is encrypted: ask for its passphrase and try again.
    PassphraseRequired,
    WrongPassphrase,
    /// The key is already known: same account (`accountId` = this one) or another account.
    #[serde(rename_all = "camelCase")]
    AlreadyKnown {
        key_id: String,
        account_id: Option<String>,
    },
}

/// Outcome of "Upload to provider".
#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SshUploadResult {
    Uploaded {
        key: SshKeyInfo,
    },
    /// The provider already had the key on this account; it is now marked as uploaded.
    AlreadyRegistered {
        key: SshKeyInfo,
    },
    /// The provider says the key is in use, but not on this account (GitHub: a key belongs to
    /// one user only).
    UsedByAnotherAccount,
    /// The token lacks a scope: GitHub `admin:public_key`/`write:public_key`, GitLab `api`.
    MissingScope {
        required: Vec<String>,
        granted: Option<Vec<String>>,
    },
    /// Fine-grained GitHub token without the "Git SSH keys" (write) permission.
    TokenPermissionMissing,
    /// No token saved for the account: sign in again.
    NoToken,
    /// Token rejected (expired / revoked): sign in again.
    Unauthorized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SshTestStatus {
    /// The provider accepted the key; see `username`.
    Authenticated,
    /// `Permission denied (publickey)`: the key is not registered on the provider.
    Denied,
    /// The host key changed or could not be verified.
    HostKeyFailed,
    /// DNS / connect / timeout.
    Network,
    /// The key needs a passphrase that is not saved.
    PassphraseNeeded,
    Unknown,
}

/// Result of `ssh -T git@<host>`.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SshTestResult {
    pub status: SshTestStatus,
    pub host: String,
    /// User the provider authenticated (GitHub `owner/repo` for a deploy key).
    pub username: Option<String>,
    /// The account's own user name.
    pub expected_username: String,
    /// `username` is the account's user (case-insensitive).
    pub matches_account: bool,
    /// The key was registered as a deploy key of one repository (GitHub).
    pub deploy_key: bool,
    pub key_id: String,
    /// ssh output (stderr + stdout), trimmed, for the details view.
    pub output: String,
}

/// Permission status of a key file.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SshKeyPermissions {
    pub key_id: String,
    /// Only the current user (plus SYSTEM/Administrators on Windows) can read the file.
    pub restricted: bool,
    pub entries: Vec<String>,
    pub problem: Option<String>,
    /// The app may fix it (key inside the app key store).
    pub repairable: bool,
}

// ----- helpers -------------------------------------------------------------------------------

pub(crate) fn ssh_err(e: SshKeyError) -> AppError {
    match e {
        e @ (SshKeyError::LegacyPem { .. } | SshKeyError::PuttyFormat) => {
            AppError::Unsupported(e.to_string())
        }
        e @ (SshKeyError::NotAPrivateKey
        | SshKeyError::InvalidKey(_)
        | SshKeyError::InvalidComment
        | SshKeyError::InvalidKeyId(_)
        | SshKeyError::PassphraseRequired
        | SshKeyError::WrongPassphrase) => AppError::InvalidInput(e.to_string()),
        e @ SshKeyError::NotFound(_) => AppError::PathNotFound(e.to_string()),
        other => AppError::Internal(other.to_string()),
    }
}

fn keychain_err(e: nanomgg_credentials::KeychainError) -> AppError {
    AppError::Internal(e.to_string())
}

fn account_of(storage: &Storage, account_id: &str) -> AppResult<Account> {
    storage
        .account(account_id)
        .map_err(storage_err)?
        .ok_or_else(|| AppError::InvalidInput(format!("account `{account_id}` not found")))
}

fn key_of(storage: &Storage, key_id: &str) -> AppResult<SshKey> {
    storage
        .ssh_key(key_id)
        .map_err(storage_err)?
        .ok_or_else(|| AppError::InvalidInput(format!("SSH key `{key_id}` not found")))
}

fn provider_of(storage: &Storage, account: &Account) -> AppResult<Provider> {
    storage
        .provider(&account.provider_id)
        .map_err(storage_err)?
        .ok_or_else(|| {
            AppError::InvalidInput(format!("provider `{}` not found", account.provider_id))
        })
}

fn file_exists(key: &SshKey) -> bool {
    key.private_key_path
        .as_deref()
        .is_some_and(|p| Path::new(p).is_file())
}

/// The key `GIT_SSH_COMMAND` uses for `account_id`: the explicit default when its file exists,
/// else the first key (oldest) whose file exists.
pub fn default_key(storage: &Storage, account_id: &str) -> AppResult<Option<SshKey>> {
    let keys = storage.ssh_keys(Some(account_id)).map_err(storage_err)?;
    let explicit: Option<String> = storage.get_setting(&default_setting(account_id))?;
    if let Some(id) = explicit
        && let Some(k) = keys.iter().find(|k| k.id == id && file_exists(k))
    {
        return Ok(Some(k.clone()));
    }
    Ok(keys.into_iter().find(file_exists))
}

/// Private key path of [`default_key`] (used by the account-context resolver).
pub fn default_key_path(storage: &Storage, account_id: &str) -> AppResult<Option<PathBuf>> {
    Ok(default_key(storage, account_id)?
        .and_then(|k| k.private_key_path)
        .map(PathBuf::from))
}

fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        let s = p.to_string_lossy().replace('\\', "/");
        if cfg!(windows) {
            s.to_ascii_lowercase()
        } else {
            s
        }
    };
    norm(a) == norm(b)
}

fn is_managed(store: &KeyStore, key: &SshKey) -> bool {
    match (
        key.private_key_path.as_deref(),
        store.private_key_path(&key.id),
    ) {
        (Some(path), Ok(expected)) => same_path(Path::new(path), &expected),
        _ => false,
    }
}

fn info_of(st: &AppState, store: &KeyStore, key: SshKey, default_id: Option<&str>) -> SshKeyInfo {
    let explicit: Option<String> = key
        .account_id
        .as_deref()
        .and_then(|acc| st.storage.get_setting(&default_setting(acc)).ok().flatten());
    let passphrase_saved = key.has_passphrase
        && st
            .accounts
            .secrets
            .get(&SecretKey::ssh_passphrase(&key.id))
            .ok()
            .flatten()
            .is_some();
    SshKeyInfo {
        is_default: default_id == Some(key.id.as_str()),
        explicit_default: explicit.as_deref() == Some(key.id.as_str()),
        file_exists: file_exists(&key),
        managed: is_managed(store, &key),
        passphrase_saved,
        id: key.id,
        account_id: key.account_id,
        key_type: key.key_type,
        fingerprint: key.fingerprint,
        public_key: key.public_key,
        comment: key.comment,
        private_key_path: key.private_key_path,
        has_passphrase: key.has_passphrase,
        uploaded_to_provider: key.uploaded_to_provider,
        provider_key_id: key.provider_key_id,
        created_at: key.created_at,
    }
}

fn describe(st: &AppState, store: &KeyStore, key: SshKey) -> AppResult<SshKeyInfo> {
    let default_id = match key.account_id.as_deref() {
        Some(acc) => default_key(&st.storage, acc)?.map(|k| k.id),
        None => None,
    };
    Ok(info_of(st, store, key, default_id.as_deref()))
}

/// Short algorithm name stored in `key_type` (`ssh-ed25519` → `ed25519`).
fn key_type(algorithm: &str) -> String {
    match algorithm {
        "ssh-ed25519" => "ed25519".into(),
        "ssh-rsa" => "rsa".into(),
        "ssh-dss" => "dsa".into(),
        a if a.starts_with("ecdsa-") => "ecdsa".into(),
        a if a.starts_with("sk-ssh-ed25519") => "ed25519-sk".into(),
        a if a.starts_with("sk-ecdsa") => "ecdsa-sk".into(),
        a => a.to_owned(),
    }
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

/// Saves the passphrase of `key_id` in the keychain (no-op for `None`/empty).
fn save_passphrase(st: &AppState, key_id: &str, passphrase: Option<&str>) -> AppResult<bool> {
    match passphrase.filter(|p| !p.is_empty()) {
        Some(p) => {
            st.accounts
                .secrets
                .set(&SecretKey::ssh_passphrase(key_id), p)
                .map_err(keychain_err)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Undo of a half-finished generate/import.
fn rollback(st: &AppState, store: &KeyStore, key_id: &str) {
    if let Err(e) = store.remove(key_id) {
        tracing::warn!(key = key_id, error = %e, "could not remove key files after a failure");
    }
    let _ = st
        .accounts
        .secrets
        .delete(&SecretKey::ssh_passphrase(key_id));
}

/// Makes `key_id` the account's default when the account has no usable explicit default yet.
fn adopt_as_default_if_none(st: &AppState, account_id: &str, key_id: &str) -> AppResult<()> {
    let explicit: Option<String> = st.storage.get_setting(&default_setting(account_id))?;
    let usable = match explicit {
        Some(id) => st
            .storage
            .ssh_key(&id)
            .map_err(storage_err)?
            .is_some_and(|k| k.account_id.as_deref() == Some(account_id) && file_exists(&k)),
        None => false,
    };
    if !usable {
        st.storage
            .set_setting(&default_setting(account_id), key_id)?;
    }
    Ok(())
}

// ----- implementations -----------------------------------------------------------------------

pub(crate) fn list_impl(
    st: &AppState,
    store: &KeyStore,
    account_id: Option<&str>,
) -> AppResult<Vec<SshKeyInfo>> {
    let keys = st.storage.ssh_keys(account_id).map_err(storage_err)?;
    let mut out = Vec::with_capacity(keys.len());
    for key in keys {
        out.push(describe(st, store, key)?);
    }
    Ok(out)
}

/// Default comment: the account's default identity email, else `user@host`.
fn default_comment(st: &AppState, account: &Account) -> AppResult<String> {
    let identities = st
        .storage
        .identities(Some(&account.id))
        .map_err(storage_err)?;
    if let Some(i) = identities
        .iter()
        .find(|i| i.is_default)
        .or(identities.first())
    {
        return Ok(i.email.clone());
    }
    let host = provider_of(&st.storage, account)?
        .host()
        .unwrap_or_else(|| "git".into());
    Ok(format!("{}@{host}", account.username))
}

pub(crate) fn generate_impl(
    st: &AppState,
    store: &KeyStore,
    account_id: &str,
    comment: Option<String>,
    passphrase: Option<Zeroizing<String>>,
) -> AppResult<SshKeyInfo> {
    let account = account_of(&st.storage, account_id)?;
    let comment = match non_empty(comment) {
        Some(c) => c,
        None => default_comment(st, &account)?,
    };
    let pass = passphrase
        .as_ref()
        .map(|p| p.as_str())
        .filter(|p| !p.is_empty());
    let generated = generate_ed25519(&comment, pass).map_err(ssh_err)?;
    let key_id = new_id("key");
    let stored = store.save_generated(&key_id, &generated).map_err(ssh_err)?;
    drop(generated);

    let result = (|| {
        let has_passphrase = save_passphrase(st, &key_id, pass)?;
        let key = st
            .storage
            .insert_ssh_key(&SshKey {
                id: key_id.clone(),
                account_id: Some(account.id.clone()),
                key_type: key_type(&stored.info.algorithm),
                fingerprint: stored.info.fingerprint.clone(),
                public_key: stored.info.public_key.clone(),
                private_key_path: Some(stored.private_key_path.clone()),
                has_passphrase,
                uploaded_to_provider: false,
                provider_key_id: None,
                comment: Some(comment.clone()).filter(|c| !c.is_empty()),
                created_at: 0,
            })
            .map_err(storage_err)?;
        adopt_as_default_if_none(st, &account.id, &key.id)?;
        Ok(key)
    })();
    match result {
        Ok(key) => {
            tracing::info!(account = %account.id, key = %key.id, "SSH key generated");
            describe(st, store, key)
        }
        Err(e) => {
            rollback(st, store, &key_id);
            Err(e)
        }
    }
}

pub(crate) fn import_impl(
    st: &AppState,
    store: &KeyStore,
    account_id: Option<&str>,
    path: &Path,
    passphrase: Option<Zeroizing<String>>,
) -> AppResult<SshImportResult> {
    let account = match account_id {
        Some(id) => Some(account_of(&st.storage, id)?),
        None => None,
    };
    if !path.is_file() {
        return Err(AppError::PathNotFound(format!(
            "no file at {}",
            path.display()
        )));
    }
    let pass = passphrase
        .as_ref()
        .map(|p| p.as_str())
        .filter(|p| !p.is_empty());
    // Reads the user's file only; the normalised copy goes into the app store.
    let imported = match import_private_key_file(path, pass) {
        Ok(k) => k,
        Err(SshKeyError::PassphraseRequired) => return Ok(SshImportResult::PassphraseRequired),
        Err(SshKeyError::WrongPassphrase) => return Ok(SshImportResult::WrongPassphrase),
        Err(e) => return Err(ssh_err(e)),
    };
    let public_file_mismatch = imported.public_file == nanomgg_ssh_keys::PublicFileCheck::Mismatch;
    let account_ref = account.as_ref().map(|a| a.id.clone());

    // Same key already known (fingerprints are unique).
    if let Some(existing) = st
        .storage
        .ssh_keys(None)
        .map_err(storage_err)?
        .into_iter()
        .find(|k| k.fingerprint == imported.info.fingerprint)
    {
        if existing.account_id.is_none()
            && let Some(acc) = &account_ref
        {
            // An unassigned key (account deleted, or imported by the wizard) is adopted.
            let mut adopted = existing.clone();
            adopted.account_id = Some(acc.clone());
            let key = st.storage.update_ssh_key(&adopted).map_err(storage_err)?;
            if imported.info.encrypted {
                save_passphrase(st, &key.id, pass)?;
            }
            adopt_as_default_if_none(st, acc, &key.id)?;
            return Ok(SshImportResult::Imported {
                key: describe(st, store, key)?,
                public_file_mismatch,
            });
        }
        return Ok(SshImportResult::AlreadyKnown {
            key_id: existing.id,
            account_id: existing.account_id,
        });
    }

    let key_id = new_id("key");
    let stored = store.save_imported(&key_id, &imported).map_err(ssh_err)?;
    let encrypted = imported.info.encrypted;
    drop(imported);
    let result = (|| {
        if encrypted {
            save_passphrase(st, &key_id, pass)?;
        }
        let key = st
            .storage
            .insert_ssh_key(&SshKey {
                id: key_id.clone(),
                account_id: account_ref.clone(),
                key_type: key_type(&stored.info.algorithm),
                fingerprint: stored.info.fingerprint.clone(),
                public_key: stored.info.public_key.clone(),
                private_key_path: Some(stored.private_key_path.clone()),
                has_passphrase: encrypted,
                uploaded_to_provider: false,
                provider_key_id: None,
                comment: Some(stored.info.comment.clone()).filter(|c| !c.is_empty()),
                created_at: 0,
            })
            .map_err(storage_err)?;
        if let Some(acc) = &account_ref {
            adopt_as_default_if_none(st, acc, &key.id)?;
        }
        Ok(key)
    })();
    match result {
        Ok(key) => {
            tracing::info!(account = ?account_ref, key = %key.id, "SSH key imported");
            Ok(SshImportResult::Imported {
                key: describe(st, store, key)?,
                public_file_mismatch,
            })
        }
        Err(e) => {
            rollback(st, store, &key_id);
            Err(e)
        }
    }
}

/// Deletes the key record, its files in the app store and its keychain passphrase. Files outside
/// the app store (never created by the app) are left alone. The key stays registered on the
/// provider (remove it there if it leaked).
pub(crate) fn delete_impl(st: &AppState, store: &KeyStore, key_id: &str) -> AppResult<bool> {
    let Some(key) = st.storage.ssh_key(key_id).map_err(storage_err)? else {
        return Ok(false);
    };
    if is_managed(store, &key) {
        store.remove(&key.id).map_err(ssh_err)?;
    }
    if let Err(e) = st
        .accounts
        .secrets
        .delete(&SecretKey::ssh_passphrase(&key.id))
    {
        tracing::warn!(key = key_id, error = %e, "could not remove the key passphrase");
    }
    if let Some(acc) = &key.account_id {
        let explicit: Option<String> = st.storage.get_setting(&default_setting(acc))?;
        if explicit.as_deref() == Some(key.id.as_str()) {
            st.storage.delete_setting(&default_setting(acc))?;
        }
    }
    let deleted = st.storage.delete_ssh_key(&key.id).map_err(storage_err)?;
    tracing::info!(key = key_id, "SSH key deleted");
    Ok(deleted)
}

pub(crate) fn set_default_impl(
    st: &AppState,
    store: &KeyStore,
    account_id: &str,
    key_id: &str,
) -> AppResult<Vec<SshKeyInfo>> {
    account_of(&st.storage, account_id)?;
    let key = key_of(&st.storage, key_id)?;
    if key.account_id.as_deref() != Some(account_id) {
        return Err(AppError::InvalidInput(format!(
            "SSH key `{key_id}` does not belong to account `{account_id}`"
        )));
    }
    st.storage
        .set_setting(&default_setting(account_id), key_id)?;
    crate::accounts::sync_open_repos(st);
    list_impl(st, store, Some(account_id))
}

pub(crate) fn check_permissions_impl(
    st: &AppState,
    store: &KeyStore,
    key_id: &str,
) -> AppResult<SshKeyPermissions> {
    let key = key_of(&st.storage, key_id)?;
    let path = key
        .private_key_path
        .clone()
        .ok_or_else(|| AppError::InvalidInput("the key has no private key file".into()))?;
    let report = check_permissions(Path::new(&path)).map_err(ssh_err)?;
    Ok(SshKeyPermissions {
        key_id: key.id.clone(),
        restricted: report.restricted,
        entries: report.entries,
        problem: report.problem,
        repairable: is_managed(store, &key),
    })
}

pub(crate) fn repair_permissions_impl(
    st: &AppState,
    store: &KeyStore,
    key_id: &str,
) -> AppResult<SshKeyPermissions> {
    let key = key_of(&st.storage, key_id)?;
    if !is_managed(store, &key) {
        return Err(AppError::Unsupported(
            "only keys stored by the app can be repaired".into(),
        ));
    }
    store.repair_permissions(&key.id).map_err(ssh_err)?;
    check_permissions_impl(st, store, key_id)
}

fn provider_kind(kind: AccountProviderKind) -> ProviderKind {
    match kind {
        AccountProviderKind::Github => ProviderKind::Github,
        AccountProviderKind::GithubEnterprise => ProviderKind::GithubEnterprise,
        AccountProviderKind::Gitlab => ProviderKind::Gitlab,
        AccountProviderKind::GitlabSelfHosted => ProviderKind::GitlabSelfHosted,
    }
}

/// Everything an upload needs, read from SQLite + keychain before the network call.
pub(crate) struct UploadPlan {
    key: SshKey,
    endpoint: ProviderEndpoint,
    token: Option<SecretToken>,
}

pub(crate) fn upload_plan(st: &AppState, key_id: &str) -> AppResult<UploadPlan> {
    let key = key_of(&st.storage, key_id)?;
    let account_id = key
        .account_id
        .clone()
        .ok_or_else(|| AppError::InvalidInput("assign the key to an account first".into()))?;
    let account = account_of(&st.storage, &account_id)?;
    let provider = provider_of(&st.storage, &account)?;
    let endpoint = ProviderEndpoint::custom(
        provider_kind(provider.kind),
        &provider.base_url,
        &provider.api_url,
    )
    .map_err(|e| AppError::InvalidInput(e.to_string()))?;
    let token = account_secret(st, &account.id, AccountSecret::Token)?
        .filter(|t| !t.is_empty())
        .map(|t| SecretToken::new(t.as_str()));
    Ok(UploadPlan {
        key,
        endpoint,
        token,
    })
}

/// Network part of the upload.
pub(crate) enum UploadOutcome {
    Registered(String),
    AlreadyRegistered(Option<String>),
    Result(SshUploadResult),
}

pub(crate) async fn upload_network(plan: &UploadPlan, title: &str) -> AppResult<UploadOutcome> {
    let Some(token) = plan.token.clone() else {
        return Ok(UploadOutcome::Result(SshUploadResult::NoToken));
    };
    let http =
        build_http_client(&HttpConfig::default()).map_err(|e| AppError::Internal(e.to_string()))?;
    let client = ProviderClient::new(http, plan.endpoint.clone(), token);
    let public_key = plan.key.public_key.trim();
    match client.add_ssh_key(title, public_key).await {
        Ok(k) => Ok(UploadOutcome::Registered(k.id)),
        Err(ProviderError::AlreadyExists { .. }) => {
            // On this account, or on someone else's?
            let keys = client.list_ssh_keys().await.map_err(provider_err)?;
            match keys.iter().find(|k| k.matches(public_key)) {
                Some(k) => Ok(UploadOutcome::AlreadyRegistered(Some(k.id.clone()))),
                None => Ok(UploadOutcome::Result(SshUploadResult::UsedByAnotherAccount)),
            }
        }
        Err(e) => match upload_error_result(&e) {
            Some(r) => Ok(UploadOutcome::Result(r)),
            None => Err(provider_err(e)),
        },
    }
}

/// Typed results for errors the user can act on.
fn upload_error_result(e: &ProviderError) -> Option<SshUploadResult> {
    match (e, e.hint()) {
        (ProviderError::Unauthorized { .. }, _) => Some(SshUploadResult::Unauthorized),
        (_, Some(AccessHint::MissingScopes { required, granted })) => {
            Some(SshUploadResult::MissingScope {
                required: required.clone(),
                granted: granted.clone(),
            })
        }
        (_, Some(AccessHint::TokenPermissionMissing)) => {
            Some(SshUploadResult::TokenPermissionMissing)
        }
        // Without a hint a 403/404 on POST /user/keys still means "not allowed".
        (ProviderError::Forbidden { .. } | ProviderError::NotFound { .. }, None) => {
            Some(SshUploadResult::MissingScope {
                required: Vec::new(),
                granted: None,
            })
        }
        _ => None,
    }
}

fn provider_err(e: ProviderError) -> AppError {
    match e {
        ProviderError::Network { .. } | ProviderError::Timeout => {
            AppError::Internal(format!("could not reach the provider: {e}"))
        }
        other => AppError::Internal(other.to_string()),
    }
}

pub(crate) fn upload_finish(
    st: &AppState,
    store: &KeyStore,
    plan: UploadPlan,
    outcome: UploadOutcome,
) -> AppResult<SshUploadResult> {
    let (provider_key_id, already) = match outcome {
        UploadOutcome::Result(r) => return Ok(r),
        UploadOutcome::Registered(id) => (Some(id), false),
        UploadOutcome::AlreadyRegistered(id) => (id, true),
    };
    // Re-read: the key may have changed during the request.
    let mut key = key_of(&st.storage, &plan.key.id)?;
    key.uploaded_to_provider = true;
    key.provider_key_id = provider_key_id;
    let key = st.storage.update_ssh_key(&key).map_err(storage_err)?;
    tracing::info!(key = %key.id, already, "SSH key registered on the provider");
    let key = describe(st, store, key)?;
    Ok(if already {
        SshUploadResult::AlreadyRegistered { key }
    } else {
        SshUploadResult::Uploaded { key }
    })
}

/// Parses `ssh -T git@host` output: GitHub `Hi <user>! You've successfully authenticated…`,
/// GitLab `Welcome to GitLab, @<user>!`, Bitbucket/Gitea variants, and the usual failures.
pub fn parse_ssh_test_output(output: &str) -> (SshTestStatus, Option<String>) {
    for line in output.lines() {
        let line = line.trim();
        // GitHub (and GitHub Enterprise): "Hi octocat! You've successfully authenticated, …"
        if let Some(rest) = line.strip_prefix("Hi ")
            && let Some((user, tail)) = rest.split_once('!')
            && tail.contains("authenticated")
            && !user.trim().is_empty()
        {
            return (SshTestStatus::Authenticated, Some(user.trim().to_owned()));
        }
        // GitLab: "Welcome to GitLab, @alice!"
        if let Some(rest) = line.strip_prefix("Welcome to GitLab, ") {
            let user = rest
                .trim_start_matches('@')
                .trim_end_matches('!')
                .trim()
                .to_owned();
            if !user.is_empty() {
                return (SshTestStatus::Authenticated, Some(user));
            }
        }
        // Gitea / Forgejo: "Hi there, alice! You've successfully authenticated with the key …"
        if let Some(rest) = line.strip_prefix("Hi there, ")
            && let Some((user, _)) = rest.split_once('!')
        {
            return (SshTestStatus::Authenticated, Some(user.trim().to_owned()));
        }
    }
    let lower = output.to_ascii_lowercase();
    let status = if lower.contains("permission denied") {
        if lower.contains("passphrase") || lower.contains("incorrect passphrase") {
            SshTestStatus::PassphraseNeeded
        } else {
            SshTestStatus::Denied
        }
    } else if lower.contains("host key verification failed")
        || lower.contains("remote host identification has changed")
        || lower.contains("host key for")
    {
        SshTestStatus::HostKeyFailed
    } else if lower.contains("could not resolve hostname")
        || lower.contains("connection timed out")
        || lower.contains("operation timed out")
        || lower.contains("connection refused")
        || lower.contains("network is unreachable")
        || lower.contains("no route to host")
        || lower.contains("connection closed by")
        || lower.contains("timed out")
    {
        SshTestStatus::Network
    } else if lower.contains("enter passphrase") || lower.contains("bad passphrase") {
        SshTestStatus::PassphraseNeeded
    } else {
        SshTestStatus::Unknown
    };
    (status, None)
}

/// `ssh` used by git: Git for Windows' own (`<git>/usr/bin/ssh.exe`, OpenSSH ≥ 8.4 so
/// `SSH_ASKPASS_REQUIRE=force` works) rather than the older Windows OpenSSH in System32.
fn ssh_program(st: &AppState) -> PathBuf {
    #[cfg(windows)]
    {
        let out = st
            .git
            .command(
                &std::env::temp_dir(),
                &AccountContext::default(),
                ["--exec-path"],
            )
            .output();
        if let Ok(out) = out
            && out.status.success()
        {
            let exec = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
            // <root>/mingw64/libexec/git-core → <root>/usr/bin/ssh.exe
            if let Some(root) = exec.ancestors().nth(3) {
                let ssh = root.join("usr").join("bin").join("ssh.exe");
                if ssh.is_file() {
                    return ssh;
                }
            }
        }
    }
    let _ = st;
    PathBuf::from("ssh")
}

/// Host to test: the provider's web host (port dropped — SSH uses 22).
fn ssh_host(provider: &Provider) -> AppResult<String> {
    let host = provider
        .host()
        .ok_or_else(|| AppError::InvalidInput("the provider has no host".into()))?;
    // `[::1]:8443` / `gitlab.acme:8443` → host only.
    let host = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or(rest).to_owned()
    } else {
        host.split(':').next().unwrap_or(&host).to_owned()
    };
    Ok(host)
}

fn run_with_timeout(mut cmd: Command, timeout: Duration) -> AppResult<String> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::Internal(format!("could not run ssh: {e}")))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(AppError::Internal(e.to_string())),
        }
    }
    let out = child
        .wait_with_output()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let mut text = String::from_utf8_lossy(&out.stderr).into_owned();
    let stdout = String::from_utf8_lossy(&out.stdout);
    if !stdout.trim().is_empty() {
        text.push('\n');
        text.push_str(&stdout);
    }
    Ok(nanomgg_logging::redact(text.trim()).into_owned())
}

pub(crate) fn test_connection_impl(
    st: &AppState,
    account_id: &str,
    key_id: Option<&str>,
) -> AppResult<SshTestResult> {
    let account = account_of(&st.storage, account_id)?;
    let provider = provider_of(&st.storage, &account)?;
    let key = match key_id {
        Some(id) => {
            let k = key_of(&st.storage, id)?;
            if k.account_id.as_deref() != Some(account_id) {
                return Err(AppError::InvalidInput(format!(
                    "SSH key `{id}` does not belong to account `{account_id}`"
                )));
            }
            k
        }
        None => default_key(&st.storage, account_id)?
            .ok_or_else(|| AppError::InvalidInput("the account has no SSH key yet".into()))?,
    };
    let key_path = key
        .private_key_path
        .clone()
        .filter(|p| Path::new(p).is_file())
        .ok_or_else(|| AppError::PathNotFound("the private key file is missing".into()))?;
    let host = ssh_host(&provider)?;
    // Same plumbing as git: askpass env (sidecar + NANOMGG_*), app known_hosts, this key only.
    let ctx = crate::accounts::context_for_account(st, account_id, None)?;
    let env: Vec<(String, String)> = ctx
        .env()
        .into_iter()
        .filter(|(k, _)| k.starts_with("SSH_ASKPASS") || k.starts_with("NANOMGG_"))
        .collect();
    let opts = SshCommandOptions {
        key_path: Some(PathBuf::from(&key_path)),
        known_hosts_file: st.accounts.known_hosts.clone(),
        host_key_policy: if st.accounts.known_hosts.is_some() {
            HostKeyPolicy::AcceptNew
        } else {
            HostKeyPolicy::SshDefault
        },
        connect_timeout_secs: Some(15),
        ..SshCommandOptions::default()
    };
    let mut cmd = Command::new(ssh_program(st));
    cmd.args(ssh_args(&opts))
        .args(["-T", "-o", "LogLevel=ERROR"])
        .envs(env);
    if !ctx.ssh.as_ref().is_some_and(|s| s.askpass.is_some()) {
        // No sidecar: never hang on a passphrase prompt.
        cmd.args(["-o", "BatchMode=yes"]);
    }
    // Options must precede the destination (anything after it is the remote command).
    cmd.arg(format!("git@{host}"));
    let output = run_with_timeout(cmd, Duration::from_secs(30))?;
    let (status, username) = parse_ssh_test_output(&output);
    Ok(build_test_result(
        status, username, &account, host, key.id, output,
    ))
}

fn build_test_result(
    status: SshTestStatus,
    username: Option<String>,
    account: &Account,
    host: String,
    key_id: String,
    output: String,
) -> SshTestResult {
    let deploy_key = username.as_deref().is_some_and(|u| u.contains('/'));
    let matches_account = !deploy_key
        && username
            .as_deref()
            .is_some_and(|u| u.eq_ignore_ascii_case(&account.username));
    SshTestResult {
        status,
        host,
        username,
        expected_username: account.username.clone(),
        matches_account,
        deploy_key,
        key_id,
        output,
    }
}

// ----- commands ------------------------------------------------------------------------------

fn key_store(app: &AppHandle) -> AppResult<KeyStore> {
    Ok(KeyStore::new(&crate::app_paths::data_dir(app)?))
}

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppState, &KeyStore) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    let store = key_store(&app)?;
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>(), &store))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Keys of one account, or every key (also unassigned ones) when `accountId` is `null`.
#[tauri::command]
#[specta::specta]
pub async fn ssh_key_list(
    app: AppHandle,
    account_id: Option<String>,
) -> AppResult<Vec<SshKeyInfo>> {
    blocking(app, move |st, store| {
        list_impl(st, store, account_id.as_deref())
    })
    .await
}

/// New Ed25519 key for the account; an optional passphrase is saved in the OS keychain.
#[tauri::command]
#[specta::specta]
pub async fn ssh_key_generate(
    app: AppHandle,
    account_id: String,
    comment: Option<String>,
    passphrase: Option<String>,
) -> AppResult<SshKeyInfo> {
    let passphrase = passphrase.map(Zeroizing::new);
    blocking(app, move |st, store| {
        generate_impl(st, store, &account_id, comment, passphrase)
    })
    .await
}

/// Copies an existing OpenSSH private key into the app key store (the file is not modified).
#[tauri::command]
#[specta::specta]
pub async fn ssh_key_import(
    app: AppHandle,
    account_id: String,
    path: String,
    passphrase: Option<String>,
) -> AppResult<SshImportResult> {
    let passphrase = passphrase.map(Zeroizing::new);
    blocking(app, move |st, store| {
        import_impl(st, store, Some(&account_id), Path::new(&path), passphrase)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn ssh_key_delete(app: AppHandle, key_id: String) -> AppResult<bool> {
    blocking(app, move |st, store| delete_impl(st, store, &key_id)).await
}

/// The key git uses for the account's SSH remotes. Returns the account's keys.
#[tauri::command]
#[specta::specta]
pub async fn ssh_key_set_default(
    app: AppHandle,
    account_id: String,
    key_id: String,
) -> AppResult<Vec<SshKeyInfo>> {
    blocking(app, move |st, store| {
        set_default_impl(st, store, &account_id, &key_id)
    })
    .await
}

/// Registers the public key on the account's provider with its token.
#[tauri::command]
#[specta::specta]
pub async fn ssh_key_upload(
    app: AppHandle,
    key_id: String,
    title: String,
) -> AppResult<SshUploadResult> {
    let plan = blocking(app.clone(), move |st, _| upload_plan(st, &key_id)).await?;
    let title = match title.trim() {
        "" => "NANOMultipleGitGUI".to_owned(),
        t => t.to_owned(),
    };
    let outcome = upload_network(&plan, &title).await?;
    blocking(app, move |st, store| {
        upload_finish(st, store, plan, outcome)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn ssh_key_check_permissions(
    app: AppHandle,
    key_id: String,
) -> AppResult<SshKeyPermissions> {
    blocking(app, move |st, store| {
        check_permissions_impl(st, store, &key_id)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn ssh_key_repair_permissions(
    app: AppHandle,
    key_id: String,
) -> AppResult<SshKeyPermissions> {
    blocking(app, move |st, store| {
        repair_permissions_impl(st, store, &key_id)
    })
    .await
}

/// `ssh -T git@<host>` with the account's key (or `keyId`): which user does it authenticate as?
#[tauri::command]
#[specta::specta]
pub async fn ssh_test_connection(
    app: AppHandle,
    account_id: String,
    key_id: Option<String>,
) -> AppResult<SshTestResult> {
    blocking(app, move |st, _| {
        test_connection_impl(st, &account_id, key_id.as_deref())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::commands::{
        AccountInput, ProviderInput, account_save_impl, account_save_token_impl, provider_save_impl,
    };
    use nanomgg_accounts::AuthMethod;
    use nanomgg_credentials::{MemorySecretStore, SecretStore};
    use std::sync::Arc;

    struct Env {
        tmp: tempfile::TempDir,
        st: AppState,
        store: KeyStore,
        secrets: Arc<MemorySecretStore>,
        account: String,
    }

    fn env_with(kind: AccountProviderKind, base_url: &str, api_url: Option<String>) -> Env {
        let tmp = tempfile::tempdir().unwrap();
        let storage = Arc::new(Storage::open(&tmp.path().join("app.db")).unwrap());
        let secrets = Arc::new(MemorySecretStore::default());
        let rt = crate::accounts::AccountsRuntime::new(None, None, secrets.clone());
        let st = AppState::with_accounts(storage, rt);
        let provider = provider_save_impl(
            &st,
            ProviderInput {
                id: None,
                kind,
                name: "Test".into(),
                base_url: base_url.into(),
                api_url,
                oauth_client_id: None,
                large_file_warn_mb: None,
                large_file_block_mb: None,
            },
        )
        .unwrap();
        let account = account_save_impl(
            &st,
            AccountInput {
                id: None,
                provider_id: provider.id,
                username: "alice".into(),
                display_name: None,
                avatar_url: None,
                auth_method: AuthMethod::Pat,
                scopes: vec![],
                token_expires_at: None,
                color: None,
                label: None,
                default_clone_dir: None,
            },
        )
        .unwrap();
        let store = KeyStore::new(&tmp.path().join("appdata"));
        Env {
            tmp,
            st,
            store,
            secrets,
            account: account.id,
        }
    }

    fn env() -> Env {
        env_with(
            AccountProviderKind::GithubEnterprise,
            "https://ghe.example.test",
            None,
        )
    }

    #[test]
    fn generate_with_passphrase_stores_key_file_and_keychain_entry() {
        let e = env();
        let key = generate_impl(
            &e.st,
            &e.store,
            &e.account,
            None,
            Some(Zeroizing::new("s3cret pass".into())),
        )
        .unwrap();
        assert_eq!(key.key_type, "ed25519");
        assert!(key.fingerprint.starts_with("SHA256:"));
        assert!(key.public_key.starts_with("ssh-ed25519 "));
        assert_eq!(key.comment.as_deref(), Some("alice@ghe.example.test"));
        assert!(key.has_passphrase && key.passphrase_saved);
        assert!(key.managed && key.file_exists);
        assert!(
            key.is_default && key.explicit_default,
            "first key becomes default"
        );
        let path = PathBuf::from(key.private_key_path.as_deref().unwrap());
        assert!(path.starts_with(e.store.dir()));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("OPENSSH PRIVATE KEY"));
        assert_eq!(
            e.secrets
                .get(&SecretKey::ssh_passphrase(&key.id))
                .unwrap()
                .as_deref()
                .map(String::as_str),
            Some("s3cret pass")
        );
        // The resolver uses it.
        assert_eq!(
            default_key_path(&e.st.storage, &e.account).unwrap(),
            Some(path.clone())
        );

        // A second key does not steal the default; "Set default" does.
        let second = generate_impl(&e.st, &e.store, &e.account, Some("work".into()), None).unwrap();
        assert!(!second.is_default && !second.has_passphrase && !second.passphrase_saved);
        let list = set_default_impl(&e.st, &e.store, &e.account, &second.id).unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.iter().find(|k| k.id == second.id).unwrap().is_default);
        assert!(!list.iter().find(|k| k.id == key.id).unwrap().is_default);

        // Delete: files, keychain entry, record and default setting go away.
        assert!(delete_impl(&e.st, &e.store, &second.id).unwrap());
        assert!(!Path::new(second.private_key_path.as_deref().unwrap()).exists());
        let list = list_impl(&e.st, &e.store, Some(&e.account)).unwrap();
        assert_eq!(list.len(), 1);
        assert!(
            list[0].is_default && !list[0].explicit_default,
            "implicit fallback"
        );
        assert!(delete_impl(&e.st, &e.store, &key.id).unwrap());
        assert!(!path.exists());
        assert!(e.secrets.is_empty());
        assert!(!delete_impl(&e.st, &e.store, &key.id).unwrap());
        assert_eq!(default_key_path(&e.st.storage, &e.account).unwrap(), None);
    }

    #[test]
    fn import_copies_without_touching_the_original() {
        let e = env();
        let user_dir = e.tmp.path().join("home").join(".ssh");
        std::fs::create_dir_all(&user_dir).unwrap();
        let original = user_dir.join("id_work");
        let generated = generate_ed25519("me@work", Some("pw")).unwrap();
        std::fs::write(&original, generated.private_key.as_bytes()).unwrap();
        std::fs::write(
            user_dir.join("id_work.pub"),
            format!("{}\n", generated.info.public_key),
        )
        .unwrap();
        let before = std::fs::read(&original).unwrap();

        let r = import_impl(&e.st, &e.store, Some(&e.account), &original, None).unwrap();
        assert!(matches!(r, SshImportResult::PassphraseRequired), "{r:?}");
        let r = import_impl(
            &e.st,
            &e.store,
            Some(&e.account),
            &original,
            Some(Zeroizing::new("nope".into())),
        )
        .unwrap();
        assert!(matches!(r, SshImportResult::WrongPassphrase), "{r:?}");
        let r = import_impl(
            &e.st,
            &e.store,
            Some(&e.account),
            &original,
            Some(Zeroizing::new("pw".into())),
        )
        .unwrap();
        let SshImportResult::Imported {
            key,
            public_file_mismatch,
        } = r
        else {
            panic!("{r:?}")
        };
        assert!(!public_file_mismatch);
        assert_eq!(key.fingerprint, generated.info.fingerprint);
        assert_eq!(key.comment.as_deref(), Some("me@work"));
        assert!(key.has_passphrase && key.passphrase_saved && key.managed && key.is_default);
        assert_ne!(
            PathBuf::from(key.private_key_path.as_deref().unwrap()),
            original
        );
        assert_eq!(
            std::fs::read(&original).unwrap(),
            before,
            "original untouched"
        );

        // Importing again is reported, not duplicated.
        let again = import_impl(
            &e.st,
            &e.store,
            Some(&e.account),
            &original,
            Some(Zeroizing::new("pw".into())),
        )
        .unwrap();
        assert!(
            matches!(&again, SshImportResult::AlreadyKnown { account_id: Some(a), .. } if *a == e.account),
            "{again:?}"
        );

        // Deleting removes only the app copy.
        assert!(delete_impl(&e.st, &e.store, &key.id).unwrap());
        assert_eq!(std::fs::read(&original).unwrap(), before);

        // Public keys and garbage are refused.
        let err = import_impl(
            &e.st,
            &e.store,
            Some(&e.account),
            &user_dir.join("id_work.pub"),
            None,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)), "{err:?}");
    }

    #[test]
    fn unassigned_import_is_adopted_by_an_account() {
        let e = env();
        let file = e.tmp.path().join("id_plain");
        let generated = generate_ed25519("plain", None).unwrap();
        std::fs::write(&file, generated.private_key.as_bytes()).unwrap();
        let SshImportResult::Imported { key, .. } =
            import_impl(&e.st, &e.store, None, &file, None).unwrap()
        else {
            panic!()
        };
        assert_eq!(key.account_id, None);
        assert!(!key.is_default);
        let SshImportResult::Imported { key: adopted, .. } =
            import_impl(&e.st, &e.store, Some(&e.account), &file, None).unwrap()
        else {
            panic!()
        };
        assert_eq!(adopted.id, key.id);
        assert_eq!(adopted.account_id.as_deref(), Some(e.account.as_str()));
        assert!(adopted.is_default);
    }

    #[test]
    fn permissions_check_and_repair_for_managed_keys() {
        let e = env();
        let key = generate_impl(&e.st, &e.store, &e.account, None, None).unwrap();
        let report = check_permissions_impl(&e.st, &e.store, &key.id).unwrap();
        assert!(report.restricted, "{report:?}");
        assert!(report.repairable);
        let repaired = repair_permissions_impl(&e.st, &e.store, &key.id).unwrap();
        assert!(repaired.restricted);
    }

    #[test]
    fn parses_ssh_test_output() {
        let gh = "Hi octocat! You've successfully authenticated, but GitHub does not provide shell access.";
        assert_eq!(
            parse_ssh_test_output(gh),
            (SshTestStatus::Authenticated, Some("octocat".into()))
        );
        let deploy = "Hi acme/api! You've successfully authenticated, but GitHub does not provide shell access.";
        assert_eq!(parse_ssh_test_output(deploy).1.as_deref(), Some("acme/api"));
        let gl = "Warning: Permanently added 'gitlab.com' (ED25519) to the list of known hosts.\r\nWelcome to GitLab, @alice.w!\r\n";
        assert_eq!(
            parse_ssh_test_output(gl),
            (SshTestStatus::Authenticated, Some("alice.w".into()))
        );
        assert_eq!(
            parse_ssh_test_output("git@github.com: Permission denied (publickey).").0,
            SshTestStatus::Denied
        );
        assert_eq!(
            parse_ssh_test_output("Host key verification failed.").0,
            SshTestStatus::HostKeyFailed
        );
        assert_eq!(
            parse_ssh_test_output(
                "ssh: Could not resolve hostname github.com: No such host is known."
            )
            .0,
            SshTestStatus::Network
        );
        assert_eq!(
            parse_ssh_test_output("ssh: connect to host github.com port 22: Connection timed out")
                .0,
            SshTestStatus::Network
        );
        assert_eq!(parse_ssh_test_output("").0, SshTestStatus::Unknown);

        let e = env();
        let acc = account_of(&e.st.storage, &e.account).unwrap();
        let ok = build_test_result(
            SshTestStatus::Authenticated,
            Some("Alice".into()),
            &acc,
            "ghe.example.test".into(),
            "key_1".into(),
            String::new(),
        );
        assert!(ok.matches_account && !ok.deploy_key);
        let other = build_test_result(
            SshTestStatus::Authenticated,
            Some("bob".into()),
            &acc,
            "ghe.example.test".into(),
            "key_1".into(),
            String::new(),
        );
        assert!(!other.matches_account);
        let deploy = build_test_result(
            SshTestStatus::Authenticated,
            Some("alice/repo".into()),
            &acc,
            "h".into(),
            "k".into(),
            String::new(),
        );
        assert!(deploy.deploy_key && !deploy.matches_account);
    }

    #[test]
    fn ssh_host_drops_port() {
        let mut p = Provider::github_com();
        assert_eq!(ssh_host(&p).unwrap(), "github.com");
        p.base_url = "https://gitlab.acme.dev:8443".into();
        assert_eq!(ssh_host(&p).unwrap(), "gitlab.acme.dev");
    }

    mod upload {
        use super::*;
        use wiremock::matchers::{body_json, header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        fn run<T>(f: impl std::future::Future<Output = T>) -> T {
            tauri::async_runtime::block_on(f)
        }

        fn upload(e: &Env, key_id: &str) -> SshUploadResult {
            let plan = upload_plan(&e.st, key_id).unwrap();
            let outcome = run(upload_network(&plan, "NANO laptop")).unwrap();
            upload_finish(&e.st, &e.store, plan, outcome).unwrap()
        }

        fn github_env(server: &MockServer) -> Env {
            let base = format!("http://127.0.0.1:{}", server.address().port());
            env_with(
                AccountProviderKind::GithubEnterprise,
                &base,
                Some(format!("{base}/api/v3")),
            )
        }

        fn with_token(e: &Env) {
            account_save_token_impl(
                &e.st,
                &e.account,
                AccountSecret::Token,
                Zeroizing::new("ghp_test_token".into()),
            )
            .unwrap();
        }

        #[test]
        fn uploads_and_records_provider_key_id() {
            let server = run(MockServer::start());
            let e = github_env(&server);
            with_token(&e);
            let key = generate_impl(&e.st, &e.store, &e.account, Some("me".into()), None).unwrap();
            run(Mock::given(method("POST"))
                .and(path("/api/v3/user/keys"))
                .and(header("authorization", "Bearer ghp_test_token"))
                .and(body_json(
                    serde_json::json!({"title": "NANO laptop", "key": key.public_key}),
                ))
                .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                    "id": 42, "key": "ssh-ed25519 AAAA", "title": "NANO laptop"
                })))
                .mount(&server));
            let r = upload(&e, &key.id);
            let SshUploadResult::Uploaded { key: up } = r else {
                panic!("{r:?}")
            };
            assert!(up.uploaded_to_provider);
            assert_eq!(up.provider_key_id.as_deref(), Some("42"));
            let stored = e.st.storage.ssh_key(&key.id).unwrap().unwrap();
            assert!(stored.uploaded_to_provider);
        }

        #[test]
        fn already_registered_on_this_or_another_account() {
            let server = run(MockServer::start());
            let e = github_env(&server);
            with_token(&e);
            let key = generate_impl(&e.st, &e.store, &e.account, None, None).unwrap();
            let other = generate_impl(&e.st, &e.store, &e.account, None, None).unwrap();
            run(Mock::given(method("POST"))
                .and(path("/api/v3/user/keys"))
                .respond_with(ResponseTemplate::new(422).set_body_json(serde_json::json!({
                    "message": "Validation Failed",
                    "errors": [{"resource": "PublicKey", "code": "custom", "field": "key",
                                "message": "key is already in use"}]
                })))
                .mount(&server));
            let core: Vec<&str> = key.public_key.split_whitespace().take(2).collect();
            run(Mock::given(method("GET"))
                .and(path("/api/v3/user/keys"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                    {"id": 7, "key": core.join(" "), "title": "old"}
                ])))
                .mount(&server));
            let r = upload(&e, &key.id);
            assert!(
                matches!(&r, SshUploadResult::AlreadyRegistered { key } if key.provider_key_id.as_deref() == Some("7")),
                "{r:?}"
            );
            let r = upload(&e, &other.id);
            assert!(matches!(r, SshUploadResult::UsedByAnotherAccount), "{r:?}");
            assert!(
                !e.st
                    .storage
                    .ssh_key(&other.id)
                    .unwrap()
                    .unwrap()
                    .uploaded_to_provider
            );
        }

        #[test]
        fn missing_scope_token_and_unauthorized_are_typed() {
            let server = run(MockServer::start());
            let e = github_env(&server);
            let key = generate_impl(&e.st, &e.store, &e.account, None, None).unwrap();
            // No token saved yet.
            assert!(matches!(upload(&e, &key.id), SshUploadResult::NoToken));

            with_token(&e);
            run(Mock::given(method("POST"))
                .and(path("/api/v3/user/keys"))
                .respond_with(
                    ResponseTemplate::new(404)
                        .insert_header("x-oauth-scopes", "repo, read:user")
                        .insert_header(
                            "x-accepted-oauth-scopes",
                            "admin:public_key, write:public_key",
                        )
                        .set_body_json(serde_json::json!({"message": "Not Found"})),
                )
                .up_to_n_times(1)
                .mount(&server));
            let r = upload(&e, &key.id);
            let SshUploadResult::MissingScope { required, granted } = &r else {
                panic!("{r:?}")
            };
            assert!(required.iter().any(|s| s == "write:public_key"), "{r:?}");
            assert!(
                granted
                    .as_ref()
                    .is_some_and(|g| g.iter().any(|s| s == "repo"))
            );

            run(Mock::given(method("POST"))
                .and(path("/api/v3/user/keys"))
                .respond_with(
                    ResponseTemplate::new(401)
                        .set_body_json(serde_json::json!({"message": "Bad credentials"})),
                )
                .mount(&server));
            assert!(matches!(upload(&e, &key.id), SshUploadResult::Unauthorized));
            assert!(
                !e.st
                    .storage
                    .ssh_key(&key.id)
                    .unwrap()
                    .unwrap()
                    .uploaded_to_provider
            );
        }

        #[test]
        fn gitlab_upload_uses_api_v4() {
            let server = run(MockServer::start());
            let base = format!("http://127.0.0.1:{}", server.address().port());
            let e = env_with(
                AccountProviderKind::GitlabSelfHosted,
                &base,
                Some(format!("{base}/api/v4")),
            );
            with_token(&e);
            let key = generate_impl(&e.st, &e.store, &e.account, None, None).unwrap();
            run(Mock::given(method("POST"))
                .and(path("/api/v4/user/keys"))
                .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                    "id": 9, "title": "NANO laptop", "key": key.public_key,
                    "created_at": "2024-05-01T10:00:00Z"
                })))
                .mount(&server));
            let r = upload(&e, &key.id);
            assert!(
                matches!(&r, SshUploadResult::Uploaded { key } if key.provider_key_id.as_deref() == Some("9")),
                "{r:?}"
            );
        }
    }
}
