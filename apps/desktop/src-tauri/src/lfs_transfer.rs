//! LFS upload/download commands (PLAN §6.4, §6.5): push estimate, `git lfs push`,
//! `git lfs pull [--include]`, `git lfs fetch --all`, with throttled progress events.
//!
//! Transfers register in the same cancel registry as fetch / pull / push, so the frontend
//! stops them with `transfer_cancel(opId)`; cancelling kills git-lfs and its children
//! (Windows Job Object). Running again resumes: objects already transferred are skipped.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use nanomgg_git_engine::AccountContext;
use nanomgg_git_engine::progress::CancelToken;
use nanomgg_lfs::transfer::{
    self, DownloadEstimate, DownloadRequest, LfsDirection, LfsFailure, LfsFileProgress,
    LfsTransferStats, PushEstimate, TransferError, TransferOptions, TransferSummary,
};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::accounts::{self, AccountOverride};
use crate::branches::Registration;
use crate::error::{AppError, AppResult};
use crate::repos::repo_root;
use crate::state::AppState;

/// Minimum time between two progress events of one transfer (the final one is always sent).
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Result of an LFS command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum LfsOutcome<T> {
    Done {
        value: T,
    },
    /// git-lfs failed; `failure.kind` says why (permission, quota, size limit…).
    Failed {
        failure: LfsFailure,
    },
    /// Stopped by `transfer_cancel`.
    Cancelled,
}

fn outcome<T>(result: Result<T, TransferError>) -> AppResult<LfsOutcome<T>> {
    match result {
        Ok(value) => Ok(LfsOutcome::Done { value }),
        Err(TransferError::Cancelled) => Ok(LfsOutcome::Cancelled),
        Err(TransferError::Failed(mut failure)) => {
            failure.message = nanomgg_logging::redact(&failure.message).into_owned();
            Ok(LfsOutcome::Failed { failure })
        }
        Err(TransferError::Invalid(message)) => Err(AppError::InvalidInput(message)),
        Err(TransferError::Lfs(e)) => Err(e.into()),
    }
}

/// Progress of a running LFS transfer, at most ~10 per second per operation.
#[derive(Debug, Clone, Serialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct LfsTransferProgress {
    pub repo_id: i64,
    /// Id chosen by the frontend when it started the command.
    pub op_id: String,
    pub direction: LfsDirection,
    pub files_done: u32,
    pub files_total: u32,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current_file: Option<String>,
    pub bytes_per_sec: u64,
    pub eta_secs: Option<u64>,
    /// Files in flight with their own progress.
    pub active: Vec<LfsFileProgress>,
    pub done: bool,
}

impl LfsTransferProgress {
    fn new(repo_id: i64, op_id: &str, s: &LfsTransferStats) -> Self {
        Self {
            repo_id,
            op_id: op_id.to_owned(),
            direction: s.direction,
            files_done: s.files_done,
            files_total: s.files_total,
            bytes_done: s.bytes_done,
            bytes_total: s.bytes_total,
            current_file: s.current_file.clone(),
            bytes_per_sec: s.bytes_per_sec,
            eta_secs: s.eta_secs,
            active: s.active.clone(),
            done: s.done,
        }
    }
}

/// Latest progress of each running LFS transfer, for [`lfs_transfer_queue`].
static RUNNING: LazyLock<Mutex<HashMap<String, LfsTransferProgress>>> =
    LazyLock::new(Mutex::default);

fn running() -> MutexGuard<'static, HashMap<String, LfsTransferProgress>> {
    RUNNING.lock().unwrap_or_else(|e| e.into_inner())
}

/// Removes the queue entry when the transfer ends, however it ends.
struct QueueEntry(String);

impl Drop for QueueEntry {
    fn drop(&mut self) {
        running().remove(&self.0);
    }
}

