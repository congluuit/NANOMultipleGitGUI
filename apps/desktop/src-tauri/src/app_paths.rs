//! Settings → Storage (PLAN 3B.3): the single source of the log / cache / app-data folders.
//!
//! [`AppPaths::resolve`] reads `<default local data>/locations.json` at startup (before
//! logging and the database), so every other module asks [`data_dir`] /
//! `AppPaths::get(StorageKind::Cache)` instead of `app.path().app_data_dir()`. Nothing is
//! written to the cache folder yet (repository lists are kept in memory); it is the place
//! for future disk caches — everything in it may be deleted at any time.
//!
//! - Log and cache move right away: copy → verify → switch → delete the old files, with
//!   [`StorageMoveProgress`] events. The log writer is paused (lines buffered in memory) during
//!   the copy and reopened in the new folder — no restart needed.
//! - The data folder (SQLite open, SSH keys, credential server) is only *scheduled*; the move
//!   runs on the next start before the database opens ([`AppPaths::apply_pending_data`]), then
//!   [`after_data_move`] rewrites the absolute key paths stored in the database.
//! - The OS keychain is not a folder and never moves. The WebView2 profile (`EBWebView` in
//!   `%LOCALAPPDATA%\<id>`) is not listed: it is held open by the running window, can only be
//!   relocated when the window is created, and holds UI preferences (localStorage) rather
//!   than cache the user benefits from clearing.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use nanomgg_app_paths::{
    CleanReport, Locations, MovePhase, MoveProgress, POINTER_FILE, PendingDataMove, Pointer,
    StorageKind, StorageWarning, TargetProblem,
};
use nanomgg_logging::LogHandle;
use nanomgg_storage::Storage;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;
use tauri_specta::Event;

use crate::error::{AppError, AppResult};

/// Below this much free space on the data drive the UI warns.
pub const LOW_SPACE_BYTES: u64 = 1024 * 1024 * 1024;
/// Time budget for measuring one folder in the overview.
const USAGE_BUDGET: Duration = Duration::from_secs(3);
/// Time budget for measuring the folder about to be moved (free-space check).
const MOVE_SIZE_BUDGET: Duration = Duration::from_secs(20);

struct Current {
    locations: Locations,
    pointer: Pointer,
    warnings: Vec<StorageWarning>,
}

/// Managed state: where each kind of folder is right now.
pub struct AppPaths {
    defaults: Locations,
    pointer_path: PathBuf,
    /// Folder of the pointer file (also the WebView2 profile on Windows): never a target.
    app_root: PathBuf,
    current: Mutex<Current>,
    log: Mutex<Option<LogHandle>>,
    /// A log/cache move is running.
    busy: AtomicBool,
}

impl AppPaths {
    /// Reads the pointer next to the default local data. Never fails on a bad pointer.
    pub fn resolve(app: &AppHandle) -> tauri::Result<Self> {
        let path = app.path();
        let app_root = path.app_local_data_dir()?;
        let defaults = Locations {
            log: path.app_log_dir()?,
            cache: app_root.join("cache"),
            data: path.app_data_dir()?,
        };
        Ok(Self::from_defaults(defaults, app_root))
    }

    fn from_defaults(defaults: Locations, app_root: PathBuf) -> Self {
        let pointer_path = app_root.join(POINTER_FILE);
        let res = nanomgg_app_paths::resolve(&defaults, &pointer_path);
        Self {
            defaults,
            pointer_path,
            app_root,
            current: Mutex::new(Current {
                locations: res.locations,
                pointer: res.pointer,
                warnings: res.warnings,
            }),
            log: Mutex::default(),
            busy: AtomicBool::new(false),
        }
    }

    fn current(&self) -> MutexGuard<'_, Current> {
        self.current.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn get(&self, kind: StorageKind) -> PathBuf {
        self.current().locations.get(kind).to_owned()
    }

    pub fn data_dir(&self) -> PathBuf {
        self.get(StorageKind::Data)
    }

