//! Large File Guard, Git LFS tracking/migration and transfer progress.
//!
//! Everything runs the system `git` / `git lfs` through [`nanomgg_git_engine::GitRunner`]:
//! - [`detect`]: is git-lfs installed, does the repo use LFS (`.gitattributes`), are the LFS
//!   hooks and filters set up (PLAN §6.1);
//! - [`setup`]: `git lfs install --local`, `track` / `untrack`, re-staging files as pointers;
//! - [`guard`]: the Large File Guard run before a commit (PLAN §6.2);
//! - [`transfer`]: upload/download with estimate, progress, cancel/resume and translated
//!   errors (PLAN §6.4, §6.5).

pub mod attributes;
pub mod detect;
pub mod guard;
pub mod setup;
pub mod transfer;

use nanomgg_git_engine::GitError;

#[derive(Debug, thiserror::Error)]
pub enum LfsError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// `git lfs version` failed: git-lfs is not installed or not on PATH.
    #[error("git-lfs is not installed")]
    NotInstalled,
    /// Empty, option-like (`-x`), multi-line or otherwise unusable LFS pattern.
    #[error("invalid LFS pattern: {0:?}")]
    InvalidPattern(String),
    /// Absolute, `..`-escaping or otherwise unusable repository path.
    #[error("invalid path: {0:?}")]
    InvalidPath(String),
}

pub use attributes::LfsPattern;
pub use detect::{LfsHook, LfsStatus};
pub use guard::{GuardFinding, GuardLevel, GuardProvider, GuardReport, GuardThresholds};