/// Account context of a transfer, resolved on the blocking thread right before git-lfs runs:
/// the one-shot override when given, else the account bound to `remote` (`None` = the repo
/// default, which follows `origin` — the remote `git lfs pull/fetch` use unless configured
/// otherwise). The credential server only answers for the account's provider host, so an
/// `lfs.url` on another host never receives the token.
fn context(
    st: &AppState,
    repo_id: i64,
    remote: Option<&str>,
    over: Option<&AccountOverride>,
) -> AppResult<AccountContext> {
    accounts::context_with(st, repo_id, remote, over)
}

/// Rate limiter for progress events: one per [`PROGRESS_INTERVAL`], plus the final one.
#[derive(Debug, Default)]
struct Throttle {
    last: Option<Instant>,
}

impl Throttle {
    fn ready(&mut self, done: bool, now: Instant) -> bool {
        let send = done
            || self
                .last
                .is_none_or(|at| now.duration_since(at) >= PROGRESS_INTERVAL);
        if send {
            self.last = Some(now);
        }
        send
    }
}

type Sink<'a> = &'a mut dyn FnMut(&LfsTransferStats);

/// Runs a transfer on a blocking thread, registered for cancellation and the queue.
async fn run_transfer(
    app: AppHandle,
    repo_id: i64,
    op_id: String,
    direction: LfsDirection,
    who: (Option<String>, Option<AccountOverride>),
    f: impl FnOnce(
        &AppState,
        &std::path::Path,
        &AccountContext,
        &CancelToken,
        Sink,
    ) -> Result<TransferSummary, TransferError>
    + Send
    + 'static,
) -> AppResult<LfsOutcome<TransferSummary>> {
    let repo = repo_root(&app.state::<AppState>(), repo_id)?;
    // Registered before the thread starts, so a cancel sent right away is not lost.
    let (registration, token) = Registration::new(&op_id)?;
    let initial = LfsTransferProgress::new(
        repo_id,
        &op_id,
        &LfsTransferStats {
            direction,
            files_done: 0,
            files_total: 0,
            bytes_done: 0,
            bytes_total: 0,
            current_file: None,
            bytes_per_sec: 0,
            eta_secs: None,
            active: Vec::new(),
            done: false,
        },
    );
    running().insert(op_id.clone(), initial);
    let queue_entry = QueueEntry(op_id.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let _registration = registration;
        let _queue_entry = queue_entry;
        let st = app.state::<AppState>();
        let ctx = context(&st, repo_id, who.0.as_deref(), who.1.as_ref())?;
        let started = Instant::now();
        let mut throttle = Throttle::default();
        let mut emit = |s: &LfsTransferStats| {
            let event = LfsTransferProgress::new(repo_id, &op_id, s);
            running().insert(op_id.clone(), event.clone());
            if !throttle.ready(s.done, Instant::now()) {
                return;
            }
            if let Err(e) = event.emit(&app) {
                tracing::warn!(error = %e, "failed to emit LFS transfer progress");
            }
        };
        let result = f(&st, &repo, &ctx, &token, &mut emit);
        let status = match &result {
            Ok(_) => "done".to_owned(),
            Err(TransferError::Failed(f)) => format!("failed: {:?}", f.kind),
            Err(TransferError::Cancelled) => "cancelled".to_owned(),
            Err(_) => "error".to_owned(),
        };
        tracing::info!(
            repo = repo_id,
            ?direction,
            status,
            ms = started.elapsed().as_millis() as u64,
            "LFS transfer finished"
        );
        outcome(result)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Runs blocking estimate work off the IPC thread.
async fn run_query<T: Send + 'static>(
    app: AppHandle,
    repo_id: i64,
    remote: Option<String>,
    f: impl FnOnce(&AppState, &std::path::Path, &AccountContext) -> Result<T, TransferError>
    + Send
    + 'static,
) -> AppResult<LfsOutcome<T>> {
    let repo = repo_root(&app.state::<AppState>(), repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        let ctx = context(&st, repo_id, remote.as_deref(), None)?;
        outcome(f(&st, &repo, &ctx))
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

/// What `git lfs push <remote> <branch>` would upload (an upper bound: objects the server
/// already has are only skipped during the real push).
#[tauri::command]
#[specta::specta]
pub async fn lfs_push_estimate(
    app: AppHandle,
    repo_id: i64,
    remote: String,
    branch: String,
) -> AppResult<LfsOutcome<PushEstimate>> {
    run_query(app, repo_id, Some(remote.clone()), move |st, repo, ctx| {
        transfer::push_estimate(&st.git, repo, ctx, &remote, &branch)
    })
    .await
}

/// `git lfs push <remote> <branch>` with progress events (`lfsTransferProgress`).
#[tauri::command]
#[specta::specta]
pub async fn lfs_push(
    app: AppHandle,
    repo_id: i64,
    op_id: String,
    remote: String,
    branch: String,
    options: Option<TransferOptions>,
    as_account: Option<AccountOverride>,
) -> AppResult<LfsOutcome<TransferSummary>> {
    let opts = options.unwrap_or_default();
    run_transfer(
        app,
        repo_id,
        op_id,
        LfsDirection::Upload,
        (Some(remote.clone()), as_account),
        move |st, repo, ctx, cancel, sink| {
            // The estimate gives the totals for the progress bar; a failure there is not fatal.
            let estimate = transfer::push_estimate(&st.git, repo, ctx, &remote, &branch).ok();
            transfer::push(
                &st.git,
                repo,
                ctx,
                &remote,
                &branch,
                &opts,
                estimate.as_ref(),
                cancel,
                sink,
            )
        },
    )
    .await
}

/// Objects a download would fetch, and whether they fit on the disk.
#[tauri::command]
#[specta::specta]
pub async fn lfs_download_estimate(
    app: AppHandle,
    repo_id: i64,
    request: DownloadRequest,
) -> AppResult<LfsOutcome<DownloadEstimate>> {
    run_query(app, repo_id, None, move |st, repo, ctx| {
        transfer::download_estimate(&st.git, repo, ctx, &request)
    })
    .await
}

/// `git lfs pull`, or `git lfs pull --include=<path>` for one pointer file ("Tải file này");
/// also used for the clone option "download later". Refuses with `insufficientDiskSpace`
/// unless `ignoreDiskSpace`.
#[tauri::command]
#[specta::specta]
pub async fn lfs_pull(
    app: AppHandle,
    repo_id: i64,
    op_id: String,
    include: Option<String>,
    ignore_disk_space: bool,
    options: Option<TransferOptions>,
    as_account: Option<AccountOverride>,
) -> AppResult<LfsOutcome<TransferSummary>> {
    let opts = options.unwrap_or_default();
    let request = DownloadRequest {
        include,
        ignore_disk_space,
        ..Default::default()
    };
    run_transfer(
        app,
        repo_id,
        op_id,
        LfsDirection::Download,
        (None, as_account),
        move |st, repo, ctx, cancel, sink| {
            transfer::download(&st.git, repo, ctx, &request, &opts, cancel, sink)
        },
    )
    .await
}

/// `git lfs pull --include=<a>,<b>,…`: downloads the selected pointer files in one transfer
/// ("Tải các file đã chọn" in the LFS panel).
#[tauri::command]
#[specta::specta]
pub async fn lfs_pull_files(
    app: AppHandle,
    repo_id: i64,
    op_id: String,
    paths: Vec<String>,
    ignore_disk_space: bool,
    options: Option<TransferOptions>,
) -> AppResult<LfsOutcome<TransferSummary>> {
    if paths.is_empty() {
        return Err(AppError::InvalidInput("no file selected".into()));
    }
    let opts = options.unwrap_or_default();
    let request = DownloadRequest {
        paths,
        ignore_disk_space,
        ..Default::default()
    };
    run_transfer(
        app,
        repo_id,
        op_id,
        LfsDirection::Download,
        (None, None),
        move |st, repo, ctx, cancel, sink| {
            transfer::download(&st.git, repo, ctx, &request, &opts, cancel, sink)
        },
    )
    .await
}

/// `git lfs fetch --all`: every LFS object of every ref into the local store.
#[tauri::command]
#[specta::specta]
pub async fn lfs_fetch_all(
    app: AppHandle,
    repo_id: i64,
    op_id: String,
    ignore_disk_space: bool,
    options: Option<TransferOptions>,
) -> AppResult<LfsOutcome<TransferSummary>> {
    let opts = options.unwrap_or_default();
    let request = DownloadRequest {
        all: true,
        ignore_disk_space,
        ..Default::default()
    };
    run_transfer(
        app,
        repo_id,
        op_id,
        LfsDirection::Download,
        (None, None),
        move |st, repo, ctx, cancel, sink| {
            transfer::download(&st.git, repo, ctx, &request, &opts, cancel, sink)
        },
    )
    .await
}

/// LFS transfers currently running (latest progress of each), e.g. after a window reload.
/// Cancel one with `transfer_cancel(opId)`.
#[tauri::command]
#[specta::specta]
pub fn lfs_transfer_queue() -> Vec<LfsTransferProgress> {
    let mut list: Vec<_> = running().values().cloned().collect();
    list.sort_by(|a, b| a.op_id.cmp(&b.op_id));
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcomes_serialize_with_status_tag_and_redacted_message() {
        let failed: LfsOutcome<u32> = outcome(Err(TransferError::Failed(
            transfer::errors::translate(
                "batch response: Authorization error: https://user:ghp_abcdefghijklmnopqrstuvwxyz0123456789@github.com/a/b.git/info/lfs/objects/batch",
                Some("https://github.com/a/b.git"),
            ),
        )))
        .unwrap();
        let json = serde_json::to_value(&failed).unwrap();
        assert_eq!(json["status"], "failed");
        assert_eq!(json["failure"]["kind"], "noPermission");
        let message = json["failure"]["message"].as_str().unwrap();
        assert!(
            !message.contains("ghp_abcdefghijklmnopqrstuvwxyz0123456789"),
            "{message}"
        );

        let cancelled: LfsOutcome<u32> = outcome(Err(TransferError::Cancelled)).unwrap();
        assert_eq!(
            serde_json::to_value(&cancelled).unwrap(),
            serde_json::json!({ "status": "cancelled" })
        );
        assert!(matches!(
            outcome::<u32>(Err(TransferError::Invalid("bad".into()))),
            Err(AppError::InvalidInput(_))
        ));
    }

    #[test]
    fn throttle_sends_at_most_ten_per_second_plus_the_last() {
        let mut t = Throttle::default();
        let t0 = Instant::now();
        assert!(t.ready(false, t0));
        assert!(!t.ready(false, t0 + Duration::from_millis(50)));
        assert!(t.ready(true, t0 + Duration::from_millis(60)), "final event");
        assert!(!t.ready(false, t0 + Duration::from_millis(100)));
        assert!(t.ready(false, t0 + Duration::from_millis(170)));
    }

    #[test]
    fn queue_entries_disappear_when_the_transfer_ends() {
        let stats = LfsTransferStats {
            direction: LfsDirection::Upload,
            files_done: 1,
            files_total: 2,
            bytes_done: 10,
            bytes_total: 20,
            current_file: Some("a.bin".into()),
            bytes_per_sec: 5,
            eta_secs: Some(2),
            active: Vec::new(),
            done: false,
        };
        running().insert(
            "lfs-q-1".into(),
            LfsTransferProgress::new(7, "lfs-q-1", &stats),
        );
        {
            let _entry = QueueEntry("lfs-q-1".into());
            assert!(lfs_transfer_queue().iter().any(|p| p.op_id == "lfs-q-1"));
        }
        assert!(!lfs_transfer_queue().iter().any(|p| p.op_id == "lfs-q-1"));
        let json = serde_json::to_value(LfsTransferProgress::new(7, "x", &stats)).unwrap();
        for key in [
            "repoId",
            "opId",
            "direction",
            "filesDone",
            "filesTotal",
            "bytesDone",
            "bytesTotal",
            "currentFile",
            "bytesPerSec",
            "etaSecs",
            "active",
            "done",
        ] {
            assert!(json.get(key).is_some(), "missing {key}");
        }
    }
}