    /// Installs the global logger in the log folder; a custom folder that cannot be opened
    /// falls back to the default with a warning.
    pub fn init_logging(&self) -> std::io::Result<()> {
        let dir = self.get(StorageKind::Log);
        let handle = match nanomgg_logging::init(&dir) {
            Ok(h) => h,
            Err(e) if dir != self.defaults.log => {
                let mut cur = self.current();
                cur.warnings.push(StorageWarning::MissingFolder {
                    storage: StorageKind::Log,
                    path: format!("{} ({e})", dir.display()),
                    fallback: self.defaults.log.display().to_string(),
                });
                cur.locations.log = self.defaults.log.clone();
                drop(cur);
                nanomgg_logging::init(&self.defaults.log)?
            }
            Err(e) => return Err(e),
        };
        *self.log.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle);
        for w in &self.current().warnings {
            tracing::warn!(warning = ?w, "storage location problem");
        }
        Ok(())
    }

    /// Runs the data move scheduled in the previous session (call before opening the DB).
    pub fn apply_pending_data(&self) -> Option<nanomgg_app_paths::DataMoved> {
        let mut cur = self.current();
        let mut res = nanomgg_app_paths::Resolution {
            locations: cur.locations.clone(),
            pointer: cur.pointer.clone(),
            warnings: Vec::new(),
        };
        res.pointer.pending_data.as_ref()?;
        tracing::info!(pending = ?res.pointer.pending_data, "moving the app data folder");
        let moved = nanomgg_app_paths::apply_pending_data(
            &mut res,
            &self.defaults,
            &self.pointer_path,
            &mut |_| {},
            &mut |to| restrict_ssh_keys(to),
        );
        for w in &res.warnings {
            tracing::warn!(warning = ?w, "app data move");
        }
        cur.locations = res.locations;
        cur.pointer = res.pointer;
        cur.warnings.extend(res.warnings);
        moved
    }

    fn log_handle(&self) -> Option<LogHandle> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn protected(&self) -> Vec<PathBuf> {
        vec![self.app_root.clone()]
    }
}

/// Bundle identifiers of earlier builds: the OS derives the app folders from the identifier,
/// so data written under one of these is copied over once (see [`migrate_legacy_identifier`]).
const LEGACY_IDENTIFIERS: &[&str] = &["io.github.nanomgg.nanomultiplegitgui"];
const DB_FILE: &str = "nanomgg.db";

/// First start after the bundle identifier changed (pre-1.0 builds used a placeholder): copies
/// the database, SSH keys and `locations.json` of the old folders into the new ones. The old
/// folders are left as they were. The OS keychain does not depend on the identifier. Runs
/// before logging starts, so the caller logs the outcome.
pub fn migrate_legacy_identifier(
    app: &AppHandle,
) -> Result<Option<nanomgg_app_paths::DataMoved>, String> {
    let path = app.path();
    let data = path.app_data_dir().map_err(|e| e.to_string())?;
    let local = path.app_local_data_dir().map_err(|e| e.to_string())?;
    migrate_legacy_from(&data, &local, LEGACY_IDENTIFIERS)
}

fn migrate_legacy_from(
    data: &Path,
    local: &Path,
    legacy: &[&str],
) -> Result<Option<nanomgg_app_paths::DataMoved>, String> {
    if data.join(DB_FILE).exists() || local.join(POINTER_FILE).exists() {
        return Ok(None);
    }
    for id in legacy {
        let (old_data, old_local) = (data.with_file_name(id), local.with_file_name(id));
        let old_pointer = old_local.join(POINTER_FILE);
        let has_db = old_data.join(DB_FILE).is_file();
        if !has_db && !old_pointer.is_file() {
            continue;
        }
        let mut moved = None;
        if has_db {
            // On macOS the data and local folders are the same one.
            let exclude = [
                old_pointer.clone(),
                old_local.join("cache"),
                old_local.join("EBWebView"),
            ];
            let tree = nanomgg_app_paths::copy_tree(&old_data, data, &exclude, &mut |_| {})
                .map_err(|e| format!("could not copy {}: {e}", old_data.display()))?;
            restrict_ssh_keys(data);
            moved = Some(nanomgg_app_paths::DataMoved {
                from: tree.from,
                to: tree.to,
            });
        }
        // Copied last: its presence marks the migration as done. When the user had moved the
        // data folder elsewhere, the pointer alone keeps the app using it.
        if old_pointer.is_file() {
            std::fs::create_dir_all(local)
                .and_then(|()| std::fs::copy(&old_pointer, local.join(POINTER_FILE)))
                .map_err(|e| format!("could not copy {}: {e}", old_pointer.display()))?;
        }
        return Ok(moved);
    }
    Ok(None)
}

