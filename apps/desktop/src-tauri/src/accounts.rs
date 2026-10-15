//! Multi-account runtime (PLAN §4): the account-context **resolver** used by every git command,
//! the credential server + `nanomgg-credential` sidecar wiring, and the IPC commands for
//! providers, accounts, identities, rules and repo bindings ([`commands`]).
//!
//! # Running a git command as the right account
//!
//! ```ignore
//! let ctx = crate::accounts::context_for(&state, repo_id, Some("origin"))?; // or None
//! state.git.run(&root, &ctx, ["push", "origin", "main"])?;
//! ```
//!
//! - `remote: Some(name)` for commands talking to one remote (fetch/pull/push/ls-remote…);
//!   `None` for local commands (commit, merge, stash…) — this uses the repo default, i.e. the
//!   identity for commits. An unknown remote name (e.g. pushing to a raw URL) also gets the default.
//! - An unbound repo yields `AccountContext::default()`: git runs with the user's own config,
//!   exactly like before Phase 2 (PLAN §4.3 step 5 — read-only work is never blocked).
//! - "Only for this operation" / clone: [`context_for_account`] with an explicit account
//!   ([`context_with`] picks between the two for commands taking an `asAccount` override).
//! - The context carries identity (`user.name/email` via `GIT_CONFIG_*`), the HTTPS helper
//!   (sidecar + `NANOMGG_SOCKET/SESSION/ACCOUNT`, global helpers reset) and, when the account
//!   has an SSH key file, `GIT_SSH_COMMAND` + `SSH_ASKPASS` for its passphrase. Everything is
//!   environment, so git-lfs and hooks inherit it; nothing is written to `.git/config` except
//!   the opt-out identity copy described in [`sync_repo_config`].
//!
//! Both functions are blocking (SQLite + at most one `git config` process, cached on the
//! repo's config file stamp): call them inside `spawn_blocking`, like the git command itself.
//! They lock `state.workspace()` briefly — never call them while holding that guard (std
//! mutexes are not re-entrant). Resolve right before each command rather than caching the
//! context: bindings, rules and remotes can change between commands.

pub mod commands;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use nanomgg_accounts::{
    AccountDirectory as Directory, AuthMethod, MatchOptions, ProviderKind, RemoteInfo, RemoteUrl,
    Resolution, RuleSet, SigningMethod,
};
use nanomgg_credentials::{
    AccountDirectory, AccountRef, CredentialServer, KeychainSource, MemorySecretStore, SecretStore,
    SshKeyRef,
};
use nanomgg_git_engine::{
    AccountContext, GitRunner, HttpsAuth, Identity, Signing, SigningFormat, SshAskpass, SshAuth,
};
use nanomgg_ssh_keys::{HostKeyPolicy, SshCommandOptions, git_ssh_command};
use nanomgg_storage::{Storage, StorageError};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Setting: copy the resolved identity into the repo's `.git/config` (default on, PLAN §13.1).
pub const KEY_WRITE_IDENTITY: &str = "identity.writeToRepoConfig";
/// Setting: also write `core.sshCommand` for SSH remotes (default off, opt-in).
pub const KEY_WRITE_SSH_COMMAND: &str = "ssh.writeCoreSshCommand";

/// Config file stamp used to cache a repo's remote list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
}

/// Multi-account services owned by [`AppState`].
pub struct AccountsRuntime {
    /// Serves tokens / SSH passphrases to the sidecar. `None` if it could not start (then git
    /// runs without injected credentials and falls back to the user's own helpers).
    pub credential_server: Option<CredentialServer>,
    /// `nanomgg-credential` executable; `None` if it is missing from the installation.
    pub sidecar: Option<PathBuf>,
    /// Where tokens are written (`account_save_token`): the OS keychain in the app.
    pub secrets: Arc<dyn SecretStore>,
    /// App-owned `known_hosts` (`<app data>/known_hosts`, seeded with the pinned github.com /
    /// gitlab.com keys) used with `StrictHostKeyChecking=accept-new`; `None` → ssh defaults.
    pub known_hosts: Option<PathBuf>,
    /// App data directory (`allowed_signers` for SSH signature verification lives there);
    /// `None` in tests → a temp-dir fallback.
    pub data_dir: Option<PathBuf>,
    remotes: Mutex<HashMap<PathBuf, (Stamp, Vec<RemoteInfo>)>>,
}

