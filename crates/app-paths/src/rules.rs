//! Safety rules for a new location.
//!
//! - The chosen folder must be absolute, without `..`.
//! - If it already holds files, a dedicated subfolder (`nanomgg-logs`, …) is used inside it;
//!   that subfolder must be empty or new. Our folders therefore never share a folder with the
//!   user's files, so "clear cache" / "delete old logs" can only ever touch app files.
//! - It must not be inside, nor contain, any current or default app folder (log, cache, data,
//!   the pointer's folder) — this also forbids moving a folder into itself.
//! - The default location of the kind is always accepted (the app owns it), even if not empty.
//! - It must be writable (probe file) and have room for the data plus a margin.

use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::{Locations, StorageKind, check_writable, free_space};

/// Extra free space required on top of the data being moved.
pub const SPACE_MARGIN: u64 = 64 * 1024 * 1024;

/// Why a folder cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TargetProblem {
    /// Not a full path, or contains `..`.
    InvalidPath,
    /// A file, not a folder.
    NotAFolder,
    SameAsCurrent,
    /// Inside the folder being moved (or another app folder).
    InsideAppFolder,
    /// Contains an app folder (would mix the user's files with ours).
    ContainsAppFolder,
    /// The `nanomgg-…` subfolder already exists and is not empty.
    NotEmpty {
        path: String,
    },
    NotWritable {
        message: String,
    },
    NotEnoughSpace {
        needed: u64,
        free: u64,
    },
}

/// The folder that will actually be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetPlan {
    pub target: PathBuf,
    /// `target` is a new subfolder of the chosen (non-empty) folder.
    pub creates_subfolder: bool,
    pub is_default: bool,
}

/// Checks the structural rules (no disk writes). `protected` = extra app folders, e.g. the
/// folder of the pointer file.
pub fn plan_target(
    kind: StorageKind,
    picked: &Path,
    current: &Locations,
    defaults: &Locations,
    protected: &[PathBuf],
) -> Result<TargetPlan, TargetProblem> {
    if !picked.is_absolute() || picked.components().any(|c| c == Component::ParentDir) {
        return Err(TargetProblem::InvalidPath);
    }
    if same_path(picked, current.get(kind)) {
        return Err(TargetProblem::SameAsCurrent);
    }
    if same_path(picked, defaults.get(kind)) {
        if is_within(picked, current.get(kind)) {
            return Err(TargetProblem::InsideAppFolder);
        }
        return Ok(TargetPlan {
            target: picked.to_owned(),
            creates_subfolder: false,
            is_default: true,
        });
    }
    if picked.exists() && !picked.is_dir() {
        return Err(TargetProblem::NotAFolder);
    }
    let (target, creates_subfolder) = if is_empty_or_missing(picked) {
        (picked.to_owned(), false)
    } else {
        let sub = picked.join(kind.subfolder());
        if sub.exists() && !sub.is_dir() {
            return Err(TargetProblem::NotAFolder);
        }
        if !is_empty_or_missing(&sub) {
            return Err(TargetProblem::NotEmpty {
                path: sub.display().to_string(),
            });
        }
        (sub, true)
    };
    let app_folders = StorageKind::ALL
        .into_iter()
        .flat_map(|k| [current.get(k), defaults.get(k)])
        .chain(protected.iter().map(PathBuf::as_path));
    for folder in app_folders {
        if is_within(&target, folder) {
            return Err(TargetProblem::InsideAppFolder);
        }
        if is_within(folder, &target) {
            return Err(TargetProblem::ContainsAppFolder);
        }
    }
    Ok(TargetPlan {
        target,
        creates_subfolder,
        is_default: false,
    })
}

fn is_empty_or_missing(dir: &Path) -> bool {
    match std::fs::read_dir(dir) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => !dir.exists(),
    }
}

/// Creates `target` if needed, probes it for writing and checks the free space. A folder
/// created only for the check is removed again.
pub fn verify_target(target: &Path, needed: u64) -> Result<u64, TargetProblem> {
    let created = !target.exists();
    if created && let Err(e) = std::fs::create_dir_all(target) {
        return Err(TargetProblem::NotWritable {
            message: e.to_string(),
        });
    }
    let result = check_writable(target)
        .map_err(|e| TargetProblem::NotWritable {
            message: e.to_string(),
        })
        .and_then(|()| {
            let free = free_space(target).unwrap_or(u64::MAX);
            let needed = needed.saturating_add(SPACE_MARGIN);
            if free < needed {
                Err(TargetProblem::NotEnoughSpace { needed, free })
            } else {
                Ok(free)
            }
        });
    if created {
        let _ = std::fs::remove_dir(target);
    }
    result
}

/// Comparable form of a path: the existing part canonicalized (symlinks, `\\?\`, short
/// names), case-folded on Windows and macOS.
pub(crate) fn key(path: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut base = path;
    let canonical = loop {
        if let Ok(c) = std::fs::canonicalize(base) {
            break c;
        }
        match (base.parent(), base.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_owned());
                base = parent;
            }
            _ => {
                rest.clear();
                break path.to_owned();
            }
        }
    };
    let mut full = canonical;
    for name in rest.into_iter().rev() {
        full.push(name);
    }
    fold(full)
}

/// Case-folds on the platforms whose file systems are case-insensitive by default.
pub(crate) fn fold(path: PathBuf) -> PathBuf {
    if cfg!(any(windows, target_os = "macos")) {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    } else {
        path
    }
}

/// Whether two paths name the same folder.
pub fn same_path(a: &Path, b: &Path) -> bool {
    a == b || key(a) == key(b)
}

/// Whether `child` is `parent` or below it.
pub fn is_within(child: &Path, parent: &Path) -> bool {
    key(child).starts_with(key(parent))
}