/// Copying does not keep the user-only ACL of the key files: re-apply it on the new copies.
fn restrict_ssh_keys(data_dir: &Path) {
    let store = nanomgg_ssh_keys::KeyStore::new(data_dir);
    if !store.dir().is_dir() {
        return;
    }
    if let Err(e) = nanomgg_ssh_keys::restrict_to_current_user(store.dir(), true) {
        tracing::warn!(error = %e, "could not restrict the moved SSH key folder");
    }
    for id in store.list().unwrap_or_default() {
        if let Err(e) = store.repair_permissions(&id) {
            tracing::warn!(key = %id, error = %e, "could not restrict a moved SSH key");
        }
    }
}

/// After a data move: absolute paths stored in the database (SSH key files, signing keys
/// generated by the app) still point into the old folder.
pub fn after_data_move(storage: &Storage, moved: &nanomgg_app_paths::DataMoved) {
    let rebase = |value: &str| {
        nanomgg_app_paths::rebase_path(Path::new(value), &moved.from, &moved.to)
            .map(|p| p.display().to_string())
    };
    match storage.ssh_keys(None) {
        Ok(keys) => {
            for mut key in keys {
                if let Some(path) = key.private_key_path.as_deref().and_then(rebase) {
                    key.private_key_path = Some(path);
                    if let Err(e) = storage.update_ssh_key(&key) {
                        tracing::warn!(key = %key.id, error = %e, "could not update a moved SSH key path");
                    }
                }
            }
        }
        Err(e) => tracing::warn!(error = %e, "could not list SSH keys after the data move"),
    }
    match storage.identities(None) {
        Ok(identities) => {
            for mut identity in identities {
                if let Some(path) = identity.signing_key_ref.as_deref().and_then(rebase) {
                    identity.signing_key_ref = Some(path);
                    if let Err(e) = storage.update_identity(&identity) {
                        tracing::warn!(error = %e, "could not update a moved signing key path");
                    }
                }
            }
        }
        Err(e) => tracing::warn!(error = %e, "could not list identities after the data move"),
    }
}

/// App data folder in effect (SQLite, SSH keys, `known_hosts`, `allowed_signers`).
pub fn data_dir(app: &AppHandle) -> AppResult<PathBuf> {
    app.try_state::<AppPaths>()
        .map(|p| p.data_dir())
        .ok_or_else(|| AppError::Internal("storage locations are not ready".into()))
}

// ----- DTOs ----------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StorageLocation {
    pub kind: StorageKind,
    pub path: String,
    pub default_path: String,
    pub is_default: bool,
    pub used_bytes: u64,
    pub file_count: u64,
    /// `false` when measuring took too long: `usedBytes` is a lower bound.
    pub usage_complete: bool,
    pub free_bytes: Option<u64>,
    /// Data folder scheduled for the next start.
    pub pending_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StorageOverview {
    pub locations: Vec<StorageLocation>,
    /// A move is running.
    pub busy: bool,
}

