//! Uploading and downloading LFS objects with progress, cancellation and translated errors
//! (PLAN §6.4, §6.5).
//!
//! - **Estimate**: [`push_estimate`] runs `git lfs push --dry-run <remote> <branch>` (objects
//!   not yet referenced by the local copy of the remote branch) and reads sizes from the local
//!   LFS store. It cannot know which objects the *server* already has: git-lfs asks that in
//!   the Batch API during the real push and silently skips them, so the estimate is an upper
//!   bound (re-pushing a branch whose objects were uploaded earlier shows them again, then
//!   finishes almost instantly). [`download_estimate`] uses `git lfs ls-files --json`, which
//!   also says which objects are already downloaded, and checks the free disk space.
//! - **Upload**: [`push`] runs `git lfs push <remote> <branch>` with `GIT_LFS_PROGRESS` and
//!   folds the progress file into totals, speed and ETA ([`progress`]).
//! - **Download**: [`download`] runs `git lfs pull [--include=<path>]` or
//!   `git lfs fetch --all`, with the same progress and a disk-space check first.
//! - **Cancel / resume**: the [`CancelToken`] kills git-lfs and every child (Windows Job
//!   Object, see `nanomgg_git_engine::progress`). LFS objects are content-addressed and the
//!   server only stores complete objects, so running the same command again transfers only
//!   what is missing (resume at file granularity).
//! - **Errors**: failures are translated into an [`LfsFailure`] ([`errors`]).
//!
//! Every function takes the repository's [`AccountContext`]: it is injected through the
//! environment, so git-lfs (and the credential helper it calls) runs as the bound account.

pub mod errors;
pub mod progress;

use std::path::{Path, PathBuf};
use std::time::Instant;

use nanomgg_git_engine::branch::validate_branch_name;
use nanomgg_git_engine::ops::OpError;
use nanomgg_git_engine::progress::{CancelToken, run_streaming_with};
use nanomgg_git_engine::remote::validate_remote;
use nanomgg_git_engine::{AccountContext, GitError, GitRunner};
use serde::{Deserialize, Serialize};

pub use errors::{LfsFailure, LfsFailureKind};
pub use progress::{LfsDirection, LfsFileProgress, LfsTransferStats};

use crate::LfsError;
use crate::setup::normalize_path;
use progress::{ProgressFile, ProgressTracker, parse_progress_line};

/// Tunables passed as `-c` to each command (Settings → LFS, overridable per repository).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferOptions {
    /// `lfs.concurrenttransfers` (git-lfs default: 8).
    #[cfg_attr(feature = "specta", specta(optional))]
    #[serde(default)]
    pub concurrent_transfers: Option<u32>,
    /// `lfs.transfer.maxretries` (default: 8).
    #[cfg_attr(feature = "specta", specta(optional))]
    #[serde(default)]
    pub max_retries: Option<u32>,
    /// `lfs.activitytimeout` in seconds (default: 30).
    #[cfg_attr(feature = "specta", specta(optional))]
    #[serde(default)]
    pub activity_timeout_secs: Option<u32>,
}

impl TransferOptions {
    /// `-c key=value` pairs for the git command line (values are clamped to sane ranges).
    pub fn config_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        let mut set = |key: &str, value: Option<u32>, min: u32, max: u32| {
            if let Some(v) = value {
                args.push("-c".to_owned());
                args.push(format!("{key}={}", v.clamp(min, max)));
            }
        };
        set("lfs.concurrenttransfers", self.concurrent_transfers, 1, 64);
        set("lfs.transfer.maxretries", self.max_retries, 0, 100);
        set("lfs.activitytimeout", self.activity_timeout_secs, 1, 3600);
        args
    }
}

/// One LFS object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LfsObject {
    pub oid: String,
    /// Bytes; `0` when unknown (object missing from the local store).
    pub size: u64,
    /// Repository path the object was found at.
    pub path: String,
}

/// What `git lfs push` would upload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PushEstimate {
    pub remote: String,
    pub branch: String,
    pub objects: Vec<LfsObject>,
    pub total_bytes: u64,
    /// Objects whose size could not be read locally (not counted in `total_bytes`).
    pub unknown_sizes: u32,
}

/// What a download would transfer, and whether it fits on the disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DownloadEstimate {
    /// Objects not downloaded yet.
    pub objects: Vec<LfsObject>,
    /// Bytes to download.
    pub missing_bytes: u64,
    /// Disk space needed: the objects, plus their working-tree copies for a pull.
    pub needed_bytes: u64,
    /// Free bytes on the repository's volume (`None` if unknown).
    pub free_bytes: Option<u64>,
    pub enough_space: bool,
}

