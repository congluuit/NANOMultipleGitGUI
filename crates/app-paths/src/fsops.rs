//! File-system work: folder size, write probe, copy-verify-delete moves, cleanup.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use serde::Serialize;

use crate::rules::{fold, key};

/// Size of a folder. `complete == false` when the walk hit its deadline (the numbers are a
/// lower bound) — huge folders never block the UI.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub bytes: u64,
    pub files: u64,
    pub complete: bool,
}

/// Total size of the regular files below `root`, skipping symlinks and `exclude`d paths.
/// A missing folder is empty.
pub fn dir_usage(root: &Path, exclude: &[PathBuf], deadline: Instant) -> Usage {
    let mut usage = Usage {
        complete: true,
        ..Usage::default()
    };
    let excluder = Excluder::new(root, exclude);
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let Ok(entries) = fs::read_dir(root.join(&rel)) else {
            continue;
        };
        for entry in entries.flatten() {
            if Instant::now() >= deadline {
                usage.complete = false;
                return usage;
            }
            let rel = rel.join(entry.file_name());
            if excluder.excluded(&rel) {
                continue;
            }
            // `DirEntry::file_type` does not follow symlinks.
            let Ok(ty) = entry.file_type() else {
                continue;
            };
            if ty.is_dir() {
                stack.push(rel);
            } else if ty.is_file() {
                usage.bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
                usage.files += 1;
            }
        }
    }
    usage
}

/// Matches paths below a root against excluded paths, canonicalizing only once.
struct Excluder {
    root: PathBuf,
    excluded: Vec<PathBuf>,
}

impl Excluder {
    fn new(root: &Path, exclude: &[PathBuf]) -> Self {
        let root_key = key(root);
        Self {
            excluded: exclude
                .iter()
                .map(|e| key(e))
                .filter(|e| e.starts_with(&root_key))
                .collect(),
            root: root_key,
        }
    }

    fn excluded(&self, rel: &Path) -> bool {
        !self.excluded.is_empty() && self.excluded.contains(&fold(self.root.join(rel)))
    }
}

/// Free bytes on the volume holding `path` (or its nearest existing parent).
pub fn free_space(path: &Path) -> Option<u64> {
    let existing = path.ancestors().find(|p| p.exists())?;
    fs4::available_space(existing).ok()
}

/// Creates, writes, reads back and deletes a probe file in `dir` (which must exist).
pub fn check_writable(dir: &Path) -> io::Result<()> {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let probe = dir.join(format!(
        ".nanomgg-write-test-{}-{nanos}",
        std::process::id()
    ));
    let result = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe)?;
        f.write_all(b"nanomgg")?;
        f.sync_all()?;
        drop(f);
        if fs::read(&probe)? != b"nanomgg" {
            return Err(io::Error::other("the test file read back differently"));
        }
        Ok(())
    })();
    let removed = fs::remove_file(&probe);
    result?;
    removed
}

/// Stage of a move, for progress events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum MovePhase {
    Copying,
    Verifying,
    Cleaning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveProgress {
    pub phase: MovePhase,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub done_files: u64,
    pub total_files: u64,
}

/// A failed copy or verification; the target has been cleaned up and the source is intact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveError {
    pub path: PathBuf,
    pub message: String,
}

impl std::fmt::Display for MoveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

impl std::error::Error for MoveError {}

fn move_err(path: &Path, e: impl std::fmt::Display) -> MoveError {
    MoveError {
        path: path.to_owned(),
        message: e.to_string(),
    }
}

/// A verified copy of a folder. Either [`CopiedTree::remove_source`] once the new location is
/// in use, or [`CopiedTree::rollback`] to undo the copy.
#[derive(Debug)]
#[must_use = "remove the source or roll the copy back"]
pub struct CopiedTree {
    pub from: PathBuf,
    pub to: PathBuf,
    /// Copied files, relative to `from` / `to`.
    pub files: Vec<PathBuf>,
    pub bytes: u64,
    /// Folders created in the target (parents first).
    created_dirs: Vec<PathBuf>,
}