/// Light status polled for the low-space warning and startup notices.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StorageStatus {
    pub data_dir: String,
    pub data_free_bytes: Option<u64>,
    pub low_space: bool,
    pub warnings: Vec<StorageWarning>,
    pub pending_data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StorageTargetCheck {
    /// Folder that would be used (`<chosen>/nanomgg-…` when the chosen one is not empty).
    pub target: String,
    pub creates_subfolder: bool,
    pub is_default: bool,
    pub needed_bytes: u64,
    pub free_bytes: Option<u64>,
    pub problem: Option<TargetProblem>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StorageChangeResult {
    pub target: String,
    /// Data folder: scheduled, applied on restart.
    pub restart_required: bool,
    /// Old files that could not be deleted after a successful move.
    pub old_files_left: u32,
    /// Set instead of moving when the folder cannot be used.
    pub problem: Option<TargetProblem>,
}

/// Progress of a log/cache move, at most ~10 per second (phase changes always sent).
#[derive(Debug, Clone, Serialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct StorageMoveProgress {
    pub kind: StorageKind,
    pub phase: MovePhase,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub done_files: u64,
    pub total_files: u64,
}

// ----- commands ------------------------------------------------------------------------------

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppHandle, &AppPaths) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = app
            .try_state::<AppPaths>()
            .ok_or_else(|| AppError::Internal("storage locations are not ready".into()))?;
        f(&app, &paths)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

fn exclude_for(locations: &Locations, kind: StorageKind, pointer: &Path) -> Vec<PathBuf> {
    let mut exclude = locations.others(kind);
    exclude.push(pointer.to_owned());
    exclude
}

fn overview_impl(paths: &AppPaths) -> StorageOverview {
    let (locations, pending) = {
        let cur = paths.current();
        (cur.locations.clone(), cur.pointer.pending_data.clone())
    };
    let deadline = Instant::now() + USAGE_BUDGET;
    let usages: Vec<_> = std::thread::scope(|s| {
        let handles: Vec<_> = StorageKind::ALL
            .into_iter()
            .map(|kind| {
                let exclude = exclude_for(&locations, kind, &paths.pointer_path);
                let dir = locations.get(kind).to_owned();
                s.spawn(move || nanomgg_app_paths::dir_usage(&dir, &exclude, deadline))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_default())
            .collect()
    });
    let locations = StorageKind::ALL
        .into_iter()
        .zip(usages)
        .map(|(kind, usage)| {
            let path = locations.get(kind);
            StorageLocation {
                kind,
                path: path.display().to_string(),
                default_path: paths.defaults.get(kind).display().to_string(),
                is_default: nanomgg_app_paths::same_path(path, paths.defaults.get(kind)),
                used_bytes: usage.bytes,
                file_count: usage.files,
                usage_complete: usage.complete,
                free_bytes: nanomgg_app_paths::free_space(path),
                pending_path: (kind == StorageKind::Data)
                    .then(|| pending.as_ref().map(|p| p.to.display().to_string()))
                    .flatten(),
            }
        })
        .collect();
    StorageOverview {
        locations,
        busy: paths.busy.load(Ordering::SeqCst),
    }
}

fn status_impl(paths: &AppPaths) -> StorageStatus {
    let cur = paths.current();
    let free = nanomgg_app_paths::free_space(&cur.locations.data);
    StorageStatus {
        data_dir: cur.locations.data.display().to_string(),
        data_free_bytes: free,
        low_space: free.is_some_and(|f| f < LOW_SPACE_BYTES),
        warnings: cur.warnings.clone(),
        pending_data: cur
            .pointer
            .pending_data
            .as_ref()
            .map(|p| p.to.display().to_string()),
    }
}

/// `None` = back to the default location.
fn picked_path(paths: &AppPaths, kind: StorageKind, path: Option<String>) -> PathBuf {
    match path {
        Some(p) => PathBuf::from(p.trim()),
        None => paths.defaults.get(kind).to_owned(),
    }
}

fn check_impl(paths: &AppPaths, kind: StorageKind, picked: &Path) -> StorageTargetCheck {
    let (locations, pointer_path) = (paths.current().locations.clone(), &paths.pointer_path);
    let mut check = StorageTargetCheck {
        target: picked.display().to_string(),
        creates_subfolder: false,
        is_default: false,
        needed_bytes: 0,
        free_bytes: None,
        problem: None,
    };
    let plan = match nanomgg_app_paths::plan_target(
        kind,
        picked,
        &locations,
        &paths.defaults,
        &paths.protected(),
    ) {
        Ok(plan) => plan,
        Err(problem) => {
            check.problem = Some(problem);
            return check;
        }
    };
    check.target = plan.target.display().to_string();
    check.creates_subfolder = plan.creates_subfolder;
    check.is_default = plan.is_default;
    let usage = nanomgg_app_paths::dir_usage(
        locations.get(kind),
        &exclude_for(&locations, kind, pointer_path),
        Instant::now() + MOVE_SIZE_BUDGET,
    );
    check.needed_bytes = usage.bytes;
    match nanomgg_app_paths::verify_target(&plan.target, usage.bytes) {
        Ok(free) => check.free_bytes = Some(free),
        Err(problem) => {
            check.free_bytes = nanomgg_app_paths::free_space(&plan.target);
            check.problem = Some(problem);
        }
    }
    check
}

struct BusyGuard<'a>(&'a AtomicBool);

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn change_impl(
    app: &AppHandle,
    paths: &AppPaths,
    kind: StorageKind,
    picked: &Path,
) -> AppResult<StorageChangeResult> {
    if paths.busy.swap(true, Ordering::SeqCst) {
        return Err(AppError::InvalidInput(
            "another storage move is still running".into(),
        ));
    }
    let _busy = BusyGuard(&paths.busy);
    let check = check_impl(paths, kind, picked);
    let target = PathBuf::from(&check.target);
    let mut result = StorageChangeResult {
        target: check.target.clone(),
        restart_required: false,
        old_files_left: 0,
        problem: check.problem.clone(),
    };
    if result.problem.is_some() {
        return Ok(result);
    }
    if kind == StorageKind::Data {
        let mut cur = paths.current();
        let mut next = cur.pointer.clone();
        next.pending_data = Some(PendingDataMove {
            from: cur.locations.data.clone(),
            to: target,
        });
        write_pointer(&paths.pointer_path, &next)?;
        cur.pointer = next;
        result.restart_required = true;
        tracing::info!(to = %result.target, "app data move scheduled for the next start");
        return Ok(result);
    }
    result.old_files_left = move_now(app, paths, kind, &target)?;
    Ok(result)
}