impl AccountsRuntime {
    /// No credential server, secrets in memory only. Used by `AppState::new` (tests, and a
    /// fallback) — commands still work, git just gets identities but no tokens.
    pub fn disabled() -> Self {
        Self::new(None, None, Arc::new(MemorySecretStore::default()))
    }

    pub fn new(
        credential_server: Option<CredentialServer>,
        sidecar: Option<PathBuf>,
        secrets: Arc<dyn SecretStore>,
    ) -> Self {
        Self {
            credential_server,
            sidecar,
            secrets,
            known_hosts: None,
            data_dir: None,
            remotes: Mutex::default(),
        }
    }

    /// Starts the credential server over `storage` + `secrets`, locates the sidecar and seeds
    /// `<data_dir>/known_hosts`. Failures are logged and degrade to "no injected credentials",
    /// never abort startup.
    pub fn start(storage: Arc<Storage>, secrets: Arc<dyn SecretStore>, data_dir: &Path) -> Self {
        let source = KeychainSource::new(Arc::new(StorageDirectory(storage)), secrets.clone());
        let server = match CredentialServer::start(Arc::new(source)) {
            Ok(server) => Some(server),
            Err(e) => {
                tracing::error!(error = %e, "credential server failed to start; HTTPS accounts disabled");
                None
            }
        };
        let sidecar = sidecar_path();
        match &sidecar {
            Some(path) => tracing::info!(sidecar = %path.display(), "credential helper found"),
            None => tracing::error!(
                "nanomgg-credential not found next to the app; HTTPS accounts disabled"
            ),
        }
        let known_hosts = data_dir.join("known_hosts");
        if let Err(e) = nanomgg_ssh_keys::seed_known_hosts(&known_hosts) {
            tracing::warn!(error = %e, "could not seed the app known_hosts file");
        }
        let mut rt = Self::new(server, sidecar, secrets);
        rt.known_hosts = Some(known_hosts);
        rt.data_dir = Some(data_dir.to_owned());
        rt
    }

    /// `GIT_SSH_COMMAND` / `core.sshCommand` for a key: only that key, no agent, the app's
    /// known_hosts with trust-on-first-use (built by `nanomgg_ssh_keys`).
    pub fn ssh_command(&self, key_path: &Path) -> String {
        git_ssh_command(&SshCommandOptions {
            key_path: Some(key_path.to_owned()),
            known_hosts_file: self.known_hosts.clone(),
            host_key_policy: if self.known_hosts.is_some() {
                HostKeyPolicy::AcceptNew
            } else {
                HostKeyPolicy::SshDefault
            },
            ..SshCommandOptions::default()
        })
    }
}

/// Locates the `nanomgg-credential` sidecar: next to the app executable (bundles, `cargo run`,
/// `tauri dev` — build.rs + tauri-build put it there) or one level up (test binaries live in
/// `target/<profile>/deps`). Debug builds honour `NANOMGG_CREDENTIAL_HELPER` as an override.
pub fn sidecar_path() -> Option<PathBuf> {
    if cfg!(debug_assertions)
        && let Some(p) = std::env::var_os("NANOMGG_CREDENTIAL_HELPER")
    {
        return Some(PathBuf::from(p));
    }
    let name = format!("nanomgg-credential{}", std::env::consts::EXE_SUFFIX);
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    [Some(dir.join(&name)), dir.parent().map(|d| d.join(&name))]
        .into_iter()
        .flatten()
        .find(|p| p.is_file())
}

/// Account metadata for the credential server, read from SQLite.
pub struct StorageDirectory(pub Arc<Storage>);

impl AccountDirectory for StorageDirectory {
    fn account(&self, account_id: &str) -> Option<AccountRef> {
        let account = self.0.account(account_id).ok()??;
        let provider = self.0.provider(&account.provider_id).ok()??;
        // GitLab accepts OAuth access tokens only with the user name `oauth2`.
        let username = if account.auth_method == AuthMethod::Oauth
            && matches!(
                provider.kind,
                ProviderKind::Gitlab | ProviderKind::GitlabSelfHosted
            ) {
            "oauth2".to_owned()
        } else {
            account.username
        };
        Some(AccountRef {
            host: provider.host()?,
            username,
        })
    }

