//! Publishing a repository that has no remote yet (first push of a freshly initialised project).
//!
//! The frontend drives the steps so each one shows its own progress and error:
//! [`publish_create_repo`] creates an empty repository on GitHub / GitLab with the chosen
//! account → [`publish_add_remote`] adds it as `origin` (or any name) bound to that account →
//! `remote_push` with `--set-upstream` pushes the current branch.
//!
//! [`publish_add_remote`] also serves "Add remote…" with a URL the user pastes (a repository
//! created on the web, a self-hosted server…); the account binding is then optional.

use std::path::Path;
use std::time::Duration;

use nanomgg_git_engine::{AccountContext, GitRunner};
use nanomgg_providers::publish::NewRepoOptions;
use nanomgg_providers::{Provider, ProviderError};
use tauri::{AppHandle, Manager};

use crate::accounts::storage_err;
use crate::error::{AppError, AppResult};
use crate::forks::{
    ForkOutcome, ForkRemoteChange, ForkRepo, bind_remote, check_remote_name, check_url, client,
    remote_names, remote_url,
};
use crate::repos::repo_root;
use crate::state::AppState;

/// Creates an empty repository with `account_id` (see [`NewRepoOptions`]).
#[tauri::command]
#[specta::specta]
pub async fn publish_create_repo(
    app: AppHandle,
    account_id: String,
    options: NewRepoOptions,
) -> AppResult<ForkOutcome<ForkRepo>> {
    let Some(c) = client(&app, &account_id).await? else {
        return Ok(ForkOutcome::NoToken { account_id });
    };
    let full_name = match c.create_repository(&options).await {
        Ok(n) => n,
        Err(e) => return Ok(ForkOutcome::failed(e)),
    };
    tracing::info!(repo = %full_name, "repository created for publishing");
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

/// Adds remote `name` → `url`; reusing a remote that already has this URL is not an error.
pub(crate) fn add_remote(
    git: &GitRunner,
    root: &Path,
    name: &str,
    url: &str,
) -> AppResult<ForkRemoteChange> {
    let name = name.trim();
    let url = url.trim();
    check_remote_name(name)?;
    check_url(url)?;
    if remote_names(git, root)?.iter().any(|n| n == name) {
        let existing = remote_url(git, root, name)?;
        if existing == url {
            return Ok(ForkRemoteChange {
                remote: name.to_owned(),
                url: url.to_owned(),
                added: false,
                renamed_to: None,
            });
        }
        return Err(AppError::InvalidInput(format!(
            "a remote named `{name}` already points at {existing} — choose another name"
        )));
    }
    git.run(
        root,
        &AccountContext::default(),
        ["remote", "add", name, url],
    )?;
    Ok(ForkRemoteChange {
        remote: name.to_owned(),
        url: url.to_owned(),
        added: true,
        renamed_to: None,
    })
}

/// Adds remote `name` → `url` to the open repository and, when `account_id` is given, binds
/// that remote to the account so fetch / push use its credentials.
#[tauri::command]
#[specta::specta]
pub async fn publish_add_remote(
    app: AppHandle,
    repo_id: i64,
    name: String,
    url: String,
    account_id: Option<String>,
) -> AppResult<ForkRemoteChange> {
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        let root = repo_root(&st, repo_id)?;
        let account = match account_id.as_deref().filter(|a| !a.is_empty()) {
            Some(id) => Some(
                st.storage
                    .account(id)
                    .map_err(storage_err)?
                    .ok_or_else(|| AppError::InvalidInput(format!("account `{id}` not found")))?,
            ),
            None => None,
        };
        let change = add_remote(&st.git, &root, &name, &url)?;
        if let Some(account) = &account {
            bind_remote(&st, repo_id, account, &change.remote, &change.url)?;
        }
        tracing::info!(repo = repo_id, remote = %change.remote, "remote added");
        Ok(change)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_a_remote_once() {
        let g = GitRunner::default();
        let dir = tempfile::tempdir().unwrap();
        g.run(dir.path(), &AccountContext::default(), ["init", "-q"])
            .unwrap();
        let url = "https://github.com/me/app.git";
        let first = add_remote(&g, dir.path(), "origin", url).unwrap();
        assert!(first.added);
        assert_eq!(remote_url(&g, dir.path(), "origin").unwrap(), url);
        assert!(!add_remote(&g, dir.path(), " origin ", url).unwrap().added);
        assert!(add_remote(&g, dir.path(), "origin", "https://github.com/me/x.git").is_err());
        assert!(add_remote(&g, dir.path(), "bad name", url).is_err());
        assert!(add_remote(&g, dir.path(), "up", "--upload-pack=x").is_err());
    }
}