/// Copies every regular file below `from` into `to` (created if needed), then compares each
/// copy byte for byte with its source. Symlinks, special files (sockets) and `exclude`d
/// paths are skipped. On any error everything copied so far is removed again and the source
/// is left as it was.
pub fn copy_tree(
    from: &Path,
    to: &Path,
    exclude: &[PathBuf],
    progress: &mut dyn FnMut(&MoveProgress),
) -> Result<CopiedTree, MoveError> {
    let (files, dirs, total_bytes) = scan(from, exclude)?;
    let mut tree = CopiedTree {
        from: from.to_owned(),
        to: to.to_owned(),
        files: Vec::with_capacity(files.len()),
        bytes: total_bytes,
        created_dirs: Vec::new(),
    };
    match copy_and_verify(&mut tree, &files, &dirs, total_bytes, progress) {
        Ok(()) => Ok(tree),
        Err(e) => {
            tree.rollback();
            Err(e)
        }
    }
}

type Scan = (Vec<(PathBuf, u64)>, Vec<PathBuf>, u64);

fn scan(from: &Path, exclude: &[PathBuf]) -> Result<Scan, MoveError> {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    let mut total = 0;
    if !from.exists() {
        return Ok((files, dirs, total));
    }
    let excluder = Excluder::new(from, exclude);
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let dir = from.join(&rel);
        for entry in fs::read_dir(&dir).map_err(|e| move_err(&dir, e))? {
            let entry = entry.map_err(|e| move_err(&dir, e))?;
            let path = entry.path();
            let rel = rel.join(entry.file_name());
            if excluder.excluded(&rel) {
                continue;
            }
            let ty = entry.file_type().map_err(|e| move_err(&path, e))?;
            if ty.is_dir() {
                dirs.push(rel.clone());
                stack.push(rel);
            } else if ty.is_file() {
                let len = entry.metadata().map_err(|e| move_err(&path, e))?.len();
                total += len;
                files.push((rel, len));
            }
        }
    }
    Ok((files, dirs, total))
}

fn copy_and_verify(
    tree: &mut CopiedTree,
    files: &[(PathBuf, u64)],
    dirs: &[PathBuf],
    total_bytes: u64,
    progress: &mut dyn FnMut(&MoveProgress),
) -> Result<(), MoveError> {
    let total_files = files.len() as u64;
    let mut report = |phase, done_bytes, done_files| {
        progress(&MoveProgress {
            phase,
            done_bytes,
            total_bytes,
            done_files,
            total_files,
        })
    };
    create_dir(tree, &tree.to.clone())?;
    for rel in dirs {
        create_dir(tree, &tree.to.join(rel))?;
    }
    let mut done = 0;
    report(MovePhase::Copying, 0, 0);
    for (i, (rel, len)) in files.iter().enumerate() {
        let src = tree.from.join(rel);
        let dst = tree.to.join(rel);
        // Recorded first: a half-written copy is removed by the rollback too.
        tree.files.push(rel.clone());
        fs::copy(&src, &dst).map_err(|e| move_err(&src, e))?;
        done += len;
        report(MovePhase::Copying, done, i as u64 + 1);
    }
    done = 0;
    report(MovePhase::Verifying, 0, 0);
    for (i, (rel, len)) in files.iter().enumerate() {
        let src = tree.from.join(rel);
        let dst = tree.to.join(rel);
        if !same_content(&src, &dst).map_err(|e| move_err(&dst, e))? {
            return Err(move_err(&dst, "the copy differs from the original"));
        }
        done += len;
        report(MovePhase::Verifying, done, i as u64 + 1);
    }
    Ok(())
}

fn create_dir(tree: &mut CopiedTree, dir: &Path) -> Result<(), MoveError> {
    if dir.is_dir() {
        return Ok(());
    }
    // Parents first, so the rollback (reverse order) removes children first.
    if let Some(parent) = dir.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        create_dir(tree, parent)?;
    }
    fs::create_dir(dir).map_err(|e| move_err(dir, e))?;
    tree.created_dirs.push(dir.to_owned());
    Ok(())
}