    fn ssh_keys(&self, account_id: &str) -> Vec<SshKeyRef> {
        self.0
            .ssh_keys(Some(account_id))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|k| {
                Some(SshKeyRef {
                    key_id: k.id,
                    private_key_path: k.private_key_path?,
                })
            })
            .collect()
    }
}

/// Storage errors as user-facing IPC errors (validation/conflicts become `invalidInput`).
pub(crate) fn storage_err(e: StorageError) -> AppError {
    match e {
        StorageError::Invalid(v) => AppError::InvalidInput(v.to_string()),
        StorageError::Conflict(m) => AppError::InvalidInput(m),
        e @ StorageError::NotFound { .. } => AppError::InvalidInput(e.to_string()),
        other => other.into(),
    }
}

fn config_path(git_dir: &Path) -> PathBuf {
    // Linked worktrees keep the shared config in the common dir.
    match std::fs::read_to_string(git_dir.join("commondir")) {
        Ok(common) => git_dir.join(common.trim()).join("config"),
        Err(_) => git_dir.join("config"),
    }
}

fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some(Stamp {
        modified: meta.modified().ok(),
        len: meta.len(),
    })
}

/// Parses `git config -z --get-regexp '^remote\..*\.url$'`.
fn parse_remote_urls(raw: &[u8]) -> Vec<RemoteInfo> {
    let mut out: Vec<RemoteInfo> = Vec::new();
    for entry in raw.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let text = String::from_utf8_lossy(entry);
        let Some((key, url)) = text.split_once('\n') else {
            continue;
        };
        let Some(name) = key
            .strip_prefix("remote.")
            .and_then(|k| k.strip_suffix(".url"))
        else {
            continue;
        };
        // Multiple urls for one remote: git fetches from the first.
        if !out.iter().any(|r| r.name == name) {
            out.push(RemoteInfo {
                name: name.to_owned(),
                url: url.to_owned(),
            });
        }
    }
    out
}

fn read_remotes(git: &GitRunner, root: &Path) -> AppResult<Vec<RemoteInfo>> {
    let out = git
        .command(
            root,
            &AccountContext::default(),
            ["config", "-z", "--get-regexp", r"^remote\..*\.url$"],
        )
        .output()
        .map_err(|e| AppError::Git(e.to_string()))?;
    // Exit status 1 = no remote configured.
    if !out.status.success() && out.status.code() != Some(1) {
        return Err(AppError::Git(
            String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        ));
    }
    Ok(parse_remote_urls(&out.stdout))
}

/// Remotes of the repo at `root`, cached until its config file changes.
pub fn repo_remotes(st: &AppState, root: &Path) -> AppResult<Vec<RemoteInfo>> {
    let git_dir = st
        .workspace()
        .repos
        .values()
        .find(|r| r.root == root)
        .map(|r| r.git_dir.clone())
        .unwrap_or_else(|| root.join(".git"));
    let current = stamp(&config_path(&git_dir));
    if let Some(s) = current
        && let Some((cached, remotes)) = st
            .accounts
            .remotes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(root)
        && *cached == s
    {
        return Ok(remotes.clone());
    }
    let remotes = read_remotes(&st.git, root)?;
    if let Some(s) = current {
        st.accounts
            .remotes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(root.to_owned(), (s, remotes.clone()));
    }
    Ok(remotes)
}

fn match_options() -> MatchOptions {
    MatchOptions {
        home_dir: std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .map(|h| h.to_string_lossy().into_owned()),
        ..MatchOptions::default()
    }
}

/// Everything the resolver needs from SQLite, loaded once per resolution.
pub(crate) struct Snapshot {
    pub accounts: Vec<nanomgg_accounts::Account>,
    pub providers: Vec<nanomgg_accounts::Provider>,
    pub identities: Vec<nanomgg_accounts::Identity>,
    pub rules: RuleSet,
}

impl Snapshot {
    pub fn load(storage: &Storage) -> AppResult<Self> {
        Ok(Self {
            accounts: storage.accounts().map_err(storage_err)?,
            providers: storage.providers().map_err(storage_err)?,
            identities: storage.identities(None).map_err(storage_err)?,
            rules: RuleSet::new(storage.rules().map_err(storage_err)?, &match_options()),
        })
    }

    pub fn dir(&self) -> Directory<'_> {
        Directory {
            accounts: &self.accounts,
            providers: &self.providers,
            identities: &self.identities,
        }
    }
}