fn write_pointer(path: &Path, pointer: &Pointer) -> AppResult<()> {
    nanomgg_app_paths::write_pointer(path, pointer)
        .map_err(|e| AppError::Internal(format!("could not save {}: {e}", path.display())))
}

/// Log/cache: copy, verify, switch, delete the old files. Any failure before the switch
/// leaves the old folder in use, untouched.
fn move_now(app: &AppHandle, paths: &AppPaths, kind: StorageKind, target: &Path) -> AppResult<u32> {
    let (locations, pointer) = {
        let cur = paths.current();
        (cur.locations.clone(), cur.pointer.clone())
    };
    let from = locations.get(kind).to_owned();
    let exclude = exclude_for(&locations, kind, &paths.pointer_path);
    let log = (kind == StorageKind::Log)
        .then(|| paths.log_handle())
        .flatten();
    let reopen_old = |log: &Option<LogHandle>| {
        if let Some(h) = log
            && let Err(e) = h.resume(&from)
        {
            eprintln!("could not reopen the log file in {}: {e}", from.display());
        }
    };
    tracing::info!(?kind, from = %from.display(), to = %target.display(), "moving storage folder");
    if let Some(h) = &log {
        h.pause();
    }
    let mut emitter = ProgressEmitter::new(app, kind);
    let tree = match nanomgg_app_paths::copy_tree(&from, target, &exclude, &mut |p| emitter.send(p))
    {
        Ok(tree) => tree,
        Err(e) => {
            reopen_old(&log);
            tracing::warn!(?kind, error = %e, "storage move failed; old folder kept");
            return Err(AppError::Internal(
                nanomgg_logging::redact(&e.to_string()).into_owned(),
            ));
        }
    };
    if let Some(h) = &log
        && let Err(e) = h.resume(target)
    {
        tree.rollback();
        reopen_old(&log);
        return Err(AppError::Internal(format!(
            "could not write logs to {}: {e}",
            target.display()
        )));
    }
    let mut next = pointer;
    next.set(kind, target, &paths.defaults);
    if let Err(e) = write_pointer(&paths.pointer_path, &next) {
        tree.rollback();
        reopen_old(&log);
        return Err(e);
    }
    {
        let mut cur = paths.current();
        cur.pointer = next;
        cur.locations.set(kind, target.to_owned());
    }
    emitter.send(&MoveProgress {
        phase: MovePhase::Cleaning,
        done_bytes: tree.bytes,
        total_bytes: tree.bytes,
        done_files: tree.files.len() as u64,
        total_files: tree.files.len() as u64,
    });
    let left = tree.remove_source();
    if !left.is_empty() {
        tracing::warn!(
            ?kind,
            count = left.len(),
            "some old files could not be deleted"
        );
    }
    tracing::info!(?kind, to = %target.display(), "storage folder moved");
    Ok(u32::try_from(left.len()).unwrap_or(u32::MAX))
}

