//! Opening, creating and closing repositories, the tab workspace, recent repositories,
//! sidebar refs and on-disk change notifications.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use nanomgg_git_engine::refs::{RepoRefs, read_refs};
use nanomgg_git_engine::repo::{self, RepoInfo};
use nanomgg_git_engine::version::{self, Tools};
use nanomgg_git_engine::watch::RepoWatcher;
use nanomgg_storage::Storage;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::error::{AppError, AppResult};
use crate::state::{AppState, Workspace};

const KEY_TABS: &str = "workspace.tabs";
const KEY_ACTIVE: &str = "workspace.active";
const RECENT_LIMIT: u32 = 30;

#[derive(Debug, Clone, Serialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RepoTab {
    pub id: i64,
    pub path: String,
    pub name: String,
    /// The folder is gone (moved, deleted, unplugged drive) since it was last opened.
    pub missing: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceState {
    pub tabs: Vec<RepoTab>,
    pub active_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecentRepo {
    pub id: i64,
    pub path: String,
    pub name: String,
    /// Milliseconds since the Unix epoch.
    pub last_opened_at: i64,
    pub exists: bool,
}

/// Emitted (debounced) when files of an open repository change on disk.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct RepoChanged {
    pub repo_id: i64,
    pub refs: bool,
    pub index: bool,
    pub worktree: bool,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Comparison key for a repository root: case-folded where file systems usually ignore case.
pub fn path_key(path: &Path) -> String {
    let s = path.to_string_lossy();
    if cfg!(any(windows, target_os = "macos")) {
        s.to_lowercase()
    } else {
        s.into_owned()
    }
}

/// Runs blocking work (git processes, disk access) off the IPC thread.
async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppHandle, &AppState) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || f(&app, &app.state::<AppState>()))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

fn persist(storage: &Storage, ws: &Workspace) -> AppResult<()> {
    storage.set_setting(KEY_TABS, &ws.tabs)?;
    storage.set_setting(KEY_ACTIVE, &ws.active)?;
    Ok(())
}

fn start_watcher(app: &AppHandle, ws: &mut Workspace, id: i64, info: &RepoInfo) {
    if ws.watchers.contains_key(&id) {
        return;
    }
    let handle = app.clone();
    let git = app.state::<AppState>().git.clone();
    match RepoWatcher::start(&git, &info.root, &info.git_dir, move |c| {
        let event = RepoChanged {
            repo_id: id,
            refs: c.refs,
            index: c.index,
            worktree: c.worktree,
        };
        if let Err(e) = event.emit(&handle) {
            tracing::warn!(error = %e, "failed to emit repo change");
        }
    }) {
        Ok(w) => {
            ws.watchers.insert(id, w);
        }
        Err(e) => tracing::warn!(repo = id, error = %e, "file watcher unavailable"),
    }
}

fn tab_for(storage: &Storage, ws: &Workspace, id: i64) -> AppResult<Option<RepoTab>> {
    Ok(storage.repo(id)?.map(|r| RepoTab {
        id,
        missing: !ws.repos.contains_key(&id),
        path: r.path,
        name: r.name,
    }))
}

fn snapshot(storage: &Storage, ws: &Workspace) -> AppResult<WorkspaceState> {
    let mut tabs = Vec::with_capacity(ws.tabs.len());
    for &id in &ws.tabs {
        if let Some(tab) = tab_for(storage, ws, id)? {
            tabs.push(tab);
        }
    }
    Ok(WorkspaceState {
        tabs,
        active_id: ws.active,
    })
}

/// Adds a resolved repository as the active tab (or focuses its existing tab).
pub(crate) fn register(app: &AppHandle, st: &AppState, info: RepoInfo) -> AppResult<RepoTab> {
    let record = st.storage.touch_repo(
        &info.root.to_string_lossy(),
        &path_key(&info.root),
        &info.name,
        now_ms(),
    )?;
    let mut ws = st.workspace();
    if !ws.tabs.contains(&record.id) {
        ws.tabs.push(record.id);
    }
    ws.active = Some(record.id);
    start_watcher(app, &mut ws, record.id, &info);
    ws.repos.insert(record.id, info);
    persist(&st.storage, &ws)?;
    tracing::info!(repo = record.id, path = %record.path, "repository opened");
    Ok(RepoTab {
        id: record.id,
        path: record.path,
        name: record.name,
        missing: false,
    })
}