/// Resolves the repo's default and per-remote accounts (PLAN §4.3).
pub fn resolve_repo(st: &AppState, repo_id: i64, root: &Path) -> AppResult<Resolution> {
    let remotes = repo_remotes(st, root)?;
    let bindings = st.storage.repo_bindings(repo_id).map_err(storage_err)?;
    let snap = Snapshot::load(&st.storage)?;
    Ok(nanomgg_accounts::resolve(
        &root.to_string_lossy(),
        &remotes,
        &bindings,
        &snap.rules,
        &snap.dir(),
    ))
}

/// **The resolver used by every git command**: the [`AccountContext`] for `remote` (or the
/// repo default when `None`) of open repo `repo_id`. See the module docs for usage.
pub fn context_for(st: &AppState, repo_id: i64, remote: Option<&str>) -> AppResult<AccountContext> {
    let root = crate::repos::repo_root(st, repo_id)?;
    let resolution = resolve_repo(st, repo_id, &root)?;
    match resolution.for_remote(remote) {
        Some(r) => context_for_account(st, &r.account_id, r.identity_id.as_deref()),
        None => Ok(AccountContext::default()),
    }
}

/// "Only for this operation" (PLAN §4.5): run one commit/push/pull/fetch as another account
/// without touching the repo binding. Sent by the switcher's one-shot override (`asAccount`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountOverride {
    pub account_id: String,
    /// Must belong to the account; omitted → its default identity.
    #[serde(default)]
    #[specta(optional)]
    pub identity_id: Option<String>,
}

/// [`context_for_account`] for a one-shot override, else [`context_for`]. The account must
/// exist; for remote commands the credential server still only answers for the account's
/// provider host, so an override never leaks a token to another host.
pub fn context_with(
    st: &AppState,
    repo_id: i64,
    remote: Option<&str>,
    over: Option<&AccountOverride>,
) -> AppResult<AccountContext> {
    match over {
        Some(o) => context_for_account(
            st,
            &o.account_id,
            o.identity_id.as_deref().filter(|id| !id.is_empty()),
        ),
        None => context_for(st, repo_id, remote),
    }
}

/// Context for an explicit account ("Commit as…/Push as…" one-off, clone). `identity_id`
/// must belong to the account; `None` picks its default identity.
pub fn context_for_account(
    st: &AppState,
    account_id: &str,
    identity_id: Option<&str>,
) -> AppResult<AccountContext> {
    let account = st
        .storage
        .account(account_id)
        .map_err(storage_err)?
        .ok_or_else(|| AppError::InvalidInput(format!("account `{account_id}` not found")))?;
    let identities = st
        .storage
        .identities(Some(&account.id))
        .map_err(storage_err)?;
    if let Some(wanted) = identity_id
        && !identities.iter().any(|i| i.id == wanted)
    {
        return Err(AppError::InvalidInput(format!(
            "identity `{wanted}` does not belong to account `{account_id}`"
        )));
    }
    let dir = Directory {
        accounts: std::slice::from_ref(&account),
        providers: &[],
        identities: &identities,
    };
    let chosen = dir.identity_for(&account.id, identity_id);
    let identity = chosen.map(|i| Identity {
        name: i.name.clone(),
        email: i.email.clone(),
    });
    let signing = chosen.and_then(identity_signing);

    let rt = &st.accounts;
    let session_env = match (&rt.credential_server, &rt.sidecar) {
        (Some(server), Some(sidecar)) => Some((server.env_for(&account.id), sidecar.clone())),
        _ => None,
    };
    let https = session_env.as_ref().map(|(env, sidecar)| HttpsAuth {
        helper_path: sidecar.clone(),
        env: env.clone(),
    });
    let ssh = account_ssh_key(&st.storage, &account.id)?.map(|key_path| SshAuth {
        command: Some(rt.ssh_command(&key_path)),
        key_path,
        askpass: session_env.as_ref().map(|(env, sidecar)| SshAskpass {
            program: sidecar.clone(),
            env: env.clone(),
        }),
    });
    Ok(AccountContext {
        identity,
        https,
        ssh,
        signing,
    })
}

/// Commit signing configured on an identity (`commit.gpgsign`, `gpg.format`,
/// `user.signingkey`); `None` when the identity does not sign.
pub fn identity_signing(identity: &nanomgg_accounts::Identity) -> Option<Signing> {
    let format = match identity.signing {
        SigningMethod::None => return None,
        SigningMethod::Gpg => SigningFormat::OpenPgp,
        SigningMethod::Ssh => SigningFormat::Ssh,
    };
    nanomgg_git_engine::signature::signing_for(format, identity.signing_key_ref.as_deref()?)
}

