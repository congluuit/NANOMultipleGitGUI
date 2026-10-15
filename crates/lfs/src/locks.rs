//! Git LFS file locking (PLAN §6.6): list locks (`git lfs locks --verify --json`: mine vs
//! others), lock, unlock and force-unlock, and marking patterns `lockable`.
//!
//! Every call takes the repository's [`AccountContext`]: the lock API is called as the account
//! bound to the remote (credentials come from the injected helper, like transfers).
//! Failures are translated into a [`LockFailure`] (already locked, not the owner, no
//! permission, server without lock support…).

use std::path::Path;

use nanomgg_git_engine::{AccountContext, GitError, GitRunner};
use serde::{Deserialize, Serialize};

use crate::LfsError;
use crate::setup::{normalize_path, validate_pattern};

/// One lock on the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LfsLock {
    pub id: String,
    /// Repository-relative path.
    pub path: String,
    /// Name of the user holding the lock as the server reports it.
    pub owner: Option<String>,
    /// RFC 3339 timestamp.
    pub locked_at: Option<String>,
    /// Held by the account the command ran as.
    pub ours: bool,
}

/// Locks of a repository.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LockList {
    pub locks: Vec<LfsLock>,
    /// The server confirmed which locks are ours (`--verify`). When `false`, `ours` was guessed
    /// by comparing the owner with the account's user name.
    pub verified: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum LockFailureKind {
    /// Somebody (maybe you, from another clone) already holds the lock.
    AlreadyLocked,
    /// The lock belongs to another user: only a force unlock (with permission) removes it.
    NotOwner,
    /// The account may not lock / force-unlock (needs write / maintainer access).
    NoPermission,
    /// Sign-in required or rejected.
    AuthRequired,
    /// The server (or a local / file remote) has no LFS lock API.
    Unsupported,
    /// No such lock / file.
    NotFound,
    Network,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LockFailure {
    pub kind: LockFailureKind,
    /// git-lfs output (redacted by the app).
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("{}", .0.message)]
    Failed(LockFailure),
    #[error(transparent)]
    Lfs(#[from] LfsError),
}

impl From<GitError> for LockError {
    fn from(e: GitError) -> Self {
        Self::Lfs(LfsError::Git(e))
    }
}

/// Classifies git-lfs lock / unlock / locks output.
pub fn lock_failure_kind(message: &str) -> LockFailureKind {
    use LockFailureKind::*;
    let s = message.to_ascii_lowercase();
    let any = |ps: &[&str]| ps.iter().any(|p| s.contains(p));
    if any(&[
        "missing protocol",
        "standalone transfer agent",
        "does not support the git lfs locking api",
        "locking api not supported",
        "not implemented",
        "501",
    ]) {
        Unsupported
    } else if any(&["lock exists", "already locked", "already created lock"]) {
        AlreadyLocked
    } else if any(&[
        "owned by another user",
        "is owned by",
        "not the owner",
        "lock belongs to",
        "use --force",
        "unable to unlock: lock",
    ]) {
        NotOwner
    } else if any(&[
        "403",
        "forbidden",
        "must have push access",
        "must have admin",
        "permission",
        "not allowed",
    ]) {
        NoPermission
    } else if any(&[
        "401",
        "authorization error",
        "authentication",
        "credentials",
        "terminal prompts disabled",
        "could not read username",
    ]) {
        AuthRequired
    } else if any(&[
        "404",
        "not found",
        "no matching locks",
        "unable to find lock",
    ]) {
        NotFound
    } else if any(&[
        "could not resolve host",
        "connection refused",
        "timeout",
        "timed out",
        "connection reset",
        "no such host",
        "eof",
    ]) {
        Network
    } else {
        Other
    }
}

fn failure(message: &str) -> LockError {
    LockError::Failed(LockFailure {
        kind: lock_failure_kind(message),
        message: message.trim().to_owned(),
    })
}

#[derive(Debug, Deserialize)]
struct RawOwner {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawLock {
    id: String,
    path: String,
    owner: Option<RawOwner>,
    locked_at: Option<String>,
}

impl RawLock {
    fn into_lock(self, ours: bool) -> LfsLock {
        LfsLock {
            id: self.id,
            path: self.path.replace('\\', "/"),
            owner: self.owner.and_then(|o| o.name).filter(|n| !n.is_empty()),
            locked_at: self.locked_at.filter(|t| !t.is_empty()),
            ours,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Verified {
    #[serde(default)]
    ours: Option<Vec<RawLock>>,
    #[serde(default)]
    theirs: Option<Vec<RawLock>>,
}

/// Parses `git lfs locks --verify --json` (`{"ours": [...], "theirs": [...]}`).
pub fn parse_verified_locks(json: &str) -> Result<Vec<LfsLock>, serde_json::Error> {
    let v: Verified = serde_json::from_str(json.trim())?;
    let mut out: Vec<LfsLock> = v
        .ours
        .unwrap_or_default()
        .into_iter()
        .map(|l| l.into_lock(true))
        .collect();
    out.extend(
        v.theirs
            .unwrap_or_default()
            .into_iter()
            .map(|l| l.into_lock(false)),
    );
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Parses `git lfs locks --json` (a plain array); `ours` is guessed from `username`
/// (case-insensitive).
pub fn parse_locks(json: &str, username: Option<&str>) -> Result<Vec<LfsLock>, serde_json::Error> {
    let raw: Vec<RawLock> = serde_json::from_str(json.trim())?;
    let mut out: Vec<LfsLock> = raw
        .into_iter()
        .map(|l| {
            let ours = match (username, l.owner.as_ref().and_then(|o| o.name.as_deref())) {
                (Some(u), Some(owner)) => u.eq_ignore_ascii_case(owner),
                _ => false,
            };
            l.into_lock(ours)
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Parses the single lock printed by `git lfs lock --json`.
pub fn parse_lock(json: &str) -> Result<LfsLock, serde_json::Error> {
    let raw: RawLock = serde_json::from_str(json.trim())?;
    Ok(raw.into_lock(true))
}

fn remote_args(remote: Option<&str>) -> Result<Vec<String>, LockError> {
    match remote {
        Some(r) => {
            if r.is_empty() || r.starts_with('-') || r.chars().any(char::is_control) {
                return Err(LockError::Lfs(LfsError::InvalidPath(r.to_owned())));
            }
            Ok(vec![format!("--remote={r}")])
        }
        None => Ok(Vec::new()),
    }
}

fn run_lfs(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    args: &[String],
) -> Result<String, LockError> {
    let out = git
        .command(repo, ctx, args)
        .env("LC_ALL", "C")
        .output()
        .map_err(GitError::from)?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if out.status.success() {
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        Err(failure(&format!("{}\n{}", stderr.trim(), stdout.trim())))
    }
}

/// Locks of the repository's LFS server. Tries `--verify` (the server says which are ours)
/// and falls back to the plain list, guessing ownership from `username`.
pub fn list(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    remote: Option<&str>,
    username: Option<&str>,
) -> Result<LockList, LockError> {
    let mut base = vec!["lfs".to_owned(), "locks".to_owned()];
    base.extend(remote_args(remote)?);
    let mut verify = base.clone();
    verify.extend(["--verify".into(), "--json".into()]);
    match run_lfs(git, repo, ctx, &verify) {
        Ok(out) => {
            if let Ok(locks) = parse_verified_locks(&out) {
                return Ok(LockList {
                    locks,
                    verified: true,
                });
            }
        }
        Err(LockError::Failed(f))
            if matches!(
                f.kind,
                LockFailureKind::Unsupported
                    | LockFailureKind::AuthRequired
                    | LockFailureKind::Network
            ) =>
        {
            return Err(LockError::Failed(f));
        }
        Err(_) => {}
    }
    let mut plain = base;
    plain.push("--json".into());
    let out = run_lfs(git, repo, ctx, &plain)?;
    let locks = parse_locks(&out, username)
        .map_err(|e| failure(&format!("unexpected git lfs locks output: {e}")))?;
    Ok(LockList {
        locks,
        verified: false,
    })
}

/// `git lfs lock --json -- <path>`.
pub fn lock(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    remote: Option<&str>,
    path: &str,
) -> Result<LfsLock, LockError> {
    let path = normalize_path(path)?;
    let mut args = vec!["lfs".to_owned(), "lock".to_owned()];
    args.extend(remote_args(remote)?);
    args.extend(["--json".into(), "--".into(), path.clone()]);
    let out = run_lfs(git, repo, ctx, &args)?;
    Ok(parse_lock(&out).unwrap_or(LfsLock {
        id: String::new(),
        path,
        owner: None,
        locked_at: None,
        ours: true,
    }))
}

/// `git lfs unlock [--force] -- <path>` (or `--id=<id>` when the file no longer exists).
/// Unlocking another user's lock without `force` fails with [`LockFailureKind::NotOwner`];
/// with `force` the server checks the permission ([`LockFailureKind::NoPermission`]).
pub fn unlock(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    remote: Option<&str>,
    path: Option<&str>,
    id: Option<&str>,
    force: bool,
) -> Result<(), LockError> {
    let mut args = vec!["lfs".to_owned(), "unlock".to_owned()];
    args.extend(remote_args(remote)?);
    if force {
        args.push("--force".into());
    }
    args.push("--json".into());
    match (path, id) {
        (Some(p), _) => {
            args.extend(["--".into(), normalize_path(p)?]);
        }
        (None, Some(id)) => {
            if id.is_empty()
                || !id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err(LockError::Lfs(LfsError::InvalidPath(id.to_owned())));
            }
            args.push(format!("--id={id}"));
        }
        (None, None) => {
            return Err(LockError::Lfs(LfsError::InvalidPath(String::new())));
        }
    }
    run_lfs(git, repo, ctx, &args)?;
    Ok(())
}

/// Marks (`git lfs track --lockable`) or unmarks (`--not-lockable`) an LFS pattern of the
/// root `.gitattributes`. Returns whether the file changed.
pub fn set_lockable(
    git: &GitRunner,
    repo: &Path,
    pattern: &str,
    lockable: bool,
) -> Result<bool, LfsError> {
    validate_pattern(pattern)?;
    let before = std::fs::read(repo.join(".gitattributes")).ok();
    git.run(
        repo,
        &AccountContext::default(),
        [
            "lfs",
            "track",
            if lockable {
                "--lockable"
            } else {
                "--not-lockable"
            },
            "--",
            pattern,
        ],
    )?;
    Ok(std::fs::read(repo.join(".gitattributes")).ok() != before)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_verified_and_plain_lists() {
        // Shape of git-lfs 3.x `locks --verify --json`.
        let json = r#"{"ours":[{"id":"2","path":"art/b.psd","owner":{"name":"alice"},"locked_at":"2026-10-15T01:02:03Z"}],
"theirs":[{"id":"1","path":"a.psd","owner":{"name":"bob"},"locked_at":"2026-10-14T00:00:00Z"}]}"#;
        let locks = parse_verified_locks(json).unwrap();
        assert_eq!(locks.len(), 2);
        assert_eq!(locks[0].path, "a.psd");
        assert!(!locks[0].ours && locks[0].owner.as_deref() == Some("bob"));
        assert!(locks[1].ours);
        let empty = parse_verified_locks(r#"{"ours":null,"theirs":[]}"#).unwrap();
        assert!(empty.is_empty());

        let json = r#"[{"id":"1","path":"a.psd","owner":{"name":"Bob"},"locked_at":"x"},
{"id":"3","path":"c.psd","owner":{"name":""}}]"#;
        let locks = parse_locks(json, Some("bob")).unwrap();
        assert!(locks[0].ours, "case-insensitive owner match");
        assert!(!locks[1].ours && locks[1].owner.is_none() && locks[1].locked_at.is_none());
        assert!(parse_locks("[]", None).unwrap().is_empty());
        assert!(parse_locks("not json", None).is_err());

        let one = parse_lock(r#"{"id":"9","path":"x.bin","owner":{"name":"me"},"locked_at":"t"}"#)
            .unwrap();
        assert_eq!((one.id.as_str(), one.ours), ("9", true));
    }

    #[test]
    fn classifies_lock_failures() {
        let cases = [
            (
                "Locking a.bin failed: Lock exists",
                LockFailureKind::AlreadyLocked,
            ),
            (
                "Unable to unlock: lock is owned by another user, use --force",
                LockFailureKind::NotOwner,
            ),
            (
                "Error unlocking a.psd: Authorization error: https://x/info/lfs/locks/1/unlock\nCheck that you have proper access to the repository\n403 Forbidden",
                LockFailureKind::NoPermission,
            ),
            (
                "Locking a.bin failed: missing protocol: \"file:///C:/r.git\"",
                LockFailureKind::Unsupported,
            ),
            (
                "Remote \"origin\" does not support the Git LFS locking API. Consider disabling it with:",
                LockFailureKind::Unsupported,
            ),
            (
                "batch response: Authentication required: Authorization error: https://x\n401",
                LockFailureKind::AuthRequired,
            ),
            ("lock not found", LockFailureKind::NotFound),
            (
                "Post \"https://x\": dial tcp: lookup x: no such host",
                LockFailureKind::Network,
            ),
            ("something else", LockFailureKind::Other),
        ];
        for (msg, kind) in cases {
            assert_eq!(lock_failure_kind(msg), kind, "{msg}");
        }
    }

    #[test]
    fn rejects_bad_arguments_before_running_git() {
        let git = GitRunner::new("definitely-not-git");
        let dir = std::env::temp_dir();
        let ctx = AccountContext::default();
        assert!(matches!(
            lock(&git, &dir, &ctx, None, "../x"),
            Err(LockError::Lfs(LfsError::InvalidPath(_)))
        ));
        assert!(matches!(
            lock(&git, &dir, &ctx, Some("--upload"), "a"),
            Err(LockError::Lfs(LfsError::InvalidPath(_)))
        ));
        assert!(matches!(
            unlock(&git, &dir, &ctx, None, None, Some("1;rm"), false),
            Err(LockError::Lfs(LfsError::InvalidPath(_)))
        ));
        assert!(unlock(&git, &dir, &ctx, None, None, None, false).is_err());
        assert!(set_lockable(&git, &dir, "-x", true).is_err());
    }
}
