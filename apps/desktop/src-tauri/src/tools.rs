//! Less common git tools: bisect, the reflog ("find a lost commit"), patch files, sparse
//! checkout and repository statistics, plus the overview of every open repository.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use nanomgg_git_engine::bisect::{self, BisectMark, BisectState};
use nanomgg_git_engine::ops::{self, OpError, OpOutcome, RepoOperation};
use nanomgg_git_engine::patch::{self, ApplyMode, PatchInfo, PatchSource};
use nanomgg_git_engine::reflog::{self, ReflogEntry};
use nanomgg_git_engine::sparse::{self, SparseState};
use nanomgg_git_engine::stats::{self, RepoStats};
use nanomgg_git_engine::status::read_status;
use nanomgg_git_engine::{AccountContext, GitRunner};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::branches::{Outcome, run_op};
use crate::error::{AppError, AppResult};
use crate::repos::repo_root;
use crate::state::AppState;

/// Runs a read-only engine call on a blocking thread.
async fn read<T: Send + 'static>(
    app: AppHandle,
    repo_id: i64,
    f: impl FnOnce(&GitRunner, &Path) -> Result<T, OpError> + Send + 'static,
) -> AppResult<T> {
    let repo = repo_root(&app.state::<AppState>(), repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        match f(&st.git, &repo) {
            Ok(value) => Ok(value),
            Err(OpError::Git(e)) => Err(e.into()),
            Err(e) => Err(AppError::Git(
                nanomgg_logging::redact(&e.to_string()).into_owned(),
            )),
        }
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

fn absolute(path: &str) -> Result<PathBuf, OpError> {
    let p = PathBuf::from(path);
    if p.is_absolute() {
        Ok(p)
    } else {
        Err(OpError::rejected(
            ops::FailureKind::InvalidInput,
            format!("not an absolute path: {path:?}"),
        ))
    }
}

/// The bisect in progress (`active: false` when there is none).
#[tauri::command]
#[specta::specta]
pub async fn bisect_state(app: AppHandle, repo_id: i64) -> AppResult<BisectState> {
    read(app, repo_id, bisect::state).await
}

/// Starts a bisect between a broken commit (`bad`) and an older working one (`good`).
#[tauri::command]
#[specta::specta]
pub async fn bisect_start(
    app: AppHandle,
    repo_id: i64,
    bad: String,
    good: String,
) -> AppResult<Outcome<BisectState>> {
    run_op(app, repo_id, move |git, repo, ctx| {
        bisect::start(git, repo, ctx, &bad, &good)
    })
    .await
}

/// Says whether the commit being tested works; git moves on to the next candidate.
#[tauri::command]
#[specta::specta]
pub async fn bisect_mark(
    app: AppHandle,
    repo_id: i64,
    mark: BisectMark,
) -> AppResult<Outcome<BisectState>> {
    run_op(app, repo_id, move |git, repo, ctx| {
        bisect::mark(git, repo, ctx, mark)
    })
    .await
}

/// Ends the bisect and returns to the branch it started from.
#[tauri::command]
#[specta::specta]
pub async fn bisect_reset(app: AppHandle, repo_id: i64) -> AppResult<Outcome<()>> {
    run_op(app, repo_id, bisect::reset).await
}

/// Where HEAD has been, newest first.
#[tauri::command]
#[specta::specta]
pub async fn reflog_list(app: AppHandle, repo_id: i64, limit: u32) -> AppResult<Vec<ReflogEntry>> {
    read(app, repo_id, move |git, repo| {
        Ok(reflog::read(git, repo, limit.clamp(1, 2000))?)
    })
    .await
}

/// Branches checked out recently according to the reflog (also on other machines' clones
/// of this folder, before the app was installed), newest first.
#[tauri::command]
#[specta::specta]
pub async fn reflog_recent_branches(
    app: AppHandle,
    repo_id: i64,
    limit: u32,
) -> AppResult<Vec<String>> {
    read(app, repo_id, move |git, repo| {
        let entries = reflog::read(git, repo, 500)?;
        Ok(reflog::recent_branches(
            &entries,
            limit.clamp(1, 50) as usize,
        ))
    })
    .await
}

/// Saves commit `oid` as a patch file at the absolute path `dest`.
#[tauri::command]
#[specta::specta]
pub async fn patch_save_commit(
    app: AppHandle,
    repo_id: i64,
    oid: String,
    dest: String,
) -> AppResult<Outcome<()>> {
    run_op(app, repo_id, move |git, repo, _| {
        patch::save_commit(git, repo, &oid, &absolute(&dest)?)
    })
    .await
}

/// Saves the uncommitted (or staged) changes as a patch file at the absolute path `dest`.
#[tauri::command]
#[specta::specta]
pub async fn patch_save_changes(
    app: AppHandle,
    repo_id: i64,
    source: PatchSource,
    dest: String,
) -> AppResult<Outcome<()>> {
    run_op(app, repo_id, move |git, repo, _| {
        patch::save_changes(git, repo, source, &absolute(&dest)?)
    })
    .await
}

/// What the patch file contains and whether it applies right now.
#[tauri::command]
#[specta::specta]
pub async fn patch_inspect(
    app: AppHandle,
    repo_id: i64,
    file: String,
) -> AppResult<Outcome<PatchInfo>> {
    run_op(app, repo_id, move |git, repo, _| {
        patch::inspect(git, repo, &absolute(&file)?)
    })
    .await
}

/// Applies the patch file as changes or as commits; conflicts come back in the outcome.
#[tauri::command]
#[specta::specta]
pub async fn patch_apply(
    app: AppHandle,
    repo_id: i64,
    file: String,
    mode: ApplyMode,
) -> AppResult<Outcome<OpOutcome>> {
    run_op(app, repo_id, move |git, repo, ctx| {
        patch::apply(git, repo, ctx, &absolute(&file)?, mode)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn sparse_state(app: AppHandle, repo_id: i64) -> AppResult<SparseState> {
    read(app, repo_id, sparse::state).await
}

/// Keeps only `folders` on disk (cone mode).
#[tauri::command]
#[specta::specta]
pub async fn sparse_set(
    app: AppHandle,
    repo_id: i64,
    folders: Vec<String>,
) -> AppResult<Outcome<SparseState>> {
    run_op(app, repo_id, move |git, repo, ctx| {
        sparse::set(git, repo, ctx, &folders)
    })
    .await
}

/// Turns sparse checkout off; every file comes back.
#[tauri::command]
#[specta::specta]
pub async fn sparse_disable(app: AppHandle, repo_id: i64) -> AppResult<Outcome<SparseState>> {
    run_op(app, repo_id, sparse::disable).await
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Authors, weekly activity and most changed files of the history reachable from HEAD.
#[tauri::command]
#[specta::specta]
pub async fn repo_stats(app: AppHandle, repo_id: i64) -> AppResult<RepoStats> {
    read(app, repo_id, |git, repo| {
        Ok(stats::read(git, repo, now_secs())?)
    })
    .await
}

/// State of one open repository, for the "All repositories" overview.
#[derive(Debug, Clone, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RepoOverview {
    pub repo_id: i64,
    pub name: String,
    pub path: String,
    /// The folder is gone (moved, deleted, unplugged drive).
    pub missing: bool,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// Files with uncommitted changes (staged, unstaged and untracked).
    pub changes: u32,
    pub conflicts: u32,
    pub stashes: u32,
    pub operation: Option<RepoOperation>,
    /// Why the state could not be read.
    pub error: Option<String>,
}

fn overview_of(git: &GitRunner, root: &Path, mut row: RepoOverview) -> RepoOverview {
    match read_status(git, root) {
        Ok(status) => {
            let mut paths: Vec<&str> = status
                .staged
                .iter()
                .chain(&status.unstaged)
                .map(|e| e.path.as_str())
                .collect();
            paths.sort_unstable();
            paths.dedup();
            row.changes = paths.len() as u32;
            row.conflicts = status.conflicted.len() as u32;
            row.branch = status.branch;
            row.upstream = status.upstream;
            row.ahead = status.ahead;
            row.behind = status.behind;
        }
        Err(e) => {
            row.error = Some(nanomgg_logging::redact(&e.to_string()).into_owned());
            return row;
        }
    }
    row.stashes = git
        .run_text(
            root,
            &AccountContext::default(),
            ["stash", "list", "--format=%H"],
        )
        .map_or(0, |out| {
            out.lines().filter(|l| !l.is_empty()).count() as u32
        });
    row.operation = ops::current_operation(git, root).ok().flatten();
    row
}

/// Branch, ahead / behind, uncommitted changes and stashes of every open repository (tab
/// order). Reads only local state: run a fetch first to learn about new remote commits.
#[tauri::command]
#[specta::specta]
pub async fn workspace_overview(app: AppHandle) -> AppResult<Vec<RepoOverview>> {
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        let tabs: Vec<(i64, Option<PathBuf>)> = {
            let ws = st.workspace();
            ws.tabs
                .iter()
                .map(|id| (*id, ws.repos.get(id).map(|r| r.root.clone())))
                .collect()
        };
        let mut rows = Vec::with_capacity(tabs.len());
        for (id, root) in &tabs {
            let Some(repo) = st.storage.repo(*id)? else {
                continue;
            };
            rows.push((
                RepoOverview {
                    repo_id: *id,
                    name: repo.name,
                    path: repo.path,
                    missing: root.is_none(),
                    ..Default::default()
                },
                root.clone(),
            ));
        }
        let git = &st.git;
        Ok(std::thread::scope(|scope| {
            let handles: Vec<_> = rows
                .into_iter()
                .map(|(row, root)| {
                    scope.spawn(move || match root {
                        Some(root) => overview_of(git, &root, row),
                        None => row,
                    })
                })
                .collect();
            handles
                .into_iter()
                .filter_map(|h| h.join().ok())
                .collect::<Vec<_>>()
        }))
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

const KEY_SERVER_LIMITS: &str = "guard.serverLimits";
/// Hosts remembered at most (a typo'd or one-off host must not grow the list for ever).
const MAX_LIMIT_HOSTS: usize = 200;

/// File size limits learned from servers that refused a push, by host.
#[tauri::command]
#[specta::specta]
pub async fn server_limits(app: AppHandle) -> AppResult<HashMap<String, u64>> {
    let st = app.state::<AppState>();
    Ok(st
        .storage
        .get_setting(KEY_SERVER_LIMITS)?
        .unwrap_or_default())
}

/// Remembers the smallest file size limit `host` reported; returns every known limit.
#[tauri::command]
#[specta::specta]
pub async fn server_limit_learn(
    app: AppHandle,
    host: String,
    bytes: u64,
) -> AppResult<HashMap<String, u64>> {
    let st = app.state::<AppState>();
    let host = host.trim().to_ascii_lowercase();
    if host.is_empty() || host.len() > 253 || bytes == 0 {
        return Err(AppError::InvalidInput("invalid server limit".into()));
    }
    let mut limits: HashMap<String, u64> = st
        .storage
        .get_setting(KEY_SERVER_LIMITS)?
        .unwrap_or_default();
    let known = limits.get(&host).copied();
    if known.is_none_or(|k| bytes < k) && (known.is_some() || limits.len() < MAX_LIMIT_HOSTS) {
        limits.insert(host, bytes);
        st.storage.set_setting(KEY_SERVER_LIMITS, &limits)?;
    }
    Ok(limits)
}

/// Longest title / body handed to the operating system.
const NOTIFY_TITLE_CHARS: usize = 80;
const NOTIFY_BODY_CHARS: usize = 240;

fn clip(text: &str, max: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match text.char_indices().nth(max) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text,
    }
}

/// Shows an operating-system notification (when they are on in Settings). Returns whether
/// it was shown. The UI calls this only while the window is in the background.
#[tauri::command]
#[specta::specta]
pub async fn os_notify(app: AppHandle, title: String, body: String) -> AppResult<bool> {
    use tauri_plugin_notification::NotificationExt;
    let st = app.state::<AppState>();
    if !crate::settings::load(&st.storage)?.os_notifications {
        return Ok(false);
    }
    let title = clip(&nanomgg_logging::redact(&title), NOTIFY_TITLE_CHARS);
    let body = clip(&nanomgg_logging::redact(&body), NOTIFY_BODY_CHARS);
    if title.is_empty() {
        return Ok(false);
    }
    match app.notification().builder().title(title).body(body).show() {
        Ok(()) => Ok(true),
        Err(e) => {
            tracing::warn!(error = %e, "could not show an OS notification");
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shows one real notification on this computer through the plugin the app uses.
    /// Run by hand: `cargo test -p nanomgg --lib -- --ignored shows_a_real_os_notification`.
    #[test]
    #[ignore = "pops up a real operating-system notification"]
    fn shows_a_real_os_notification() {
        use tauri::test::{mock_builder, mock_context, noop_assets};
        use tauri_plugin_notification::NotificationExt;
        let app = mock_builder()
            .plugin(tauri_plugin_notification::init())
            .build(mock_context(noop_assets()))
            .expect("mock app with the notification plugin");
        app.notification()
            .builder()
            .title(clip("backend-api", NOTIFY_TITLE_CHARS))
            .body(clip("Push done (notification test)", NOTIFY_BODY_CHARS))
            .show()
            .expect("the operating system accepted the notification");
    }

    #[test]
    fn clips_long_notification_text() {
        assert_eq!(
            clip(
                "  Push   done
 main ",
                80
            ),
            "Push done main"
        );
        assert_eq!(clip("abcdef", 3), "abc…");
    }
}
