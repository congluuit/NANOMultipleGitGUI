use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use nanomgg_git_engine::GitRunner;
use nanomgg_git_engine::history::RepoGraph;
use nanomgg_git_engine::repo::RepoInfo;
use nanomgg_git_engine::watch::RepoWatcher;
use nanomgg_storage::Storage;

use crate::accounts::AccountsRuntime;

/// Repositories open as tabs, in tab order.
#[derive(Default)]
pub struct Workspace {
    pub tabs: Vec<i64>,
    pub active: Option<i64>,
    /// Resolved location of every open repo whose folder is currently available.
    pub repos: HashMap<i64, RepoInfo>,
    pub watchers: HashMap<i64, RepoWatcher>,
    /// Whether `tabs`/`active` were already restored from the database this session.
    pub restored: bool,
}

pub struct AppState {
    /// Shared with the credential server thread (account lookups); derefs to `Storage`.
    pub storage: Arc<Storage>,
    pub git: GitRunner,
    pub workspace: Mutex<Workspace>,
    pub graphs: Mutex<HashMap<i64, RepoGraph>>,
    /// Multi-account services: credential server, sidecar path, keychain, known_hosts.
    /// Use [`crate::accounts::context_for`] to get the context of a git command.
    pub accounts: AccountsRuntime,
}

impl AppState {
    /// State without credential injection (tests): identities still resolve, tokens do not.
    pub fn new(storage: Storage) -> Self {
        Self::with_accounts(Arc::new(storage), AccountsRuntime::disabled())
    }

    pub fn with_accounts(storage: Arc<Storage>, accounts: AccountsRuntime) -> Self {
        Self {
            storage,
            git: GitRunner::default(),
            workspace: Mutex::default(),
            graphs: Mutex::default(),
            accounts,
        }
    }

    pub fn workspace(&self) -> MutexGuard<'_, Workspace> {
        self.workspace.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn graphs(&self) -> MutexGuard<'_, HashMap<i64, RepoGraph>> {
        self.graphs.lock().unwrap_or_else(|e| e.into_inner())
    }
}
