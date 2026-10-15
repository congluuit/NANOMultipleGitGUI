//! Forks (PLAN §4.7 "Fork & clone", Phase 4.3).
//!
//! The frontend drives the steps so each one shows its own progress and error:
//!
//! - **Fork & clone** (clone dialog): [`fork_create`] (returns an existing fork of the account
//!   instead of forking again) → [`fork_wait`] (GitHub copies the data asynchronously; cancel
//!   with `transfer_cancel(opId)`) → `clone_start` with the fork's URL and the fork account
//!   (binding `source = clone`, as for any clone) → [`fork_add_upstream`]: `upstream` = the
//!   source, in the same transport (HTTPS / SSH) as `origin`.
//! - **Fork the open repository**: [`fork_create`] → [`fork_wait`] → [`fork_attach`] adds the
//!   fork as a new remote (default `fork`) or renames `origin` → `upstream` and adds the fork as
//!   `origin`, and binds the fork remote to the fork account (locked, PLAN §4.3 step 1) → the
//!   frontend fetches it with `remote_fetch`.
//!
//! Provider failures come back as [`ForkOutcome::Failed`] with the typed [`ProviderError`]
//! (scope / SAML / OAuth-app hints) and a recognised [`ForkProblem`], so the UI can explain the
//! fix instead of showing raw text.

use std::path::Path;
use std::time::Duration;

use nanomgg_accounts::{AccountDirectory, BindingSource, RepoBinding, Transport};
use nanomgg_git_engine::{AccountContext, GitRunner};
use nanomgg_providers::forks::{ForkNamespace, ForkOptions, ForkProblem, fork_problem};
use nanomgg_providers::{CancellationToken, Provider, ProviderClient, ProviderError, Repository};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::accounts::storage_err;
use crate::branches::Registration;
use crate::clone::url_transport;
use crate::error::{AppError, AppResult};
use crate::hosting::account_client;
use crate::repos::repo_root;
use crate::state::AppState;

/// How long [`fork_wait`] polls before telling the user the fork is still being copied.
const WAIT_TIMEOUT: Duration = Duration::from_secs(120);
const WAIT_INTERVAL: Duration = Duration::from_secs(2);

/// Result of a fork command.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ForkOutcome<T> {
    Done {
        value: T,
    },
    /// The account has no token stored: sign in again.
    #[serde(rename_all = "camelCase")]
    NoToken {
        account_id: String,
    },
    /// The provider refused or failed.
    #[serde(rename_all = "camelCase")]
    Failed {
        error: ProviderError,
        problem: Option<ForkProblem>,
    },
}

impl<T> ForkOutcome<T> {
    fn failed(error: ProviderError) -> Self {
        Self::Failed {
            problem: fork_problem(&error),
            error,
        }
    }
}

/// A fork (new or existing) of the account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ForkRepo {
    pub full_name: String,
    pub clone_https: String,
    pub clone_ssh: String,
    pub web_url: String,
    pub default_branch: Option<String>,
    pub private: bool,
    /// The account already had this fork: nothing was created.
    pub existed: bool,
}

impl ForkRepo {
    fn new(r: Repository, existed: bool) -> Self {
        Self {
            full_name: r.full_name,
            clone_https: r.clone_https,
            clone_ssh: r.clone_ssh,
            web_url: r.web_url,
            default_branch: r.default_branch,
            private: r.private,
            existed,
        }
    }
}

/// Remote changes made by [`fork_add_upstream`] / [`fork_attach`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ForkRemoteChange {
    /// The remote that was added (`upstream` after a clone, the fork remote otherwise).
    pub remote: String,
    pub url: String,
    /// `false` when the remote already existed with this URL.
    pub added: bool,
    /// `origin` was renamed to this (swap mode).
    pub renamed_to: Option<String>,
}

/// What [`fork_attach`] does with the remotes of the open repository.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ForkAttachMode {
    /// Add the fork as a new remote (`fork` by default); `origin` stays as it is.
    AddRemote { name: String },
    /// Rename `origin` → `upstream`, then add the fork as `origin` (branches keep tracking the
    /// source through `upstream`).
    SwapOrigin,
}

