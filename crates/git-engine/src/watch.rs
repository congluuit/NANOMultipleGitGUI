//! File-system watcher that tells the UI when a repository changed on disk
//! (commits, checkouts or fetches from another tool, edits in the working tree).

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use notify_debouncer_mini::notify::{
    self, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{AccessKind, AccessMode},
};

use crate::{AccountContext, GitRunner};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// Not interesting (object database, logs, lock files, hooks...).
    Ignore,
    /// HEAD, branches, tags, remotes or an in-progress merge/rebase changed.
    Refs,
    /// The staging area changed.
    Index,
    /// A file in the working tree changed.
    Worktree,
}

/// Classifies one changed path for a repository with the given root and git directory.
pub fn classify(path: &Path, root: &Path, git_dir: &Path) -> ChangeKind {
    if let Ok(rel) = path.strip_prefix(git_dir) {
        let parts: Vec<String> = rel
            .components()
            .filter_map(|c| match c {
                Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        let Some(first) = parts.first() else {
            return ChangeKind::Ignore;
        };
        if parts.last().is_some_and(|p| p.ends_with(".lock")) {
            return ChangeKind::Ignore;
        }
        return match first.as_str() {
            "HEAD" | "ORIG_HEAD" | "FETCH_HEAD" | "MERGE_HEAD" | "CHERRY_PICK_HEAD"
            | "REVERT_HEAD" | "packed-refs" | "refs" | "config" | "rebase-merge"
            | "rebase-apply" => ChangeKind::Refs,
            "index" => ChangeKind::Index,
            _ => ChangeKind::Ignore,
        };
    }
    match path.strip_prefix(root) {
        // A `.git` path that is not this repo's git dir (e.g. a nested repo) is ignored.
        Ok(rel) if rel.components().any(|c| c.as_os_str() == ".git") => ChangeKind::Ignore,
        Ok(_) => ChangeKind::Worktree,
        Err(_) => ChangeKind::Ignore,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RepoChange {
    pub refs: bool,
    pub index: bool,
    pub worktree: bool,
}

impl RepoChange {
    pub fn add(&mut self, kind: ChangeKind) {
        match kind {
            ChangeKind::Refs => self.refs = true,
            ChangeKind::Index => self.index = true,
            ChangeKind::Worktree => self.worktree = true,
            ChangeKind::Ignore => {}
        }
    }

    pub fn is_empty(&self) -> bool {
        !(self.refs || self.index || self.worktree)
    }
}

/// Working-tree paths (relative to `root`) that are not ignored by `.gitignore`. Large batches
/// are returned unfiltered rather than spending time on them.
fn not_ignored(git: &GitRunner, root: &Path, paths: Vec<PathBuf>) -> Vec<PathBuf> {
    const MAX_CHECK: usize = 5000;
    if paths.is_empty() || paths.len() > MAX_CHECK {
        return paths;
    }
    let mut input = Vec::new();
    for p in &paths {
        input.extend_from_slice(p.to_string_lossy().replace('\\', "/").as_bytes());
        input.push(0);
    }
    let out = git.run_with_input(
        root,
        &AccountContext::default(),
        ["check-ignore", "-z", "--stdin"],
        &input,
    );
    let ignored: HashSet<String> = match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout)
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect(),
        // Exit code 1 means "nothing is ignored"; other failures keep every path.
        Err(_) => return paths,
    };
    paths
        .into_iter()
        .filter(|p| !ignored.contains(&p.to_string_lossy().replace('\\', "/")))
        .collect()
}

/// Keeps watching while alive; dropping it stops the watcher.
pub struct RepoWatcher {
    _watcher: RecommendedWatcher,
}

pub const DEBOUNCE: Duration = Duration::from_millis(300);

impl RepoWatcher {
    /// Watches the working tree (and the git dir when it lives elsewhere) and calls
    /// `on_change` at most once per debounce window with the aggregated change. Edits that only
    /// touch ignored files (build output, `node_modules`...) are not reported.
    pub fn start(
        git: &GitRunner,
        root: &Path,
        git_dir: &Path,
        on_change: impl Fn(RepoChange) + Send + 'static,
    ) -> notify_debouncer_mini::notify::Result<Self> {
        let (root_c, git_dir_c): (PathBuf, PathBuf) = (root.to_owned(), git_dir.to_owned());
        let git = git.clone();
        let (tx, rx) = mpsc::channel::<PathBuf>();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            let Ok(event) = res else { return };
            // inotify (Linux) also reports files being opened or read: every git command the
            // app runs — including `check-ignore` below — would otherwise trigger a refresh.
            if let EventKind::Access(kind) = event.kind
                && kind != AccessKind::Close(AccessMode::Write)
            {
                return;
            }
            for path in event.paths {
                let _ = tx.send(path);
            }
        })?;
        watcher.watch(root, RecursiveMode::Recursive)?;
        if !git_dir.starts_with(root) {
            watcher.watch(git_dir, RecursiveMode::Recursive)?;
        }
        // Ends when the watcher (and with it the sender) is dropped.
        std::thread::Builder::new()
            .name("repo-watcher".into())
            .spawn(move || {
                while let Ok(first) = rx.recv() {
                    let mut paths = HashSet::from([first]);
                    let deadline = Instant::now() + DEBOUNCE;
                    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
                        match rx.recv_timeout(left) {
                            Ok(path) => {
                                paths.insert(path);
                            }
                            Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                    }
                    let mut change = RepoChange::default();
                    let mut worktree = Vec::new();
                    for path in paths {
                        match classify(&path, &root_c, &git_dir_c) {
                            ChangeKind::Worktree => match path.strip_prefix(&root_c) {
                                Ok(rel) if !rel.as_os_str().is_empty() => {
                                    worktree.push(rel.to_owned());
                                }
                                _ => {}
                            },
                            kind => change.add(kind),
                        }
                    }
                    if !not_ignored(&git, &root_c, worktree).is_empty() {
                        change.worktree = true;
                    }
                    if !change.is_empty() {
                        on_change(change);
                    }
                }
            })
            .map_err(|e| notify::Error::generic(&e.to_string()))?;
        Ok(Self { _watcher: watcher })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s.replace('/', std::path::MAIN_SEPARATOR_STR))
    }

    #[test]
    fn classification() {
        let root = p("/w/repo");
        let gd = p("/w/repo/.git");
        let c = |s: &str| classify(&p(s), &root, &gd);
        assert_eq!(c("/w/repo/.git/HEAD"), ChangeKind::Refs);
        assert_eq!(c("/w/repo/.git/refs/heads/main"), ChangeKind::Refs);
        assert_eq!(c("/w/repo/.git/refs/remotes/origin/main"), ChangeKind::Refs);
        assert_eq!(c("/w/repo/.git/packed-refs"), ChangeKind::Refs);
        assert_eq!(c("/w/repo/.git/index"), ChangeKind::Index);
        assert_eq!(c("/w/repo/.git/index.lock"), ChangeKind::Ignore);
        assert_eq!(c("/w/repo/.git/refs/heads/main.lock"), ChangeKind::Ignore);
        assert_eq!(c("/w/repo/.git/objects/ab/cdef"), ChangeKind::Ignore);
        assert_eq!(c("/w/repo/.git/logs/HEAD"), ChangeKind::Ignore);
        assert_eq!(c("/w/repo/src/main.rs"), ChangeKind::Worktree);
        assert_eq!(c("/w/repo/vendor/lib/.git/HEAD"), ChangeKind::Ignore);
        assert_eq!(c("/w/other/file"), ChangeKind::Ignore);
    }

    #[test]
    fn separate_git_dir_for_worktrees() {
        let root = p("/w/wt");
        let gd = p("/w/repo/.git/worktrees/wt");
        assert_eq!(
            classify(&p("/w/repo/.git/worktrees/wt/HEAD"), &root, &gd),
            ChangeKind::Refs
        );
        assert_eq!(
            classify(&p("/w/wt/a.txt"), &root, &gd),
            ChangeKind::Worktree
        );
    }

    #[test]
    fn watcher_reports_ref_and_worktree_changes() {
        let dir = tempfile::tempdir().unwrap();
        let git = crate::GitRunner::default();
        let info = crate::repo::init(&git, &dir.path().join("r"), "main").unwrap();
        let (tx, rx) = mpsc::channel();
        std::fs::write(info.root.join(".gitignore"), "build/\n").unwrap();
        std::fs::create_dir(info.root.join("build")).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let _w = RepoWatcher::start(&git, &info.root, &info.git_dir, move |c| {
            let _ = tx.send(c);
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(200));

        // Only ignored files change: nothing is reported.
        std::fs::write(info.root.join("build/out.o"), "x").unwrap();
        let change = wait_for_ms(&rx, |c| c.worktree, 1500);
        assert!(!change.worktree, "{change:?}");

        std::fs::write(info.root.join("a.txt"), "hello").unwrap();
        let change = wait_for(&rx, |c| c.worktree);
        assert!(change.worktree, "{change:?}");

        std::fs::write(info.git_dir.join("HEAD"), "ref: refs/heads/other\n").unwrap();
        let change = wait_for(&rx, |c| c.refs);
        assert!(change.refs, "{change:?}");
    }

    fn wait_for(rx: &mpsc::Receiver<RepoChange>, pred: impl Fn(&RepoChange) -> bool) -> RepoChange {
        wait_for_ms(rx, pred, 10_000)
    }

    fn wait_for_ms(
        rx: &mpsc::Receiver<RepoChange>,
        pred: impl Fn(&RepoChange) -> bool,
        ms: u64,
    ) -> RepoChange {
        let deadline = std::time::Instant::now() + Duration::from_millis(ms);
        let mut last = RepoChange::default();
        while std::time::Instant::now() < deadline {
            if let Ok(c) = rx.recv_timeout(Duration::from_millis(500)) {
                if pred(&c) {
                    return c;
                }
                last = c;
            }
        }
        last
    }
}
