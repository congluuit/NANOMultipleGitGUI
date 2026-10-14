//! Git CLI runner and history reader for NANOMultipleGitGUI.
//!
//! Write operations always go through the system git CLI (hooks, LFS, signing and credential
//! helpers keep working); every command carries an [`AccountContext`] so it runs as the account
//! bound to the repository.

pub mod branch;
pub mod clone;
pub mod commit;
pub mod commit_write;
pub mod context;
pub mod diff;
pub mod graph;
pub mod guard;
pub mod history;
pub mod index;
mod job;
pub mod merge;
pub mod ops;
pub mod progress;
pub mod refs;
pub mod remote;
pub mod repo;
pub mod runner;
pub mod stash;
pub mod status;
pub mod version;
pub mod watch;

pub use context::{AccountContext, HttpsAuth, Identity, SshAskpass, SshAuth};
pub use runner::{GitError, GitRunner};