/// The account's default SSH key (setting `ssh.defaultKey.<account>`, see [`crate::ssh_keys`]),
/// else its first SSH key whose private key file exists.
fn account_ssh_key(storage: &Storage, account_id: &str) -> AppResult<Option<PathBuf>> {
    crate::ssh_keys::default_key_path(storage, account_id)
}

/// Flags controlling what [`sync_repo_config`] writes.
pub fn write_flags(storage: &Storage) -> AppResult<(bool, bool)> {
    Ok((
        storage.get_setting(KEY_WRITE_IDENTITY)?.unwrap_or(true),
        storage.get_setting(KEY_WRITE_SSH_COMMAND)?.unwrap_or(false),
    ))
}

fn set_local_config(git: &GitRunner, root: &Path, key: &str, value: &str) -> AppResult<bool> {
    let none = AccountContext::default();
    let current = git
        .run_text(root, &none, ["config", "--local", "--get", key])
        .ok();
    if current.as_deref() == Some(value) {
        return Ok(false);
    }
    git.run(root, &none, ["config", "--local", key, value])?;
    Ok(true)
}

/// "Ghi vào repo" (PLAN §4.4): copies the resolved identity into `.git/config` (setting
/// `identity.writeToRepoConfig`, default on) and, for an SSH primary remote with a key file,
/// `core.sshCommand` (setting `ssh.writeCoreSshCommand`, default off) so git outside the app
/// uses the same identity/key. Never writes tokens; leaves the config alone for unbound repos.
/// Only writes keys whose value differs. Runs after binding / identity / rule / setting changes
/// and right before a commit — not on reads such as `repo_account_context`.
/// Returns whether anything changed.
pub fn sync_repo_config(st: &AppState, repo_id: i64) -> AppResult<bool> {
    let (write_identity, write_ssh) = write_flags(&st.storage)?;
    if !write_identity && !write_ssh {
        return Ok(false);
    }
    let root = crate::repos::repo_root(st, repo_id)?;
    let resolution = resolve_repo(st, repo_id, &root)?;
    let Some(default) = resolution.default.as_ref() else {
        return Ok(false);
    };
    let mut changed = false;
    if write_identity
        && let Some(id) = &default.identity_id
        && let Some(identity) = st.storage.identity(id).map_err(storage_err)?
    {
        changed |= set_local_config(&st.git, &root, "user.name", &identity.name)?;
        changed |= set_local_config(&st.git, &root, "user.email", &identity.email)?;
    }
    if write_ssh {
        let primary = resolution
            .remotes
            .iter()
            .find(|r| r.remote == "origin")
            .or_else(|| resolution.remotes.first());
        if let Some(remote) = primary
            && remote.parsed.as_ref().is_some_and(RemoteUrl::is_ssh)
            && let Some(resolved) = &remote.resolved
            && let Some(key) = account_ssh_key(&st.storage, &resolved.account_id)?
        {
            changed |= set_local_config(
                &st.git,
                &root,
                "core.sshCommand",
                &st.accounts.ssh_command(&key),
            )?;
        }
    }
    Ok(changed)
}

/// Re-syncs every open repo (after identity/rule changes). Errors are logged per repo.
pub fn sync_open_repos(st: &AppState) {
    let ids: Vec<i64> = st.workspace().repos.keys().copied().collect();
    for id in ids {
        if let Err(e) = sync_repo_config(st, id) {
            tracing::warn!(repo = id, error = %e, "could not update repo identity config");
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn parses_remote_urls_with_dots_and_duplicates() {
        let raw = b"remote.origin.url\nhttps://github.com/a/b.git\0remote.my.fork.url\ngit@github.com:me/b.git\0remote.origin.url\nhttps://mirror/a/b.git\0";
        let remotes = parse_remote_urls(raw);
        assert_eq!(
            remotes,
            vec![
                RemoteInfo {
                    name: "origin".into(),
                    url: "https://github.com/a/b.git".into()
                },
                RemoteInfo {
                    name: "my.fork".into(),
                    url: "git@github.com:me/b.git".into()
                },
            ]
        );
        assert!(parse_remote_urls(b"").is_empty());
    }
}
