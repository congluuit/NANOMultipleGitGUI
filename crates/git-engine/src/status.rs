//! Working-tree status (`git status --porcelain=v2 -z`): staged, unstaged, untracked and
//! conflicted files plus branch information.

use std::path::Path;

use serde::Serialize;

use crate::{AccountContext, GitError, GitRunner};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
    Conflicted,
}

impl ChangeKind {
    fn from_code(c: char) -> Option<Self> {
        Some(match c {
            'A' => Self::Added,
            'M' => Self::Modified,
            'D' => Self::Deleted,
            'R' => Self::Renamed,
            'C' => Self::Copied,
            'T' => Self::TypeChanged,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct StatusEntry {
    pub path: String,
    /// Previous path of a staged rename or copy.
    pub old_path: Option<String>,
    pub kind: ChangeKind,
    /// Two-letter porcelain code of a conflict (`UU`, `AA`, `DU`...).
    pub conflict: Option<String>,
    pub submodule: bool,
    /// Size of the file in the working tree, if it exists there.
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct WorkingTreeStatus {
    /// Current branch; `None` when HEAD is detached.
    pub branch: Option<String>,
    pub head_oid: Option<String>,
    /// No commit yet on the current branch.
    pub unborn: bool,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub staged: Vec<StatusEntry>,
    /// Modified/deleted tracked files and untracked files, in path order.
    pub unstaged: Vec<StatusEntry>,
    pub conflicted: Vec<StatusEntry>,
}

impl WorkingTreeStatus {
    pub fn is_clean(&self) -> bool {
        self.staged.is_empty() && self.unstaged.is_empty() && self.conflicted.is_empty()
    }
}

fn entry(path: &str, old_path: Option<&str>, kind: ChangeKind, submodule: bool) -> StatusEntry {
    StatusEntry {
        path: path.to_owned(),
        old_path: old_path.map(str::to_owned),
        kind,
        conflict: None,
        submodule,
        size: None,
    }
}

/// Parses `git status --porcelain=v2 -z --branch` output. Sizes are left unset.
pub fn parse_status(raw: &str) -> WorkingTreeStatus {
    let mut st = WorkingTreeStatus::default();
    let mut records = raw.split('\0').filter(|r| !r.is_empty());
    while let Some(rec) = records.next() {
        let Some((tag, rest)) = rec.split_once(' ') else {
            continue;
        };
        match tag {
            "#" => {
                let (key, value) = rest.split_once(' ').unwrap_or((rest, ""));
                match key {
                    "branch.oid" => {
                        st.unborn = value == "(initial)";
                        st.head_oid = (!st.unborn).then(|| value.to_owned());
                    }
                    "branch.head" => st.branch = (value != "(detached)").then(|| value.to_owned()),
                    "branch.upstream" => st.upstream = Some(value.to_owned()),
                    "branch.ab" => {
                        for part in value.split(' ') {
                            if let Some(n) = part.strip_prefix('+') {
                                st.ahead = n.parse().unwrap_or(0);
                            } else if let Some(n) = part.strip_prefix('-') {
                                st.behind = n.parse().unwrap_or(0);
                            }
                        }
                    }
                    _ => {}
                }
            }
            // 1 XY sub mH mI mW hH hI path
            // 2 XY sub mH mI mW hH hI Xscore path \0 origPath
            "1" | "2" => {
                let fields = if tag == "1" { 8 } else { 9 };
                let f: Vec<&str> = rest.splitn(fields, ' ').collect();
                if f.len() < fields {
                    continue;
                }
                let path = f[fields - 1];
                let old = if tag == "2" { records.next() } else { None };
                let mut xy = f[0].chars();
                let (x, y) = (xy.next().unwrap_or('.'), xy.next().unwrap_or('.'));
                let submodule = f[1].starts_with('S');
                if let Some(kind) = ChangeKind::from_code(x) {
                    st.staged.push(entry(path, old, kind, submodule));
                }
                if let Some(kind) = ChangeKind::from_code(y) {
                    // A rename is recorded in the index; the worktree side is a plain change
                    // of the new path.
                    let kind = match kind {
                        ChangeKind::Renamed | ChangeKind::Copied => ChangeKind::Modified,
                        k => k,
                    };
                    st.unstaged.push(entry(path, None, kind, submodule));
                }
            }
            // u XY sub m1 m2 m3 mW h1 h2 h3 path
            "u" => {
                let f: Vec<&str> = rest.splitn(10, ' ').collect();
                if f.len() < 10 {
                    continue;
                }
                let mut e = entry(f[9], None, ChangeKind::Conflicted, f[1].starts_with('S'));
                e.conflict = Some(f[0].to_owned());
                st.conflicted.push(e);
            }
            "?" => st
                .unstaged
                .push(entry(rest, None, ChangeKind::Untracked, false)),
            _ => {}
        }
    }
    st.unstaged.sort_by(|a, b| a.path.cmp(&b.path));
    st
}

/// Reads the status of the repository at `root`, including working-tree file sizes.
pub fn read_status(git: &GitRunner, root: &Path) -> Result<WorkingTreeStatus, GitError> {
    let out = git.run(
        root,
        &AccountContext::default(),
        [
            "-c",
            "core.quotePath=false",
            "-c",
            "status.relativePaths=false",
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--untracked-files=all",
            "--find-renames",
        ],
    )?;
    let mut st = parse_status(&String::from_utf8_lossy(&out.stdout));
    for e in st
        .staged
        .iter_mut()
        .chain(st.unstaged.iter_mut())
        .chain(st.conflicted.iter_mut())
    {
        e.size = std::fs::symlink_metadata(root.join(&e.path))
            .ok()
            .filter(|m| m.is_file())
            .map(|m| m.len());
    }
    Ok(st)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Identity;

    #[test]
    fn parses_headers_and_entries() {
        let raw = concat!(
            "# branch.oid 1234567890abcdef1234567890abcdef12345678\0",
            "# branch.head feature/x\0",
            "# branch.upstream origin/feature/x\0",
            "# branch.ab +2 -1\0",
            "1 M. N... 100644 100644 100644 aaa bbb src/a b.rs\0",
            "1 .M N... 100644 100644 100644 aaa aaa src/c.rs\0",
            "1 MD N... 100644 100644 000000 aaa bbb both.txt\0",
            "1 A. N... 000000 100644 100644 000 bbb tên mới.txt\0",
            "2 R. N... 100644 100644 100644 aaa aaa R100 new/name.rs\0old/name.rs\0",
            "u UU N... 100644 100644 100644 100644 a b c conflict.txt\0",
            "1 .M S.M. 160000 160000 160000 aaa aaa vendor/sub\0",
            "? untracked dir/file.txt\0",
        );
        let st = parse_status(raw);
        assert_eq!(st.branch.as_deref(), Some("feature/x"));
        assert_eq!(st.upstream.as_deref(), Some("origin/feature/x"));
        assert_eq!((st.ahead, st.behind), (2, 1));
        assert!(!st.unborn);
        let staged: Vec<_> = st
            .staged
            .iter()
            .map(|e| (e.path.as_str(), e.kind))
            .collect();
        assert_eq!(
            staged,
            vec![
                ("src/a b.rs", ChangeKind::Modified),
                ("both.txt", ChangeKind::Modified),
                ("tên mới.txt", ChangeKind::Added),
                ("new/name.rs", ChangeKind::Renamed),
            ]
        );
        assert_eq!(st.staged[3].old_path.as_deref(), Some("old/name.rs"));
        let unstaged: Vec<_> = st
            .unstaged
            .iter()
            .map(|e| (e.path.as_str(), e.kind))
            .collect();
        assert_eq!(
            unstaged,
            vec![
                ("both.txt", ChangeKind::Deleted),
                ("src/c.rs", ChangeKind::Modified),
                ("untracked dir/file.txt", ChangeKind::Untracked),
                ("vendor/sub", ChangeKind::Modified),
            ]
        );
        assert!(st.unstaged[3].submodule);
        assert_eq!(st.conflicted[0].path, "conflict.txt");
        assert_eq!(st.conflicted[0].conflict.as_deref(), Some("UU"));
    }

    #[test]
    fn parses_unborn_and_detached() {
        let st = parse_status("# branch.oid (initial)\0# branch.head main\0");
        assert!(st.unborn && st.head_oid.is_none());
        assert_eq!(st.branch.as_deref(), Some("main"));
        let st = parse_status("# branch.oid abcd\0# branch.head (detached)\0");
        assert!(st.branch.is_none());
        assert_eq!(st.head_oid.as_deref(), Some("abcd"));
        assert!(st.is_clean());
    }

    #[test]
    fn reads_real_status_with_sizes() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        let git = GitRunner::default();
        let ctx = AccountContext {
            identity: Some(Identity {
                name: "T".into(),
                email: "t@x.dev".into(),
            }),
            ..Default::default()
        };
        let run = |args: &[&str]| git.run_text(p, &ctx, args).unwrap();
        run(&["init", "-q", "-b", "main"]);
        let st = read_status(&git, p).unwrap();
        assert!(st.unborn && st.is_clean());

        std::fs::write(p.join("a.txt"), "hello\n").unwrap();
        std::fs::create_dir(p.join("sub")).unwrap();
        std::fs::write(p.join("sub/b.txt"), "12345").unwrap();
        run(&["add", "a.txt"]);
        let st = read_status(&git, p).unwrap();
        assert_eq!(st.staged.len(), 1);
        assert_eq!(st.staged[0].kind, ChangeKind::Added);
        assert_eq!(st.unstaged.len(), 1);
        assert_eq!(st.unstaged[0].path, "sub/b.txt");
        assert_eq!(st.unstaged[0].kind, ChangeKind::Untracked);
        assert_eq!(st.unstaged[0].size, Some(5));

        run(&["commit", "-q", "-m", "init"]);
        std::fs::remove_file(p.join("a.txt")).unwrap();
        let st = read_status(&git, p).unwrap();
        assert!(!st.unborn);
        assert_eq!(st.branch.as_deref(), Some("main"));
        let a = st.unstaged.iter().find(|e| e.path == "a.txt").unwrap();
        assert_eq!((a.kind, a.size), (ChangeKind::Deleted, None));
    }
}