/// Root of an open, available repository.
pub fn repo_root(st: &AppState, id: i64) -> AppResult<PathBuf> {
    st.workspace()
        .repos
        .get(&id)
        .map(|r| r.root.clone())
        .ok_or_else(|| AppError::InvalidInput(format!("repository {id} is not open")))
}

#[tauri::command]
#[specta::specta]
pub async fn tool_status(app: AppHandle) -> AppResult<Tools> {
    blocking(app, |_, st| Ok(version::detect(&st.git))).await
}

#[tauri::command]
#[specta::specta]
pub async fn repo_open(app: AppHandle, path: String) -> AppResult<RepoTab> {
    blocking(app, move |app, st| {
        let info = repo::discover(&st.git, Path::new(&path))?;
        register(app, st, info)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn repo_init(
    app: AppHandle,
    path: String,
    initial_branch: Option<String>,
) -> AppResult<RepoTab> {
    blocking(app, move |app, st| {
        let branch = initial_branch
            .filter(|b| !b.trim().is_empty())
            .unwrap_or_else(|| "main".into());
        let info = repo::init(&st.git, Path::new(&path), branch.trim())?;
        register(app, st, info)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn repo_close(app: AppHandle, id: i64) -> AppResult<WorkspaceState> {
    blocking(app, move |_, st| {
        let mut ws = st.workspace();
        if let Some(pos) = ws.tabs.iter().position(|&t| t == id) {
            ws.tabs.remove(pos);
            if ws.active == Some(id) {
                let next = ws.tabs.get(pos).or_else(|| ws.tabs.last()).copied();
                ws.active = next;
            }
        }
        ws.watchers.remove(&id);
        ws.repos.remove(&id);
        st.graphs().remove(&id);
        persist(&st.storage, &ws)?;
        snapshot(&st.storage, &ws)
    })
    .await
}

/// Current tabs. The first call of a session restores the tabs saved in the database,
/// re-resolving each folder (missing ones are kept and flagged so the user can relocate or close).
#[tauri::command]
#[specta::specta]
pub async fn workspace_state(app: AppHandle) -> AppResult<WorkspaceState> {
    blocking(app, |app, st| {
        let mut ws = st.workspace();
        if !ws.restored {
            ws.restored = true;
            let saved: Vec<i64> = st.storage.get_setting(KEY_TABS)?.unwrap_or_default();
            let active: Option<i64> = st.storage.get_setting(KEY_ACTIVE)?.flatten();
            for id in saved {
                let Some(record) = st.storage.repo(id)? else {
                    continue;
                };
                if !ws.tabs.contains(&id) {
                    ws.tabs.push(id);
                }
                match repo::discover(&st.git, Path::new(&record.path)) {
                    Ok(info) => {
                        start_watcher(app, &mut ws, id, &info);
                        ws.repos.insert(id, info);
                    }
                    Err(e) => tracing::warn!(repo = id, error = %e, "saved repository unavailable"),
                }
            }
            ws.active = active
                .filter(|a| ws.tabs.contains(a))
                .or(ws.tabs.first().copied());
        }
        snapshot(&st.storage, &ws)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub fn workspace_set_active(app: AppHandle, id: i64) -> AppResult<()> {
    let st = app.state::<AppState>();
    let mut ws = st.workspace();
    if !ws.tabs.contains(&id) {
        return Err(AppError::InvalidInput(format!(
            "repository {id} is not open"
        )));
    }
    ws.active = Some(id);
    persist(&st.storage, &ws)
}

/// The tab order `wanted` asks for, limited to the tabs that are open: unknown ids are
/// dropped, open tabs it leaves out keep their place at the end.
fn reorder_tabs(current: &[i64], wanted: &[i64]) -> Vec<i64> {
    let mut out: Vec<i64> = Vec::with_capacity(current.len());
    for id in wanted {
        if current.contains(id) && !out.contains(id) {
            out.push(*id);
        }
    }
    for id in current {
        if !out.contains(id) {
            out.push(*id);
        }
    }
    out
}

/// Moves the tabs into the order of `ids` (drag and drop, pinning) and saves it.
#[tauri::command]
#[specta::specta]
pub fn workspace_reorder(app: AppHandle, ids: Vec<i64>) -> AppResult<WorkspaceState> {
    let st = app.state::<AppState>();
    let mut ws = st.workspace();
    ws.tabs = reorder_tabs(&ws.tabs, &ids);
    persist(&st.storage, &ws)?;
    snapshot(&st.storage, &ws)
}

#[tauri::command]
#[specta::specta]
pub async fn recent_repos(app: AppHandle) -> AppResult<Vec<RecentRepo>> {
    blocking(app, |_, st| {
        Ok(st
            .storage
            .recent_repos(RECENT_LIMIT)?
            .into_iter()
            .map(|r| RecentRepo {
                exists: Path::new(&r.path).is_dir(),
                id: r.id,
                path: r.path,
                name: r.name,
                last_opened_at: r.last_opened_at,
            })
            .collect())
    })
    .await
}

/// Forgets a repository (does not touch its files). Closes its tab if open.
#[tauri::command]
#[specta::specta]
pub async fn recent_remove(app: AppHandle, id: i64) -> AppResult<WorkspaceState> {
    let state = repo_close(app.clone(), id).await?;
    blocking(app, move |_, st| {
        st.storage.delete_repo(id)?;
        Ok(state)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn repo_refs(app: AppHandle, id: i64) -> AppResult<RepoRefs> {
    blocking(app, move |_, st| {
        let root = repo_root(st, id)?;
        let mut refs = read_refs(&st.git, &root)?;
        for remote in &mut refs.remotes {
            if let Some(url) = &mut remote.url {
                *url = nanomgg_logging::redact(url).into_owned();
            }
        }
        Ok(refs)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_keys_fold_case_where_the_os_does() {
        let a = path_key(Path::new("/Work/Repo"));
        let b = path_key(Path::new("/work/repo"));
        if cfg!(any(windows, target_os = "macos")) {
            assert_eq!(a, b);
        } else {
            assert_ne!(a, b);
        }
    }

    #[test]
    fn reorder_keeps_only_open_tabs_and_appends_the_rest() {
        assert_eq!(reorder_tabs(&[1, 2, 3], &[3, 1, 2]), vec![3, 1, 2]);
        assert_eq!(reorder_tabs(&[1, 2, 3], &[3, 9, 3]), vec![3, 1, 2]);
        assert_eq!(reorder_tabs(&[1, 2], &[]), vec![1, 2]);
    }

    #[test]
    fn snapshot_keeps_tab_order_and_flags_missing_folders() {
        let db = Storage::open_in_memory().unwrap();
        let a = db.touch_repo("/a", "/a", "a", 1).unwrap();
        let b = db.touch_repo("/b", "/b", "b", 2).unwrap();
        let mut ws = Workspace {
            tabs: vec![b.id, a.id, 999],
            active: Some(a.id),
            ..Default::default()
        };
        ws.repos.insert(
            a.id,
            RepoInfo {
                root: "/a".into(),
                git_dir: "/a/.git".into(),
                name: "a".into(),
            },
        );
        let snap = snapshot(&db, &ws).unwrap();
        assert_eq!(
            snap.tabs
                .iter()
                .map(|t| (t.name.as_str(), t.missing))
                .collect::<Vec<_>>(),
            vec![("b", true), ("a", false)],
            "unknown id 999 is dropped"
        );
        assert_eq!(snap.active_id, Some(a.id));

        persist(&db, &ws).unwrap();
        assert_eq!(
            db.get_setting::<Vec<i64>>(KEY_TABS).unwrap().unwrap(),
            ws.tabs
        );
    }
}
