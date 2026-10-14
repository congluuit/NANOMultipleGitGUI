//! Commit-graph commands: load or refresh a repository's history, page rows to the
//! virtualized canvas, find/search commits and show commit details.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Instant;

use nanomgg_git_engine::commit::{CommitDetails, commit_details as read_commit_details};
use nanomgg_git_engine::graph::{CommitInfo, RowLayout};
use nanomgg_git_engine::history::{RefreshKind, RepoGraph};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State};

use crate::error::{AppError, AppResult};
use crate::repos::repo_root;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GraphSummary {
    pub path: String,
    pub total: u32,
    pub max_lanes: u16,
    /// Time spent loading or refreshing, in milliseconds.
    pub load_ms: u32,
    pub refresh: RefreshKind,
    /// Commits read by this call (all of them for a full load).
    pub added: u32,
    /// Changes whenever rows or labels change; the UI re-fetches rows when it does.
    pub version: u32,
}

/// One rendered row. `segments` is flat: `[kind, from, to, color]` repeated, where kind 0 is
/// the upper half (top edge → node) and kind 1 the lower half (node → bottom edge).
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GraphRow {
    pub index: u32,
    pub hash: String,
    pub subject: String,
    pub author_name: String,
    pub author_email: String,
    pub author_time: i64,
    pub refs: Vec<String>,
    pub is_merge: bool,
    pub lane: u16,
    pub color: u8,
    pub segments: Vec<u16>,
}

fn to_row(index: usize, commit: &CommitInfo, row: &RowLayout, refs: &[String]) -> GraphRow {
    let mut segments = Vec::with_capacity((row.top.len() + row.bottom.len()) * 4);
    for (kind, list) in [(0u16, &row.top), (1u16, &row.bottom)] {
        for s in list {
            segments.extend([kind, s.from, s.to, u16::from(s.color)]);
        }
    }
    GraphRow {
        index: index as u32,
        hash: commit.hash.clone(),
        subject: commit.subject.clone(),
        author_name: commit.author_name.clone(),
        author_email: commit.author_email.clone(),
        author_time: commit.author_time,
        refs: refs.to_vec(),
        is_merge: commit.parents.len() > 1,
        lane: row.lane,
        color: row.color,
        segments,
    }
}

/// One lock per repository so concurrent `graph_open` calls (watcher event + UI refresh) run
/// one after the other: the second then sees the refreshed graph instead of loading from scratch.
static LOAD_LOCKS: LazyLock<Mutex<HashMap<i64, Arc<Mutex<()>>>>> = LazyLock::new(Mutex::default);

fn load_lock(repo_id: i64) -> Arc<Mutex<()>> {
    LOAD_LOCKS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(repo_id)
        .or_default()
        .clone()
}

fn not_loaded() -> AppError {
    AppError::InvalidInput("graph not loaded".into())
}