struct ProgressEmitter<'a> {
    app: &'a AppHandle,
    kind: StorageKind,
    last: Option<(MovePhase, Instant)>,
}

impl<'a> ProgressEmitter<'a> {
    fn new(app: &'a AppHandle, kind: StorageKind) -> Self {
        Self {
            app,
            kind,
            last: None,
        }
    }

    fn send(&mut self, p: &MoveProgress) {
        let now = Instant::now();
        let finished = p.done_files == p.total_files;
        if let Some((phase, at)) = self.last
            && phase == p.phase
            && !finished
            && now.duration_since(at) < Duration::from_millis(100)
        {
            return;
        }
        self.last = Some((p.phase, now));
        let event = StorageMoveProgress {
            kind: self.kind,
            phase: p.phase,
            done_bytes: p.done_bytes,
            total_bytes: p.total_bytes,
            done_files: p.done_files,
            total_files: p.total_files,
        };
        if let Err(e) = event.emit(self.app) {
            tracing::debug!(error = %e, "could not emit storage progress");
        }
    }
}

/// Every folder with its size (measured with a time limit), path and free space.
#[tauri::command]
#[specta::specta]
pub async fn storage_overview(app: AppHandle) -> AppResult<StorageOverview> {
    blocking(app, |_, paths| Ok(overview_impl(paths))).await
}

/// Free space of the data drive (low-space warning) and the startup warnings.
#[tauri::command]
#[specta::specta]
pub async fn storage_status(app: AppHandle) -> AppResult<StorageStatus> {
    blocking(app, |_, paths| Ok(status_impl(paths))).await
}

/// Checks a folder before moving there (`path: null` = the default location). Problems the
/// user can fix are returned in `problem`, not as errors.
#[tauri::command]
#[specta::specta]
pub async fn storage_check_target(
    app: AppHandle,
    kind: StorageKind,
    path: Option<String>,
) -> AppResult<StorageTargetCheck> {
    blocking(app, move |_, paths| {
        Ok(check_impl(paths, kind, &picked_path(paths, kind, path)))
    })
    .await
}

/// Moves the log or cache folder now (with [`StorageMoveProgress`]), or schedules the data
/// folder for the next start (`restartRequired`). `path: null` = back to the default.
#[tauri::command]
#[specta::specta]
pub async fn storage_change(
    app: AppHandle,
    kind: StorageKind,
    path: Option<String>,
) -> AppResult<StorageChangeResult> {
    blocking(app, move |app, paths| {
        change_impl(app, paths, kind, &picked_path(paths, kind, path))
    })
    .await
}

