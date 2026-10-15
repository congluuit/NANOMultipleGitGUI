//! Where NANOMultipleGitGUI keeps its logs, cache and app data (PLAN 3B.3).
//!
//! The locations are resolved once at startup from a small pointer file
//! (`locations.json`) that always lives at the *default* local-data location, so a moved
//! folder is found again on the next start. A broken pointer or a missing custom folder
//! falls back to the default and yields a [`StorageWarning`] for the UI — the app must
//! always open.
//!
//! The app-data folder (SQLite database, SSH keys) is in use while the app runs, so moving
//! it is only *scheduled* ([`Pointer::pending_data`]) and done by [`apply_pending_data`] on
//! the next start, before the database is opened.

mod fsops;
mod rules;

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use fsops::{
    CleanReport, CopiedTree, MoveError, MovePhase, MoveProgress, Usage, check_writable,
    clear_dir_contents, copy_tree, dir_usage, free_space, prune_logs,
};
pub use rules::{TargetPlan, TargetProblem, is_within, plan_target, same_path, verify_target};

/// Name of the pointer file inside the default local-data folder.
pub const POINTER_FILE: &str = "locations.json";
const POINTER_VERSION: u32 = 1;

/// One kind of folder the user can relocate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum StorageKind {
    Log,
    Cache,
    Data,
}

impl StorageKind {
    pub const ALL: [StorageKind; 3] = [StorageKind::Log, StorageKind::Cache, StorageKind::Data];

    /// Subfolder created inside a chosen folder that already holds other things.
    pub fn subfolder(self) -> &'static str {
        match self {
            StorageKind::Log => "nanomgg-logs",
            StorageKind::Cache => "nanomgg-cache",
            StorageKind::Data => "nanomgg-data",
        }
    }
}

/// One path per [`StorageKind`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locations {
    pub log: PathBuf,
    pub cache: PathBuf,
    pub data: PathBuf,
}

impl Locations {
    pub fn get(&self, kind: StorageKind) -> &Path {
        match kind {
            StorageKind::Log => &self.log,
            StorageKind::Cache => &self.cache,
            StorageKind::Data => &self.data,
        }
    }

    pub fn set(&mut self, kind: StorageKind, path: PathBuf) {
        match kind {
            StorageKind::Log => self.log = path,
            StorageKind::Cache => self.cache = path,
            StorageKind::Data => self.data = path,
        }
    }

    /// The folders of the *other* kinds — never copied along when `kind` moves (on macOS and
    /// Linux the default log/cache folders live inside the default data folder).
    pub fn others(&self, kind: StorageKind) -> Vec<PathBuf> {
        StorageKind::ALL
            .into_iter()
            .filter(|k| *k != kind)
            .map(|k| self.get(k).to_owned())
            .collect()
    }
}

/// A data-folder move waiting for the next start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingDataMove {
    /// The data folder in use when the move was scheduled; the move is dropped if it changed.
    pub from: PathBuf,
    pub to: PathBuf,
}

/// Content of `locations.json`. `None` = the default location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pointer {
    #[serde(default = "pointer_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_data: Option<PendingDataMove>,
}

fn pointer_version() -> u32 {
    POINTER_VERSION
}

impl Default for Pointer {
    fn default() -> Self {
        Self {
            version: POINTER_VERSION,
            log: None,
            cache: None,
            data: None,
            pending_data: None,
        }
    }
}

impl Pointer {
    pub fn get(&self, kind: StorageKind) -> Option<&Path> {
        match kind {
            StorageKind::Log => self.log.as_deref(),
            StorageKind::Cache => self.cache.as_deref(),
            StorageKind::Data => self.data.as_deref(),
        }
    }

    /// Stores `path`, or clears the entry when it is the default.
    pub fn set(&mut self, kind: StorageKind, path: &Path, defaults: &Locations) {
        let value = (!same_path(path, defaults.get(kind))).then(|| path.to_owned());
        match kind {
            StorageKind::Log => self.log = value,
            StorageKind::Cache => self.cache = value,
            StorageKind::Data => self.data = value,
        }
    }
}

/// Reads the pointer: `Ok(None)` when there is none yet.
pub fn read_pointer(path: &Path) -> Result<Option<Pointer>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| e.to_string()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Writes the pointer atomically (temp file + rename), or removes it when everything is
/// default and nothing is pending.
pub fn write_pointer(path: &Path, pointer: &Pointer) -> io::Result<()> {
    if *pointer == Pointer::default() {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(pointer).map_err(io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json + "\n")?;
    std::fs::rename(&tmp, path)
}

/// Something the user should know about the storage folders (shown once after start).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StorageWarning {
    /// `locations.json` could not be read; every folder uses its default.
    PointerUnreadable { path: String, message: String },
    /// A custom folder is gone (removable drive not plugged in?); the default is used for now.
    MissingFolder {
        storage: StorageKind,
        path: String,
        fallback: String,
    },
    /// The scheduled data-folder move finished.
    DataMoved { from: String, to: String },
    /// The scheduled data-folder move failed; the old folder is still in use, untouched.
    DataMoveFailed {
        from: String,
        to: String,
        message: String,
    },
    /// The move succeeded but some old files could not be deleted.
    OldFilesLeft {
        storage: StorageKind,
        path: String,
        count: u32,
    },
}