/// What to download.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    /// Remote to download from; `None` = git-lfs' default (upstream remote or `origin`).
    #[serde(default)]
    pub remote: Option<String>,
    /// Single repository path (or `lfs.fetchinclude`-style pattern) for `git lfs pull`.
    #[serde(default)]
    pub include: Option<String>,
    /// `git lfs fetch --all`: every object of every ref, without checkout.
    #[serde(default)]
    pub all: bool,
    /// Start even when the estimate says the disk is too small.
    #[serde(default)]
    pub ignore_disk_space: bool,
}

/// Result of a finished transfer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferSummary {
    /// Files git-lfs reported as transferred (objects already present are skipped).
    pub files: u32,
    pub bytes: u64,
    pub elapsed_ms: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    #[error("transfer cancelled")]
    Cancelled,
    /// git-lfs failed; translated.
    #[error("{0}")]
    Failed(LfsFailure),
    /// Invalid remote / branch / path argument.
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Lfs(#[from] LfsError),
}

impl From<GitError> for TransferError {
    fn from(e: GitError) -> Self {
        Self::Lfs(LfsError::Git(e))
    }
}

impl From<std::io::Error> for TransferError {
    fn from(e: std::io::Error) -> Self {
        Self::Lfs(LfsError::Io(e))
    }
}

fn invalid(e: OpError) -> TransferError {
    match e {
        OpError::Git(g) => g.into(),
        other => TransferError::Invalid(other.to_string()),
    }
}

/// Free bytes for the current user on the volume holding `path`.
pub fn free_space(path: &Path) -> Option<u64> {
    fs4::available_space(path).ok()
}