fn same_content(a: &Path, b: &Path) -> io::Result<bool> {
    let (mut fa, mut fb) = (fs::File::open(a)?, fs::File::open(b)?);
    if fa.metadata()?.len() != fb.metadata()?.len() {
        return Ok(false);
    }
    let (mut ba, mut bb) = (vec![0u8; 64 * 1024], vec![0u8; 64 * 1024]);
    loop {
        let n = read_full(&mut fa, &mut ba)?;
        let m = read_full(&mut fb, &mut bb)?;
        if n != m || ba[..n] != bb[..m] {
            return Ok(false);
        }
        if n == 0 {
            return Ok(true);
        }
    }
}

fn read_full(f: &mut fs::File, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

impl CopiedTree {
    /// Removes the copies (and folders created for them); the source is untouched.
    pub fn rollback(self) {
        for rel in self.files.iter().rev() {
            let _ = fs::remove_file(self.to.join(rel));
        }
        for dir in self.created_dirs.iter().rev() {
            let _ = fs::remove_dir(dir);
        }
    }

    /// Deletes the copied originals and the folders left empty (the source folder itself
    /// included). Returns the files that could not be deleted.
    pub fn remove_source(self) -> Vec<PathBuf> {
        let mut left = Vec::new();
        for rel in &self.files {
            let path = self.from.join(rel);
            if let Err(e) = fs::remove_file(&path)
                && e.kind() != io::ErrorKind::NotFound
            {
                left.push(path);
            }
        }
        let mut dirs: Vec<PathBuf> = self
            .files
            .iter()
            .flat_map(|rel| rel.ancestors().skip(1).map(Path::to_path_buf))
            .filter(|d| !d.as_os_str().is_empty())
            .collect();
        dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
        dirs.dedup();
        for dir in dirs {
            let _ = fs::remove_dir(self.from.join(dir));
        }
        let _ = fs::remove_dir(&self.from);
        left
    }
}

/// Outcome of a cleanup.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CleanReport {
    pub removed_files: u32,
    pub removed_bytes: u64,
    /// Entries that could not be deleted (in use by another program?).
    pub failed: u32,
}

/// Deletes everything inside `dir` (the folder itself stays). A missing folder is fine.
pub fn clear_dir_contents(dir: &Path) -> CleanReport {
    let mut report = CleanReport::default();
    let Ok(entries) = fs::read_dir(dir) else {
        return report;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ty) = entry.file_type() else {
            report.failed += 1;
            continue;
        };
        if ty.is_dir() {
            let usage = dir_usage(&path, &[], far_future());
            match fs::remove_dir_all(&path) {
                Ok(()) => {
                    report.removed_files += u32::try_from(usage.files).unwrap_or(u32::MAX);
                    report.removed_bytes += usage.bytes;
                }
                Err(_) => report.failed += 1,
            }
        } else {
            let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
            match fs::remove_file(&path) {
                Ok(()) => {
                    report.removed_files += 1;
                    report.removed_bytes += len;
                }
                Err(_) => report.failed += 1,
            }
        }
    }
    report
}

fn far_future() -> Instant {
    Instant::now() + std::time::Duration::from_secs(3600)
}

/// Deletes old log files (`<prefix>*.log`) in `dir`, keeping the most recently written one —
/// the file the app is writing to today.
pub fn prune_logs(dir: &Path, prefix: &str) -> CleanReport {
    let mut report = CleanReport::default();
    let Ok(entries) = fs::read_dir(dir) else {
        return report;
    };
    let mut logs: Vec<(PathBuf, SystemTime, u64)> = entries
        .flatten()
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with(prefix) && name.ends_with(".log")
        })
        .filter_map(|e| {
            let meta = e.metadata().ok().filter(|m| m.is_file())?;
            let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            Some((e.path(), modified, meta.len()))
        })
        .collect();
    // Newest first; ties broken by name (the dated file name sorts by day).
    logs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
    for (path, _, len) in logs.into_iter().skip(1) {
        match fs::remove_file(&path) {
            Ok(()) => {
                report.removed_files += 1;
                report.removed_bytes += len;
            }
            Err(_) => report.failed += 1,
        }
    }
    report
}
