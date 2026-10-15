//! Git CLI runner and history reader for NANOMultipleGitGUI.
//!
//! Write operations always go through the system git CLI (hooks, LFS, signing and credential
//! helpers keep working); every command carries an [`AccountContext`] so it runs as the account
//! bound to the repository.

pub mod blame;
pub mod blob;
pub mod branch;
pub mod clone;
pub mod commit;
pub mod commit_write;
pub mod conflict;
pub mod context;
pub mod diff;
pub mod file_history;
pub mod graph;
pub mod guard;
pub mod history;
pub mod index;
mod job;
pub mod merge;
pub mod ops;
pub mod progress;
pub mod rebase;
pub mod refs;
pub mod remote;
pub mod repo;
pub mod runner;
pub mod signature;
pub mod stash;
pub mod status;
pub mod submodule;
pub mod undo;
pub mod version;
pub mod watch;
pub mod worktrees;

pub use context::{
    AccountContext, HttpsAuth, Identity, Signing, SigningFormat, SshAskpass, SshAuth,
};
pub use runner::{GitError, GitRunner};