/// Drops the data move scheduled for the next start.
#[tauri::command]
#[specta::specta]
pub async fn storage_cancel_pending(app: AppHandle) -> AppResult<StorageStatus> {
    blocking(app, |_, paths| {
        {
            let mut cur = paths.current();
            let mut next = cur.pointer.clone();
            next.pending_data = None;
            write_pointer(&paths.pointer_path, &next)?;
            cur.pointer = next;
        }
        Ok(status_impl(paths))
    })
    .await
}

/// Opens the folder in the system file manager (created first if missing).
#[tauri::command]
#[specta::specta]
pub async fn storage_open(app: AppHandle, kind: StorageKind) -> AppResult<()> {
    blocking(app, move |app, paths| {
        let dir = paths.get(kind);
        std::fs::create_dir_all(&dir).map_err(|e| AppError::PathNotFound(e.to_string()))?;
        app.opener()
            .open_path(dir.display().to_string(), None::<&str>)
            .map_err(|e| AppError::Internal(e.to_string()))
    })
    .await
}

/// Deletes everything in the cache folder and the in-memory repository lists.
#[tauri::command]
#[specta::specta]
pub async fn storage_clear_cache(app: AppHandle) -> AppResult<CleanReport> {
    blocking(app, |_, paths| {
        let report = nanomgg_app_paths::clear_dir_contents(&paths.get(StorageKind::Cache));
        crate::clone::clear_list_cache();
        tracing::info!(?report, "cache cleared");
        Ok(report)
    })
    .await
}

/// Deletes old log files, keeping the one being written today.
#[tauri::command]
#[specta::specta]
pub async fn storage_prune_logs(app: AppHandle) -> AppResult<CleanReport> {
    blocking(app, |_, paths| {
        let report = nanomgg_app_paths::prune_logs(
            &paths.get(StorageKind::Log),
            nanomgg_logging::LOG_FILE_PREFIX,
        );
        tracing::info!(?report, "old logs deleted");
        Ok(report)
    })
    .await
}