async fn client(app: &AppHandle, account_id: &str) -> AppResult<Option<ProviderClient>> {
    let app = app.clone();
    let id = account_id.to_owned();
    match tauri::async_runtime::spawn_blocking(move || account_client(&app, &id))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    {
        Ok(c) => Ok(Some(c)),
        // No token stored (see `account_client`).
        Err(AppError::Unsupported(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

macro_rules! client_or_return {
    ($app:expr, $account:expr) => {
        match client($app, $account).await? {
            Some(c) => c,
            None => {
                return Ok(ForkOutcome::NoToken {
                    account_id: $account.to_owned(),
                });
            }
        }
    };
}

/// Where `account_id` may fork to: itself first, then organizations / groups.
#[tauri::command]
#[specta::specta]
pub async fn fork_namespaces(
    app: AppHandle,
    account_id: String,
) -> AppResult<ForkOutcome<Vec<ForkNamespace>>> {
    let c = client_or_return!(&app, &account_id);
    Ok(match c.fork_namespaces().await {
        Ok(v) => ForkOutcome::Done { value: v },
        Err(e) => ForkOutcome::failed(e),
    })
}

/// The account's existing fork of `source` in `namespace` (`None` = the account itself).
#[tauri::command]
#[specta::specta]
pub async fn fork_find(
    app: AppHandle,
    account_id: String,
    source: String,
    namespace: Option<String>,
) -> AppResult<ForkOutcome<Option<ForkRepo>>> {
    let c = client_or_return!(&app, &account_id);
    Ok(match c.find_fork(&source, namespace.as_deref()).await {
        Ok(v) => ForkOutcome::Done {
            value: v.map(|r| ForkRepo::new(r, true)),
        },
        Err(e) => ForkOutcome::failed(e),
    })
}

/// Forks `source` with `account_id` (or returns the account's existing fork in that namespace).
/// The data may still be copying: call [`fork_wait`] before cloning / fetching.
#[tauri::command]
#[specta::specta]
pub async fn fork_create(
    app: AppHandle,
    account_id: String,
    source: String,
    options: ForkOptions,
) -> AppResult<ForkOutcome<ForkRepo>> {
    let c = client_or_return!(&app, &account_id);
    match c.find_fork(&source, options.namespace.as_deref()).await {
        Ok(Some(existing)) => {
            return Ok(ForkOutcome::Done {
                value: ForkRepo::new(existing, true),
            });
        }
        Ok(None) => {}
        Err(e) => return Ok(ForkOutcome::failed(e)),
    }
    let full_name = match c.create_fork(&source, &options).await {
        Ok(n) => n,
        Err(e) => return Ok(ForkOutcome::failed(e)),
    };
    tracing::info!(fork = %full_name, "fork requested");
    // The new repository can take a moment to become visible to the API.
    let mut tries = 0;
    loop {
        match c.repository(&full_name).await {
            Ok(repo) => {
                return Ok(ForkOutcome::Done {
                    value: ForkRepo::new(repo, false),
                });
            }
            Err(ProviderError::NotFound { .. }) if tries < 5 => {
                tries += 1;
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(e) => return Ok(ForkOutcome::failed(e)),
        }
    }
}

/// Waits (≤ 2 minutes) until the fork `full_name` can be cloned. `false` = still copying.
/// Cancel with `transfer_cancel(opId)`.
#[tauri::command]
#[specta::specta]
pub async fn fork_wait(
    app: AppHandle,
    op_id: String,
    account_id: String,
    full_name: String,
) -> AppResult<ForkOutcome<bool>> {
    let (registration, token) = Registration::new(&op_id)?;
    let c = client_or_return!(&app, &account_id);
    let cancel = CancellationToken::new();
    // Bridges `transfer_cancel` (an atomic flag) to the async wait.
    let watcher = {
        let cancel = cancel.clone();
        tauri::async_runtime::spawn(async move {
            while !cancel.is_cancelled() {
                if token.is_cancelled() {
                    cancel.cancel();
                    break;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        })
    };
    let result = c
        .wait_fork_ready(&full_name, WAIT_TIMEOUT, WAIT_INTERVAL, &cancel)
        .await;
    cancel.cancel();
    let _ = watcher.await;
    drop(registration);
    Ok(match result {
        Ok(ready) => ForkOutcome::Done { value: ready },
        Err(e) => ForkOutcome::failed(e),
    })
}

// ----- remotes -------------------------------------------------------------------------------

fn remote_names(git: &GitRunner, root: &Path) -> AppResult<Vec<String>> {
    Ok(git
        .run_text(root, &AccountContext::default(), ["remote"])?
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect())
}

fn remote_url(git: &GitRunner, root: &Path, name: &str) -> AppResult<String> {
    Ok(git.run_text(
        root,
        &AccountContext::default(),
        ["remote", "get-url", name],
    )?)
}

/// `ssh` URL when `like` is an SSH remote, else `https`. Empty candidates fall back to the other.
fn same_transport<'a>(like: Option<&str>, https: &'a str, ssh: &'a str) -> &'a str {
    let wants_ssh = like.and_then(url_transport) == Some(Transport::Ssh);
    match (wants_ssh, https.trim().is_empty(), ssh.trim().is_empty()) {
        (true, _, false) | (false, true, false) => ssh.trim(),
        _ => https.trim(),
    }
}

/// A usable remote name: `[A-Za-z0-9._-]`, not starting with `-` or `.`.
fn check_remote_name(name: &str) -> AppResult<()> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with(['-', '.'])
        && !name.ends_with(".lock")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(())
    } else {
        Err(AppError::InvalidInput(format!(
            "`{name}` is not a valid remote name (letters, digits, `.`, `_`, `-`)"
        )))
    }
}

fn check_url(url: &str) -> AppResult<()> {
    if url.is_empty() || url.starts_with('-') || url.chars().any(char::is_control) {
        return Err(AppError::InvalidInput("invalid remote URL".into()));
    }
    Ok(())
}

/// Adds `upstream` (= the fork's source) to a freshly cloned fork, in `origin`'s transport.
pub(crate) fn add_upstream(
    git: &GitRunner,
    root: &Path,
    upstream_https: &str,
    upstream_ssh: &str,
) -> AppResult<ForkRemoteChange> {
    let origin = remote_url(git, root, "origin").ok();
    let url = same_transport(origin.as_deref(), upstream_https, upstream_ssh).to_owned();
    check_url(&url)?;
    let names = remote_names(git, root)?;
    if names.iter().any(|n| n == "upstream") {
        let existing = remote_url(git, root, "upstream")?;
        if existing == url {
            return Ok(ForkRemoteChange {
                remote: "upstream".into(),
                url,
                added: false,
                renamed_to: None,
            });
        }
        return Err(AppError::InvalidInput(format!(
            "a remote named `upstream` already points at {existing}"
        )));
    }
    git.run(
        root,
        &AccountContext::default(),
        ["remote", "add", "upstream", &url],
    )?;
    Ok(ForkRemoteChange {
        remote: "upstream".into(),
        url,
        added: true,
        renamed_to: None,
    })
}

/// Adds the fork to the open repository; `source_remote` is the remote that was forked (its
/// transport is reused).
pub(crate) fn attach_fork(
    git: &GitRunner,
    root: &Path,
    source_remote: &str,
    fork_https: &str,
    fork_ssh: &str,
    mode: &ForkAttachMode,
) -> AppResult<ForkRemoteChange> {
    let names = remote_names(git, root)?;
    if !names.iter().any(|n| n == source_remote) {
        return Err(AppError::InvalidInput(format!(
            "remote `{source_remote}` does not exist"
        )));
    }
    let source_url = remote_url(git, root, source_remote)?;
    let url = same_transport(Some(&source_url), fork_https, fork_ssh).to_owned();
    check_url(&url)?;
    let none = AccountContext::default();
    match mode {
        ForkAttachMode::AddRemote { name } => {
            let name = name.trim();
            check_remote_name(name)?;
            if names.iter().any(|n| n == name) {
                if remote_url(git, root, name)? == url {
                    return Ok(ForkRemoteChange {
                        remote: name.to_owned(),
                        url,
                        added: false,
                        renamed_to: None,
                    });
                }
                return Err(AppError::InvalidInput(format!(
                    "a remote named `{name}` already exists — choose another name"
                )));
            }
            git.run(root, &none, ["remote", "add", name, &url])?;
            Ok(ForkRemoteChange {
                remote: name.to_owned(),
                url,
                added: true,
                renamed_to: None,
            })
        }
        ForkAttachMode::SwapOrigin => {
            if source_remote != "origin" {
                return Err(AppError::InvalidInput(
                    "only `origin` can be swapped with the fork".into(),
                ));
            }
            if names.iter().any(|n| n == "upstream") {
                return Err(AppError::InvalidInput(
                    "a remote named `upstream` already exists — add the fork as a new remote instead"
                        .into(),
                ));
            }
            // `git remote rename` also moves the branches' tracking configuration.
            git.run(root, &none, ["remote", "rename", "origin", "upstream"])?;
            if let Err(e) = git.run(root, &none, ["remote", "add", "origin", &url]) {
                let _ = git.run(root, &none, ["remote", "rename", "upstream", "origin"]);
                return Err(e.into());
            }
            Ok(ForkRemoteChange {
                remote: "origin".into(),
                url,
                added: true,
                renamed_to: Some("upstream".into()),
            })
        }
    }
}

/// After cloning a fork: `upstream` = the source (same transport as `origin`), then a
/// best-effort fetch of it is left to the frontend (`remote_fetch`).
#[tauri::command]
#[specta::specta]
pub async fn fork_add_upstream(
    app: AppHandle,
    repo_id: i64,
    upstream_https: String,
    upstream_ssh: String,
) -> AppResult<ForkRemoteChange> {
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        let root = repo_root(&st, repo_id)?;
        add_upstream(&st.git, &root, &upstream_https, &upstream_ssh)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Adds the fork of the open repository as a remote (see [`ForkAttachMode`]) and binds that
/// remote to `account_id`. Fetch it afterwards with `remote_fetch`.
#[tauri::command]
#[specta::specta]
pub async fn fork_attach(
    app: AppHandle,
    repo_id: i64,
    account_id: String,
    source_remote: String,
    fork_https: String,
    fork_ssh: String,
    mode: ForkAttachMode,
) -> AppResult<ForkRemoteChange> {
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        let root = repo_root(&st, repo_id)?;
        let account = st
            .storage
            .account(&account_id)
            .map_err(storage_err)?
            .ok_or_else(|| AppError::InvalidInput(format!("account `{account_id}` not found")))?;
        let change = attach_fork(
            &st.git,
            &root,
            &source_remote,
            &fork_https,
            &fork_ssh,
            &mode,
        )?;
        if let Some(renamed) = &change.renamed_to
            && let Some(old) = st
                .storage
                .repo_binding(repo_id, Some("origin"))
                .map_err(storage_err)?
        {
            // The source keeps its account under its new name.
            st.storage
                .set_repo_binding(&RepoBinding {
                    remote_name: Some(renamed.clone()),
                    ..old
                })
                .map_err(storage_err)?;
        }
        let identities = st
            .storage
            .identities(Some(&account.id))
            .map_err(storage_err)?;
        let dir = AccountDirectory {
            accounts: std::slice::from_ref(&account),
            providers: &[],
            identities: &identities,
        };
        let identity = dir.identity_for(&account.id, None).map(|i| i.id.clone());
        st.storage
            .set_repo_binding(&RepoBinding {
                repo_id,
                remote_name: Some(change.remote.clone()),
                account_id: account.id.clone(),
                identity_id: identity,
                transport: url_transport(&change.url),
                source: BindingSource::Manual,
                // A repo-wide locked binding would otherwise win (PLAN §4.3 step 1).
                locked: true,
                updated_at: 0,
            })
            .map_err(storage_err)?;
        if let Err(e) = crate::accounts::sync_repo_config(&st, repo_id) {
            tracing::warn!(repo = repo_id, error = %e, "could not sync identity after fork");
        }
        Ok(change)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(root: &Path, args: &[&str]) {
        GitRunner::default()
            .run(root, &AccountContext::default(), args)
            .unwrap();
    }

    fn repo_with_origin(url: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        git(dir.path(), &["remote", "add", "origin", url]);
        git(dir.path(), &["config", "branch.main.remote", "origin"]);
        git(
            dir.path(),
            &["config", "branch.main.merge", "refs/heads/main"],
        );
        dir
    }

    fn url_of(root: &Path, name: &str) -> String {
        remote_url(&GitRunner::default(), root, name).unwrap()
    }

    #[test]
    fn picks_the_transport_of_the_reference_remote() {
        let (h, s) = ("https://github.com/o/r.git", "git@github.com:o/r.git");
        assert_eq!(same_transport(Some("git@github.com:me/r.git"), h, s), s);
        assert_eq!(same_transport(Some("ssh://git@host/me/r.git"), h, s), s);
        assert_eq!(same_transport(Some("https://github.com/me/r"), h, s), h);
        assert_eq!(same_transport(None, h, s), h);
        assert_eq!(same_transport(Some("git@github.com:me/r.git"), h, ""), h);
        assert_eq!(same_transport(None, "", s), s);
    }

    #[test]
    fn remote_names_are_checked() {
        assert!(check_remote_name("fork").is_ok());
        assert!(check_remote_name("my-fork_2.x").is_ok());
        for bad in ["", "-x", ".x", "a b", "a/b", "x.lock", "a\nb"] {
            assert!(check_remote_name(bad).is_err(), "{bad:?}");
        }
        assert!(check_url("--upload-pack=x").is_err());
    }

    #[test]
    fn upstream_follows_origin_transport_and_is_idempotent() {
        let g = GitRunner::default();
        let dir = repo_with_origin("git@github.com:me/lib.git");
        let up = add_upstream(
            &g,
            dir.path(),
            "https://github.com/org/lib.git",
            "git@github.com:org/lib.git",
        )
        .unwrap();
        assert!(up.added);
        assert_eq!(url_of(dir.path(), "upstream"), "git@github.com:org/lib.git");
        let again = add_upstream(
            &g,
            dir.path(),
            "https://github.com/org/lib.git",
            "git@github.com:org/lib.git",
        )
        .unwrap();
        assert!(!again.added);
        assert!(add_upstream(&g, dir.path(), "https://github.com/x/y.git", "").is_err());
    }

    #[test]
    fn attach_adds_a_fork_remote() {
        let g = GitRunner::default();
        let dir = repo_with_origin("https://github.com/org/lib.git");
        let mode = ForkAttachMode::AddRemote {
            name: "fork".into(),
        };
        let change = attach_fork(
            &g,
            dir.path(),
            "origin",
            "https://github.com/me/lib.git",
            "git@github.com:me/lib.git",
            &mode,
        )
        .unwrap();
        assert_eq!(change.remote, "fork");
        assert_eq!(url_of(dir.path(), "fork"), "https://github.com/me/lib.git");
        assert_eq!(
            url_of(dir.path(), "origin"),
            "https://github.com/org/lib.git"
        );
        // Same URL again: nothing to do; another URL under that name: refused.
        assert!(
            !attach_fork(
                &g,
                dir.path(),
                "origin",
                "https://github.com/me/lib.git",
                "",
                &mode
            )
            .unwrap()
            .added
        );
        assert!(
            attach_fork(
                &g,
                dir.path(),
                "origin",
                "https://github.com/me/x.git",
                "",
                &mode
            )
            .is_err()
        );
        assert!(attach_fork(&g, dir.path(), "nope", "https://a/b.git", "", &mode).is_err());
    }

    #[test]
    fn swap_moves_origin_to_upstream_with_tracking() {
        let g = GitRunner::default();
        let dir = repo_with_origin("git@github.com:org/lib.git");
        let change = attach_fork(
            &g,
            dir.path(),
            "origin",
            "https://github.com/me/lib.git",
            "git@github.com:me/lib.git",
            &ForkAttachMode::SwapOrigin,
        )
        .unwrap();
        assert_eq!(change.renamed_to.as_deref(), Some("upstream"));
        assert_eq!(url_of(dir.path(), "origin"), "git@github.com:me/lib.git");
        assert_eq!(url_of(dir.path(), "upstream"), "git@github.com:org/lib.git");
        let tracking = g
            .run_text(
                dir.path(),
                &AccountContext::default(),
                ["config", "branch.main.remote"],
            )
            .unwrap();
        assert_eq!(tracking, "upstream");
        // A second swap is refused (upstream exists now).
        assert!(
            attach_fork(
                &g,
                dir.path(),
                "origin",
                "https://github.com/me/lib.git",
                "",
                &ForkAttachMode::SwapOrigin
            )
            .is_err()
        );
    }
}