/// Locations in effect for this run.
#[derive(Debug, Clone)]
pub struct Resolution {
    pub locations: Locations,
    /// What `locations.json` says (kept even when a custom folder is missing, so it is used
    /// again once the drive is back).
    pub pointer: Pointer,
    pub warnings: Vec<StorageWarning>,
}

/// Resolves the folders from the pointer at `pointer_path`. Never fails: every problem falls
/// back to the default and is reported in [`Resolution::warnings`].
pub fn resolve(defaults: &Locations, pointer_path: &Path) -> Resolution {
    let mut warnings = Vec::new();
    let pointer = match read_pointer(pointer_path) {
        Ok(p) => p.unwrap_or_default(),
        Err(message) => {
            warnings.push(StorageWarning::PointerUnreadable {
                path: pointer_path.display().to_string(),
                message,
            });
            Pointer::default()
        }
    };
    let mut locations = defaults.clone();
    for kind in StorageKind::ALL {
        let Some(custom) = pointer.get(kind) else {
            continue;
        };
        if usable_custom(kind, custom) {
            locations.set(kind, custom.to_owned());
        } else {
            warnings.push(StorageWarning::MissingFolder {
                storage: kind,
                path: custom.display().to_string(),
                fallback: defaults.get(kind).display().to_string(),
            });
        }
    }
    Resolution {
        locations,
        pointer,
        warnings,
    }
}

/// A missing log/cache folder is recreated (only if its parent is there — a missing drive
/// stays missing); a missing data folder is never recreated: an empty database there would
/// silently hide the user's accounts.
fn usable_custom(kind: StorageKind, path: &Path) -> bool {
    if !path.is_absolute() {
        return false;
    }
    if path.is_dir() {
        return true;
    }
    if kind == StorageKind::Data || path.exists() {
        return false;
    }
    path.parent().is_some_and(Path::is_dir) && std::fs::create_dir(path).is_ok()
}

/// Result of a scheduled data move done at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataMoved {
    pub from: PathBuf,
    pub to: PathBuf,
}

/// Performs the data-folder move scheduled in the pointer, if any: copy, verify, `after_copy`
/// (e.g. restore SSH key permissions; an error there is only logged by the caller), switch the
/// pointer, then delete the old files. Any failure before the switch keeps the old folder in
/// use, untouched. The pending entry is always cleared so a broken move is not retried on
/// every start. Call before the database is opened.
pub fn apply_pending_data(
    res: &mut Resolution,
    defaults: &Locations,
    pointer_path: &Path,
    progress: &mut dyn FnMut(&MoveProgress),
    after_copy: &mut dyn FnMut(&Path),
) -> Option<DataMoved> {
    let pending = res.pointer.pending_data.take()?;
    let from = res.locations.data.clone();
    let fail = |res: &mut Resolution, message: String| {
        res.warnings.push(StorageWarning::DataMoveFailed {
            from: pending.from.display().to_string(),
            to: pending.to.display().to_string(),
            message,
        });
    };
    if !same_path(&pending.from, &from) {
        fail(
            res,
            format!(
                "the data folder in use is {} now, not {}",
                from.display(),
                pending.from.display()
            ),
        );
        let _ = write_pointer(pointer_path, &res.pointer);
        return None;
    }
    let mut exclude = res.locations.others(StorageKind::Data);
    exclude.push(pointer_path.to_owned());
    let tree = match copy_tree(&from, &pending.to, &exclude, progress) {
        Ok(tree) => tree,
        Err(e) => {
            fail(res, e.to_string());
            let _ = write_pointer(pointer_path, &res.pointer);
            return None;
        }
    };
    after_copy(&pending.to);
    let mut next = res.pointer.clone();
    next.set(StorageKind::Data, &pending.to, defaults);
    if let Err(e) = write_pointer(pointer_path, &next) {
        tree.rollback();
        fail(
            res,
            format!("could not save {}: {e}", pointer_path.display()),
        );
        let _ = write_pointer(pointer_path, &res.pointer);
        return None;
    }
    res.pointer = next;
    progress(&MoveProgress {
        phase: MovePhase::Cleaning,
        done_bytes: tree.bytes,
        total_bytes: tree.bytes,
        done_files: tree.files.len() as u64,
        total_files: tree.files.len() as u64,
    });
    let left = tree.remove_source();
    if !left.is_empty() {
        res.warnings.push(StorageWarning::OldFilesLeft {
            storage: StorageKind::Data,
            path: from.display().to_string(),
            count: u32::try_from(left.len()).unwrap_or(u32::MAX),
        });
    }
    res.locations.data = pending.to.clone();
    res.warnings.push(StorageWarning::DataMoved {
        from: from.display().to_string(),
        to: pending.to.display().to_string(),
    });
    Some(DataMoved {
        from,
        to: pending.to,
    })
}

/// Rewrites `path` from below `from` to the same place below `to` (absolute paths stored in
/// the database, e.g. SSH key files); `None` when `path` is not inside `from`.
pub fn rebase_path(path: &Path, from: &Path, to: &Path) -> Option<PathBuf> {
    path.strip_prefix(from).ok().map(|rest| to.join(rest))
}

#[cfg(test)]
mod tests;