/// Restarts the app (after the user confirmed, to apply a scheduled data move).
#[tauri::command]
#[specta::specta]
pub fn storage_restart(app: AppHandle) {
    tracing::info!("restarting to apply storage changes");
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, AppPaths) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("local");
        let defaults = Locations {
            log: root.join("logs"),
            cache: root.join("cache"),
            data: tmp.path().join("roaming"),
        };
        std::fs::create_dir_all(&defaults.data).unwrap();
        std::fs::write(defaults.data.join("nanomgg.db"), "db").unwrap();
        let paths = AppPaths::from_defaults(defaults, root);
        (tmp, paths)
    }

    #[test]
    fn overview_lists_every_kind() {
        let (_tmp, paths) = setup();
        let o = overview_impl(&paths);
        assert_eq!(o.locations.len(), 3);
        let data = &o.locations[2];
        assert_eq!(data.kind, StorageKind::Data);
        assert!(data.is_default && data.usage_complete);
        assert_eq!((data.used_bytes, data.file_count), (2, 1));
        assert!(data.free_bytes.is_some());
        let json = serde_json::to_value(&o).unwrap();
        assert_eq!(json["locations"][0]["kind"], "log");
        assert!(json["locations"][0]["usedBytes"].is_number());
    }

    #[test]
    fn check_reports_problems_without_failing() {
        let (tmp, paths) = setup();
        let inside = paths.defaults.data.join("x");
        let c = check_impl(&paths, StorageKind::Cache, &inside);
        assert_eq!(c.problem, Some(TargetProblem::InsideAppFolder));
        let ok = check_impl(&paths, StorageKind::Data, &tmp.path().join("new-data"));
        assert_eq!(ok.problem, None, "{ok:?}");
        assert_eq!(ok.needed_bytes, 2);
        assert!(!tmp.path().join("new-data").exists());
    }

    #[test]
    fn data_change_is_scheduled_then_applied_on_restart() {
        let (tmp, paths) = setup();
        let to = tmp.path().join("D").join("data");
        // Schedule like `storage_change` does (no AppHandle needed for the data kind).
        let check = check_impl(&paths, StorageKind::Data, &to);
        assert_eq!(check.problem, None);
        {
            let mut cur = paths.current();
            let mut next = cur.pointer.clone();
            next.pending_data = Some(PendingDataMove {
                from: cur.locations.data.clone(),
                to: to.clone(),
            });
            write_pointer(&paths.pointer_path, &next).unwrap();
            cur.pointer = next;
        }
        assert_eq!(
            status_impl(&paths).pending_data,
            Some(to.display().to_string())
        );
        // Next start.
        let again = AppPaths::from_defaults(paths.defaults.clone(), paths.app_root.clone());
        let moved = again.apply_pending_data().unwrap();
        assert_eq!(moved.to, to);
        assert_eq!(again.data_dir(), to);
        assert_eq!(
            std::fs::read_to_string(to.join("nanomgg.db")).unwrap(),
            "db"
        );
        assert!(!paths.defaults.data.join("nanomgg.db").exists());
        assert_eq!(status_impl(&again).pending_data, None);
    }

    #[test]
    fn data_of_the_old_identifier_is_copied_once() {
        let tmp = tempfile::tempdir().unwrap();
        let (roaming, local) = (tmp.path().join("Roaming"), tmp.path().join("Local"));
        let old_data = roaming.join("old.id");
        std::fs::create_dir_all(old_data.join("ssh")).unwrap();
        std::fs::write(old_data.join(DB_FILE), "db").unwrap();
        std::fs::write(old_data.join("ssh").join("k1"), "key").unwrap();
        std::fs::create_dir_all(local.join("old.id")).unwrap();
        std::fs::write(local.join("old.id").join(POINTER_FILE), "{}").unwrap();
        let (data, new_local) = (roaming.join("new.id"), local.join("new.id"));

        let moved = migrate_legacy_from(&data, &new_local, &["old.id"])
            .unwrap()
            .unwrap();
        assert_eq!(
            (moved.from, moved.to.clone()),
            (old_data.clone(), data.clone())
        );
        assert_eq!(std::fs::read_to_string(data.join(DB_FILE)).unwrap(), "db");
        assert_eq!(
            std::fs::read_to_string(data.join("ssh").join("k1")).unwrap(),
            "key"
        );
        assert!(new_local.join(POINTER_FILE).is_file());
        // The old folder stays; a second start does nothing.
        assert!(old_data.join(DB_FILE).is_file());
        std::fs::write(data.join(DB_FILE), "newer").unwrap();
        assert!(
            migrate_legacy_from(&data, &new_local, &["old.id"])
                .unwrap()
                .is_none()
        );
        assert_eq!(
            std::fs::read_to_string(data.join(DB_FILE)).unwrap(),
            "newer"
        );
    }

    #[test]
    fn fresh_install_without_old_data_does_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let (data, local) = (
            tmp.path().join("R").join("new.id"),
            tmp.path().join("L").join("new.id"),
        );
        assert!(
            migrate_legacy_from(&data, &local, &["old.id"])
                .unwrap()
                .is_none()
        );
        assert!(!data.exists());
    }

    #[test]
    fn moved_key_paths_are_rewritten() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = Storage::open(&tmp.path().join("db.sqlite")).unwrap();
        let (from, to) = (tmp.path().join("old"), tmp.path().join("new"));
        let key = nanomgg_accounts::SshKey {
            id: "k1".into(),
            account_id: None,
            key_type: "ed25519".into(),
            fingerprint: "SHA256:abc".into(),
            public_key: "ssh-ed25519 AAAA".into(),
            private_key_path: Some(from.join("ssh").join("k1").display().to_string()),
            has_passphrase: false,
            uploaded_to_provider: false,
            provider_key_id: None,
            comment: None,
            created_at: 0,
        };
        storage.insert_ssh_key(&key).unwrap();
        after_data_move(
            &storage,
            &nanomgg_app_paths::DataMoved {
                from,
                to: to.clone(),
            },
        );
        let path = storage.ssh_key("k1").unwrap().unwrap().private_key_path;
        assert_eq!(path, Some(to.join("ssh").join("k1").display().to_string()));
    }
}