/// Parses `git lfs push --dry-run` output: `push <oid> => <path>`.
pub fn parse_dry_run(stdout: &str) -> Vec<(String, String)> {
    stdout
        .lines()
        .filter_map(|l| {
            let rest = l.strip_prefix("push ")?;
            let (oid, path) = rest.split_once(" => ")?;
            let oid = oid.trim();
            (oid.len() == 64 && oid.chars().all(|c| c.is_ascii_hexdigit()))
                .then(|| (oid.to_owned(), path.trim_end_matches('\r').to_owned()))
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct LsFiles {
    files: Option<Vec<LsFile>>,
}

#[derive(Debug, Deserialize)]
struct LsFile {
    name: String,
    size: u64,
    oid: String,
    #[serde(default)]
    downloaded: bool,
}

/// Parses `git lfs ls-files --json` into `(object, downloaded)` pairs, one per oid.
pub fn parse_ls_files(json: &str) -> Result<Vec<(LfsObject, bool)>, serde_json::Error> {
    let parsed: LsFiles = serde_json::from_str(json)?;
    let mut seen = std::collections::HashSet::new();
    Ok(parsed
        .files
        .unwrap_or_default()
        .into_iter()
        .filter(|f| seen.insert(f.oid.clone()))
        .map(|f| {
            (
                LfsObject {
                    oid: f.oid,
                    size: f.size,
                    path: f.name,
                },
                f.downloaded,
            )
        })
        .collect())
}

/// `LocalMediaDir` from `git lfs env` (honours `lfs.storage`).
fn media_dir(git: &GitRunner, repo: &Path, ctx: &AccountContext) -> Option<PathBuf> {
    let out = git.run_text(repo, ctx, ["lfs", "env"]).ok()?;
    out.lines()
        .find_map(|l| l.strip_prefix("LocalMediaDir="))
        .map(|p| PathBuf::from(p.trim()))
        .filter(|p| !p.as_os_str().is_empty())
}

fn object_size(media: &Path, oid: &str) -> Option<u64> {
    let file = media.join(&oid[0..2]).join(&oid[2..4]).join(oid);
    std::fs::metadata(file).ok().map(|m| m.len())
}

/// URL of `remote` (or of the default remote) for provider links; no network access.
fn remote_url(git: &GitRunner, repo: &Path, remote: Option<&str>) -> Option<String> {
    let mut args = vec!["ls-remote", "--get-url"];
    if let Some(r) = remote {
        args.push(r);
    }
    git.run_text(repo, &AccountContext::default(), &args)
        .ok()
        .filter(|u| !u.is_empty())
}

fn translate_op(e: OpError, url: Option<&str>) -> TransferError {
    match e {
        OpError::Cancelled => TransferError::Cancelled,
        OpError::Rejected { message, .. } => {
            TransferError::Failed(errors::translate(&message, url))
        }
        OpError::Git(GitError::Failed { stderr, .. }) => {
            TransferError::Failed(errors::translate(&stderr, url))
        }
        OpError::Git(e) => e.into(),
    }
}

/// What `git lfs push <remote> <branch>` would upload (see the module docs for the limits).
pub fn push_estimate(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    remote: &str,
    branch: &str,
) -> Result<PushEstimate, TransferError> {
    validate_remote(git, repo, remote).map_err(invalid)?;
    validate_branch_name(git, repo, branch).map_err(invalid)?;
    let url = remote_url(git, repo, Some(remote));
    let out = git
        .run(repo, ctx, ["lfs", "push", "--dry-run", remote, branch])
        .map_err(|e| match e {
            GitError::Failed { stderr, .. } => {
                TransferError::Failed(errors::translate(&stderr, url.as_deref()))
            }
            other => other.into(),
        })?;
    let listed = parse_dry_run(&String::from_utf8_lossy(&out.stdout));
    let media = if listed.is_empty() {
        None
    } else {
        media_dir(git, repo, ctx)
    };
    let mut seen = std::collections::HashSet::new();
    let objects: Vec<LfsObject> = listed
        .into_iter()
        .filter(|(oid, _)| seen.insert(oid.clone()))
        .map(|(oid, path)| LfsObject {
            size: media
                .as_deref()
                .and_then(|m| object_size(m, &oid))
                .unwrap_or(0),
            oid,
            path,
        })
        .collect();
    Ok(PushEstimate {
        remote: remote.to_owned(),
        branch: branch.to_owned(),
        total_bytes: objects.iter().map(|o| o.size).sum(),
        unknown_sizes: objects.iter().filter(|o| o.size == 0).count() as u32,
        objects,
    })
}

/// Runs one git-lfs transfer command with a progress file; returns the final stats.
#[allow(clippy::too_many_arguments)]
fn run_transfer<F: FnMut(&LfsTransferStats)>(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    args: Vec<String>,
    direction: LfsDirection,
    estimate: Option<&[LfsObject]>,
    url: Option<&str>,
    cancel: &CancelToken,
    mut on_progress: F,
) -> Result<TransferSummary, TransferError> {
    let started = Instant::now();
    let mut file = ProgressFile::create()?;
    let env = vec![(
        "GIT_LFS_PROGRESS".to_owned(),
        file.path().to_string_lossy().into_owned(),
    )];
    let mut tracker = ProgressTracker::new(direction, estimate);
    let result = run_streaming_with(
        git,
        repo,
        ctx,
        &args,
        &env,
        cancel,
        |_| {},
        || {
            let lines = file.read_lines();
            if lines.is_empty() {
                return;
            }
            let now = Instant::now();
            for line in lines.iter().filter_map(|l| parse_progress_line(l)) {
                tracker.update(&line, now);
            }
            on_progress(&tracker.snapshot(now, false));
        },
    );
    result.map_err(|e| translate_op(e, url))?;
    let last = tracker.snapshot(Instant::now(), true);
    on_progress(&last);
    let reported = tracker.snapshot(Instant::now(), false);
    Ok(TransferSummary {
        files: reported.files_done,
        bytes: reported.bytes_done,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

/// `git lfs push <remote> <branch>` with progress. Objects the server already has are
/// skipped by git-lfs, so re-running after a cancel uploads only what is missing.
#[allow(clippy::too_many_arguments)]
pub fn push<F: FnMut(&LfsTransferStats)>(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    remote: &str,
    branch: &str,
    opts: &TransferOptions,
    estimate: Option<&PushEstimate>,
    cancel: &CancelToken,
    on_progress: F,
) -> Result<TransferSummary, TransferError> {
    validate_remote(git, repo, remote).map_err(invalid)?;
    validate_branch_name(git, repo, branch).map_err(invalid)?;
    let url = remote_url(git, repo, Some(remote));
    let mut args = opts.config_args();
    args.extend(["lfs", "push", remote, branch].map(String::from));
    run_transfer(
        git,
        repo,
        ctx,
        args,
        LfsDirection::Upload,
        estimate.map(|e| e.objects.as_slice()),
        url.as_deref(),
        cancel,
        on_progress,
    )
}

fn check_include(include: Option<&str>) -> Result<Option<String>, TransferError> {
    include
        .map(|p| {
            if p.contains(',') {
                // git-lfs splits include lists on commas.
                return Err(TransferError::Invalid(format!("invalid path: {p:?}")));
            }
            normalize_path(p).map_err(TransferError::from)
        })
        .transpose()
}

/// Objects a download would transfer and whether they fit on the disk.
pub fn download_estimate(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    req: &DownloadRequest,
) -> Result<DownloadEstimate, TransferError> {
    let include = check_include(req.include.as_deref())?;
    let mut args = vec!["lfs".to_owned(), "ls-files".to_owned(), "--json".to_owned()];
    if req.all {
        args.push("--all".into());
    } else if let Some(p) = &include {
        args.push(format!("--include={p}"));
    }
    let out = git.run_text(repo, ctx, &args)?;
    let objects: Vec<LfsObject> = parse_ls_files(&out)
        .map_err(|e| TransferError::Invalid(format!("unexpected git lfs ls-files output: {e}")))?
        .into_iter()
        .filter(|(_, downloaded)| !downloaded)
        .map(|(o, _)| o)
        .collect();
    let missing_bytes: u64 = objects.iter().map(|o| o.size).sum();
    // A pull also writes each file into the working tree.
    let needed_bytes = if req.all {
        missing_bytes
    } else {
        missing_bytes.saturating_mul(2)
    };
    let free_bytes = free_space(repo);
    Ok(DownloadEstimate {
        enough_space: free_bytes.is_none_or(|free| free >= needed_bytes),
        objects,
        missing_bytes,
        needed_bytes,
        free_bytes,
    })
}

/// `filter.lfs.*` passed with `-c` when the repository/user config lacks them, so
/// `git lfs pull` can check files out even if `git lfs install` was never run.
fn filter_args(git: &GitRunner, repo: &Path) -> Vec<String> {
    let configured = git
        .run_text(
            repo,
            &AccountContext::default(),
            ["config", "--get", "filter.lfs.process"],
        )
        .is_ok_and(|v| !v.is_empty());
    if configured {
        return Vec::new();
    }
    [
        "filter.lfs.process=git-lfs filter-process",
        "filter.lfs.smudge=git-lfs smudge -- %f",
        "filter.lfs.clean=git-lfs clean -- %f",
        "filter.lfs.required=true",
    ]
    .iter()
    .flat_map(|kv| ["-c".to_owned(), (*kv).to_owned()])
    .collect()
}

/// `git lfs pull [--include=<path>] [<remote>]` or `git lfs fetch --all [<remote>]`, with
/// progress. Checks the free disk space first ([`LfsFailureKind::InsufficientDiskSpace`]
/// unless `ignore_disk_space`). Used for the clone option "download later" too.
pub fn download<F: FnMut(&LfsTransferStats)>(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    req: &DownloadRequest,
    opts: &TransferOptions,
    cancel: &CancelToken,
    on_progress: F,
) -> Result<TransferSummary, TransferError> {
    if let Some(remote) = &req.remote {
        validate_remote(git, repo, remote).map_err(invalid)?;
    }
    let include = check_include(req.include.as_deref())?;
    if req.all && include.is_some() {
        return Err(TransferError::Invalid(
            "`fetch --all` cannot be limited to a path".into(),
        ));
    }
    let estimate = download_estimate(git, repo, ctx, req)?;
    if !estimate.enough_space && !req.ignore_disk_space {
        let free = estimate.free_bytes.unwrap_or_default();
        return Err(TransferError::Failed(LfsFailure {
            kind: LfsFailureKind::InsufficientDiskSpace,
            message: format!(
                "not enough disk space: {} bytes needed, {free} bytes free",
                estimate.needed_bytes
            ),
            retryable: true,
            billing_url: None,
            settings_url: None,
            limit_bytes: Some(free),
            object: None,
        }));
    }
    let url = remote_url(git, repo, req.remote.as_deref());
    let mut args = opts.config_args();
    if req.all {
        args.extend(["lfs", "fetch", "--all"].map(String::from));
    } else {
        args.extend(filter_args(git, repo));
        args.extend(["lfs", "pull"].map(String::from));
        if let Some(p) = &include {
            args.push(format!("--include={p}"));
        }
    }
    if let Some(remote) = &req.remote {
        args.push(remote.clone());
    }
    run_transfer(
        git,
        repo,
        ctx,
        args,
        LfsDirection::Download,
        Some(&estimate.objects),
        url.as_deref(),
        cancel,
        on_progress,
    )
}

/// `git lfs pull` for the whole checkout, or a single pointer file ("Tải file này").
pub fn pull<F: FnMut(&LfsTransferStats)>(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    include: Option<&str>,
    opts: &TransferOptions,
    cancel: &CancelToken,
    on_progress: F,
) -> Result<TransferSummary, TransferError> {
    let req = DownloadRequest {
        include: include.map(str::to_owned),
        ..Default::default()
    };
    download(git, repo, ctx, &req, opts, cancel, on_progress)
}

/// `git lfs fetch --all`: every object of every ref into the local store (no checkout).
pub fn fetch_all<F: FnMut(&LfsTransferStats)>(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    opts: &TransferOptions,
    cancel: &CancelToken,
    on_progress: F,
) -> Result<TransferSummary, TransferError> {
    let req = DownloadRequest {
        all: true,
        ..Default::default()
    };
    download(git, repo, ctx, &req, opts, cancel, on_progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_args_follow_options() {
        assert!(TransferOptions::default().config_args().is_empty());
        let opts = TransferOptions {
            concurrent_transfers: Some(0),
            max_retries: Some(500),
            activity_timeout_secs: Some(99_999),
        };
        assert_eq!(
            opts.config_args(),
            [
                "-c",
                "lfs.concurrenttransfers=1",
                "-c",
                "lfs.transfer.maxretries=100",
                "-c",
                "lfs.activitytimeout=3600"
            ]
        );
    }

    #[test]
    fn parses_dry_run_output() {
        // Captured from `git lfs push --dry-run origin main` (git-lfs 3.5.1).
        let out = "push 1d1041d209fa4195ecbec93b03e1546848faffe74595fe32d5a8f320e4b5db64 => a.bin\n\
push fdcea362382eaccdf1973a074e026f37d8701e121d3d10889cc506074e127e05 => c.bin\r\n\
push bbd7ec509fcc45114313424102d1dbfa5cb9c9aaa72d16feaee9bdb34179ec6b => sub dir/b => c.bin\n\
Locking support detected on remote \"origin\". Consider enabling it with:\n\
push nothex => x.bin\n";
        let parsed = parse_dry_run(out);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].1, "a.bin");
        assert_eq!(parsed[1].1, "c.bin");
        assert_eq!(parsed[2].1, "sub dir/b => c.bin");
        assert!(parse_dry_run("").is_empty());
    }

    #[test]
    fn parses_ls_files_json() {
        let json = r#"{
 "files": [
  {"name": "a.bin", "size": 31457280, "checkout": false, "downloaded": false,
   "oid_type": "sha256", "oid": "1d1041d209fa4195ecbec93b03e1546848faffe74595fe32d5a8f320e4b5db64",
   "version": "https://git-lfs.github.com/spec/v1"},
  {"name": "copy/a.bin", "size": 31457280, "checkout": false, "downloaded": false,
   "oid_type": "sha256", "oid": "1d1041d209fa4195ecbec93b03e1546848faffe74595fe32d5a8f320e4b5db64",
   "version": "https://git-lfs.github.com/spec/v1"},
  {"name": "sub dir/b.bin", "size": 20971520, "checkout": true, "downloaded": true,
   "oid_type": "sha256", "oid": "bbd7ec509fcc45114313424102d1dbfa5cb9c9aaa72d16feaee9bdb34179ec6b",
   "version": "https://git-lfs.github.com/spec/v1"}
 ]
}"#;
        let files = parse_ls_files(json).unwrap();
        assert_eq!(files.len(), 2, "duplicate oids are merged");
        assert_eq!(files[0].0.path, "a.bin");
        assert!(!files[0].1);
        assert!(files[1].1);
        assert!(parse_ls_files("{\n \"files\": null\n}").unwrap().is_empty());
    }

    #[test]
    fn include_paths_are_checked() {
        assert_eq!(check_include(None).unwrap(), None);
        assert_eq!(
            check_include(Some("./Assets\\intro.mp4"))
                .unwrap()
                .as_deref(),
            Some("Assets/intro.mp4")
        );
        for bad in ["../x", "/abs", "a,b", "C:/x"] {
            assert!(check_include(Some(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn free_space_of_temp_dir_is_known() {
        assert!(free_space(&std::env::temp_dir()).is_some_and(|b| b > 0));
    }
}