/// Loads the history the first time, then refreshes it (incrementally when possible).
/// The graph is taken out of the shared map while git runs so row requests never wait on it.
#[tauri::command]
#[specta::specta]
pub async fn graph_open(app: AppHandle, repo_id: i64) -> AppResult<GraphSummary> {
    let repo = repo_root(&app.state::<AppState>(), repo_id)?;
    let handle = app.clone();
    let summary = tauri::async_runtime::spawn_blocking(move || {
        let st = handle.state::<AppState>();
        let lock = load_lock(repo_id);
        let _loading = lock.lock().unwrap_or_else(|e| e.into_inner());
        let t = Instant::now();
        let existing = st.graphs().remove(&repo_id);
        let (graph, refresh, added) = match existing {
            Some(mut g) => match g.refresh(&st.git, &repo) {
                Ok((kind, added)) => (g, kind, added),
                Err(e) => {
                    st.graphs().insert(repo_id, g);
                    return Err(e.into());
                }
            },
            None => {
                let g = RepoGraph::load(&st.git, &repo)?;
                let n = g.commits.len();
                (g, RefreshKind::Full, n)
            }
        };
        let summary = GraphSummary {
            path: repo.to_string_lossy().into_owned(),
            total: graph.commits.len() as u32,
            max_lanes: graph.layout.max_lanes,
            load_ms: t.elapsed().as_millis() as u32,
            refresh,
            added: added as u32,
            version: graph.version,
        };
        st.graphs().insert(repo_id, graph);
        Ok::<_, AppError>(summary)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;
    if summary.refresh != RefreshKind::Unchanged {
        tracing::info!(repo = repo_id, ?summary, "graph updated");
    }
    Ok(summary)
}

#[tauri::command]
#[specta::specta]
pub fn graph_rows(
    state: State<'_, AppState>,
    repo_id: i64,
    offset: u32,
    limit: u32,
) -> AppResult<Vec<GraphRow>> {
    let graphs = state.graphs();
    let graph = graphs.get(&repo_id).ok_or_else(not_loaded)?;
    let start = (offset as usize).min(graph.commits.len());
    let end = start
        .saturating_add(limit.min(5_000) as usize)
        .min(graph.commits.len());
    Ok((start..end)
        .map(|i| {
            let c = &graph.commits[i];
            to_row(i, c, &graph.layout.rows[i], graph.labels_for(&c.hash))
        })
        .collect())
}

/// Row of a commit (full hash or unique-enough prefix), if it is in the loaded history.
#[tauri::command]
#[specta::specta]
pub fn graph_find(state: State<'_, AppState>, repo_id: i64, oid: String) -> AppResult<Option<u32>> {
    let graphs = state.graphs();
    let graph = graphs.get(&repo_id).ok_or_else(not_loaded)?;
    Ok(graph.find(&oid).map(|i| i as u32))
}

/// Rows matching `query` in subject, author or hash prefix (at most `limit`, in graph order).
#[tauri::command]
#[specta::specta]
pub fn graph_search(
    state: State<'_, AppState>,
    repo_id: i64,
    query: String,
    limit: u32,
) -> AppResult<Vec<u32>> {
    let graphs = state.graphs();
    let graph = graphs.get(&repo_id).ok_or_else(not_loaded)?;
    Ok(graph
        .search(&query, limit.min(10_000) as usize)
        .into_iter()
        .map(|i| i as u32)
        .collect())
}

#[tauri::command]
#[specta::specta]
pub async fn commit_details(
    app: AppHandle,
    repo_id: i64,
    hash: String,
) -> AppResult<CommitDetails> {
    let repo = repo_root(&app.state::<AppState>(), repo_id)?;
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        Ok(read_commit_details(
            &handle.state::<AppState>().git,
            &repo,
            &hash,
        )?)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[derive(Debug, Clone, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StartupArgs {
    /// Repository passed on the command line (`nanomgg <path>` or `--open <path>`).
    pub repo: Option<String>,
    /// `--bench-graph`: run the scroll benchmark once the graph is painted.
    pub bench_graph: bool,
}

pub fn parse_startup_args(args: impl IntoIterator<Item = String>) -> StartupArgs {
    let mut out = StartupArgs::default();
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--bench-graph" => out.bench_graph = true,
            "--open" => out.repo = it.next(),
            a if !a.starts_with('-') && out.repo.is_none() => out.repo = Some(a.to_owned()),
            _ => {}
        }
    }
    out
}

#[tauri::command]
#[specta::specta]
pub fn startup_args() -> StartupArgs {
    parse_startup_args(std::env::args().skip(1))
}

#[derive(Debug, Clone, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GraphBenchmark {
    pub total_rows: u32,
    pub first_paint_ms: f64,
    pub frames: u32,
    pub avg_fps: f64,
    pub p95_frame_ms: f64,
    pub max_frame_ms: f64,
    pub slow_frames: u32,
    pub avg_draw_ms: f64,
    pub max_draw_ms: f64,
}

#[tauri::command]
#[specta::specta]
pub fn report_benchmark(result: GraphBenchmark) {
    tracing::info!(?result, "graph benchmark");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> StartupArgs {
        parse_startup_args(list.iter().map(|s| (*s).to_owned()))
    }

    #[test]
    fn startup_args_parsing() {
        assert!(args(&[]).repo.is_none());
        assert_eq!(args(&["D:/repo"]).repo.as_deref(), Some("D:/repo"));
        let a = args(&["--open", "/tmp/x", "--bench-graph"]);
        assert_eq!(a.repo.as_deref(), Some("/tmp/x"));
        assert!(a.bench_graph);
    }
}
