//! Push Guard plumbing (PLAN §4.6, §6.3): what a push of `<branch>` to `<remote>` would send,
//! heavy blobs in those commits, and the two history rewrites the guard offers — "fix author"
//! and "move to LFS" (`git lfs migrate import`) — restricted to **unpushed** commits.
//!
//! # Safety rules shared by every rewrite
//! - A backup ref `refs/nanomgg/backup/<unix-ts>` (suffix `-2`, `-3`… on collision) is created
//!   first, pointing at the branch tip, with a reflog entry `nanomgg-backup op=<op>
//!   branch=<branch>` so [`list_backups`] / [`restore_backup`] know what it belongs to.
//! - The rewrite is refused ([`FailureKind::InvalidInput`] with a clear message) when any
//!   commit it would touch is reachable from a remote-tracking ref — shared history is never
//!   rewritten.
//! - The branch must be checked out, with no operation in progress and no staged/unstaged
//!   changes to tracked files.
//! - On failure the rewrite is aborted (`rebase --abort`) and the branch restored; the backup
//!   ref stays so the user can always go back.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::branch::validate_branch_name;
use crate::ops::{
    FailureKind, OpError, check_arg, classify, current_branch, ensure_idle, ref_exists, writer,
};
use crate::{AccountContext, GitRunner};

/// Namespace of backup refs.
pub const BACKUP_PREFIX: &str = "refs/nanomgg/backup/";
/// Unpushed commits listed in detail (the count is always exact).
pub const MAX_LISTED_COMMITS: usize = 1000;
/// Lines sent to one `git cat-file --batch-check` process.
const BATCH_LINES: usize = 20_000;
/// Upper bound for the `--exclude-ref` list of `git lfs migrate` (OS command-line limits).
const MAX_EXCLUDE_ARG_BYTES: usize = 24_000;
const REFLOG_TAG: &str = "nanomgg-backup";

fn none() -> AccountContext {
    AccountContext::default()
}

fn rejected(message: impl Into<String>) -> OpError {
    OpError::rejected(FailureKind::InvalidInput, message)
}

// ----- push range ----------------------------------------------------------------------------

/// The commits a push of `branch` to `remote` would send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushRange {
    pub remote: String,
    pub branch: String,
    /// `refs/remotes/<remote>/<branch>` when it exists; `None` for a branch new on the remote
    /// (then every commit not on any remote-tracking ref counts as unpushed).
    pub upstream: Option<String>,
}

impl PushRange {
    /// Validates names and finds the remote-tracking ref.
    pub fn resolve(
        git: &GitRunner,
        repo: &Path,
        remote: &str,
        branch: &str,
    ) -> Result<Self, OpError> {
        check_arg(remote, "remote name")?;
        if remote.contains(['/', ' ', '\\']) {
            return Err(OpError::rejected(
                FailureKind::InvalidName,
                format!("invalid remote name: {remote:?}"),
            ));
        }
        validate_branch_name(git, repo, branch)?;
        if !ref_exists(git, repo, &format!("refs/heads/{branch}")) {
            return Err(OpError::rejected(
                FailureKind::NotFound,
                format!("branch '{branch}' not found"),
            ));
        }
        let tracking = format!("refs/remotes/{remote}/{branch}");
        Ok(Self {
            remote: remote.to_owned(),
            branch: branch.to_owned(),
            upstream: ref_exists(git, repo, &tracking).then_some(tracking),
        })
    }

    pub fn branch_ref(&self) -> String {
        format!("refs/heads/{}", self.branch)
    }

    /// Revision arguments selecting the unpushed commits (put `--` after them).
    pub fn rev_args(&self) -> Vec<String> {
        let mut args = vec![self.branch_ref()];
        match &self.upstream {
            Some(up) => args.push(format!("^{up}")),
            None => args.extend(["--not".to_owned(), "--remotes".to_owned()]),
        }
        args
    }
}

/// One commit the push would send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct UnpushedCommit {
    pub oid: String,
    pub short_oid: String,
    pub summary: String,
    pub author_name: String,
    pub author_email: String,
    pub committer_name: String,
    pub committer_email: String,
    /// Author time, Unix seconds.
    pub time: i64,
    /// Already reachable from another remote-tracking ref (e.g. pushed to another branch):
    /// such a commit must not be rewritten.
    pub shared: bool,
}

impl UnpushedCommit {
    /// Author or committer email differs (case-insensitively) from `email`.
    pub fn mismatches(&self, email: &str) -> bool {
        let email = email.trim();
        !self.author_email.trim().eq_ignore_ascii_case(email)
            || !self.committer_email.trim().eq_ignore_ascii_case(email)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct UnpushedReport {
    pub remote: String,
    pub branch: String,
    /// Remote-tracking ref the range starts from; `None` for a new branch.
    pub upstream: Option<String>,
    /// Newest first, at most [`MAX_LISTED_COMMITS`].
    pub commits: Vec<UnpushedCommit>,
    /// Exact number of unpushed commits.
    pub total: u32,
    /// Every unpushed commit can be rewritten (none is on a remote-tracking ref).
    pub rewritable: bool,
}

/// Commits reachable from `branch` but from no remote-tracking ref.
fn local_only_set(
    git: &GitRunner,
    repo: &Path,
    branch_ref: &str,
) -> Result<HashSet<String>, OpError> {
    let out = git.run_text(
        repo,
        &none(),
        ["rev-list", branch_ref, "--not", "--remotes", "--"],
    )?;
    Ok(out.lines().map(str::to_owned).collect())
}

fn rev_list(git: &GitRunner, repo: &Path, args: &[String]) -> Result<Vec<String>, OpError> {
    let mut full = vec!["rev-list".to_owned()];
    full.extend(args.iter().cloned());
    full.push("--".into());
    Ok(git
        .run_text(repo, &none(), &full)?
        .lines()
        .map(str::to_owned)
        .collect())
}

/// Parses `git log --format=%H%x1f%h%x1f%an%x1f%ae%x1f%cn%x1f%ce%x1f%at%x1f%s%x1e`.
pub fn parse_commit_records(raw: &str, local_only: &HashSet<String>) -> Vec<UnpushedCommit> {
    raw.split('\x1e')
        .filter_map(|rec| {
            let rec = rec.trim_start_matches(['\n', '\r']);
            if rec.is_empty() {
                return None;
            }
            let f: Vec<&str> = rec.splitn(8, '\x1f').collect();
            if f.len() < 8 {
                return None;
            }
            Some(UnpushedCommit {
                oid: f[0].to_owned(),
                short_oid: f[1].to_owned(),
                author_name: f[2].to_owned(),
                author_email: f[3].to_owned(),
                committer_name: f[4].to_owned(),
                committer_email: f[5].to_owned(),
                time: f[6].trim().parse().unwrap_or(0),
                summary: f[7].trim_end().to_owned(),
                shared: !local_only.contains(f[0]),
            })
        })
        .collect()
}

/// The commits a push of `branch` to `remote` would send, with author/committer.
pub fn unpushed(
    git: &GitRunner,
    repo: &Path,
    range: &PushRange,
) -> Result<UnpushedReport, OpError> {
    let rev = range.rev_args();
    let mut count_args = vec!["rev-list".to_owned(), "--count".to_owned()];
    count_args.extend(rev.iter().cloned());
    count_args.push("--".into());
    let total: u32 = git
        .run_text(repo, &none(), &count_args)?
        .trim()
        .parse()
        .unwrap_or(0);
    let local_only = local_only_set(git, repo, &range.branch_ref())?;
    let mut args = vec![
        "log".to_owned(),
        "--no-show-signature".to_owned(),
        "--no-color".to_owned(),
        format!("--max-count={MAX_LISTED_COMMITS}"),
        "--format=%H%x1f%h%x1f%an%x1f%ae%x1f%cn%x1f%ce%x1f%at%x1f%s%x1e".to_owned(),
    ];
    args.extend(rev.iter().cloned());
    args.push("--".into());
    let raw = if total == 0 {
        String::new()
    } else {
        git.run_text(repo, &none(), &args)?
    };
    let commits = parse_commit_records(&raw, &local_only);
    let rewritable = if total == 0 {
        true
    } else {
        let all = rev_list(git, repo, &rev)?;
        all.iter().all(|c| local_only.contains(c))
    };
    Ok(UnpushedReport {
        remote: range.remote.clone(),
        branch: range.branch.clone(),
        upstream: range.upstream.clone(),
        commits,
        total,
        rewritable,
    })
}

/// Unpushed commits whose author or committer email differs from `email`.
pub fn author_mismatches<'a>(
    commits: &'a [UnpushedCommit],
    email: &str,
) -> Vec<&'a UnpushedCommit> {
    commits.iter().filter(|c| c.mismatches(email)).collect()
}

// ----- heavy blobs ---------------------------------------------------------------------------

/// A blob over the threshold, introduced by unpushed commits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct HeavyBlob {
    pub oid: String,
    /// Path under which the blob was first seen (`/` separators).
    pub path: String,
    pub size: u64,
    /// Unpushed commits that add or modify a path to this blob, newest first.
    pub commits: Vec<String>,
    /// Introduced by a commit that is already on a remote-tracking ref (or by no plain commit,
    /// e.g. a merge resolution): it cannot be moved to LFS without rewriting shared history.
    pub in_shared_commit: bool,
}

/// Parses `cat-file --batch-check='%(objecttype) %(objectname) %(objectsize) %(rest)'`,
/// keeping blobs of at least `threshold` bytes as `(oid, size, path)`.
pub fn parse_heavy_batch(raw: &str, threshold: u64) -> Vec<(String, u64, String)> {
    raw.lines()
        .filter_map(|line| {
            let mut it = line.splitn(4, ' ');
            let kind = it.next()?;
            let oid = it.next()?;
            let size: u64 = it.next()?.parse().ok()?;
            let rest = it.next().unwrap_or("");
            (kind == "blob" && size >= threshold).then(|| (oid.to_owned(), size, rest.to_owned()))
        })
        .collect()
}

/// Parses `git log --raw --no-abbrev --format=%x1e%H`: blob oid → commits that wrote it.
pub fn parse_raw_introductions(raw: &str, wanted: &HashSet<&str>) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    let mut commit = "";
    for line in raw.lines() {
        if let Some(c) = line.strip_prefix('\x1e') {
            commit = c.trim();
        } else if let Some(entry) = line.strip_prefix(':') {
            // `:100644 100644 <old> <new> M\tpath`
            let meta = entry.split('\t').next().unwrap_or("");
            let fields: Vec<&str> = meta.split_whitespace().collect();
            if fields.len() >= 5
                && !fields[4].starts_with('D')
                && wanted.contains(fields[3])
                && !commit.is_empty()
            {
                let list = out.entry(fields[3].to_owned()).or_default();
                if !list.iter().any(|c| c == commit) {
                    list.push(commit.to_owned());
                }
            }
        }
    }
    out
}

/// Blobs of at least `threshold` bytes in the unpushed commits (PLAN §6.3), biggest first.
pub fn heavy_blobs(
    git: &GitRunner,
    repo: &Path,
    range: &PushRange,
    threshold: u64,
) -> Result<Vec<HeavyBlob>, OpError> {
    let rev = range.rev_args();
    let mut args = vec!["rev-list".to_owned(), "--objects".to_owned()];
    args.extend(rev.iter().cloned());
    args.push("--".into());
    let listing = git.run(repo, &none(), &args)?;
    let listing = String::from_utf8_lossy(&listing.stdout);
    // Commits have no name; trees and blobs do (the root tree's name is empty).
    let objects: Vec<&str> = listing
        .lines()
        .filter(|l| l.len() > 41 && l.contains(' '))
        .collect();
    let mut found = Vec::new();
    for chunk in objects.chunks(BATCH_LINES) {
        let mut input = chunk.join("\n");
        input.push('\n');
        let out = git.run_with_input(
            repo,
            &none(),
            [
                "cat-file",
                "--batch-check=%(objecttype) %(objectname) %(objectsize) %(rest)",
            ],
            input.as_bytes(),
        )?;
        found.extend(parse_heavy_batch(
            &String::from_utf8_lossy(&out.stdout),
            threshold,
        ));
    }
    if found.is_empty() {
        return Ok(Vec::new());
    }
    let mut seen = HashSet::new();
    found.retain(|(oid, _, _)| seen.insert(oid.clone()));

    let wanted: HashSet<&str> = found.iter().map(|(oid, _, _)| oid.as_str()).collect();
    let mut log = vec![
        "log".to_owned(),
        "--raw".to_owned(),
        "--no-abbrev".to_owned(),
        "--no-renames".to_owned(),
        "--no-color".to_owned(),
        "--format=%x1e%H".to_owned(),
    ];
    log.extend(rev.iter().cloned());
    log.push("--".into());
    let raw = git.run(repo, &none(), &log)?;
    let intro = parse_raw_introductions(&String::from_utf8_lossy(&raw.stdout), &wanted);
    let local_only = local_only_set(git, repo, &range.branch_ref())?;

    let mut blobs: Vec<HeavyBlob> = found
        .into_iter()
        .map(|(oid, size, path)| {
            let commits = intro.get(&oid).cloned().unwrap_or_default();
            let in_shared_commit =
                commits.is_empty() || commits.iter().any(|c| !local_only.contains(c));
            HeavyBlob {
                oid,
                path: path.replace('\\', "/"),
                size,
                commits,
                in_shared_commit,
            }
        })
        .collect();
    blobs.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));
    Ok(blobs)
}

// ----- backups -------------------------------------------------------------------------------

/// A backup ref made before a rewrite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BackupRef {
    /// `refs/nanomgg/backup/<unix-ts>[-n]`.
    pub name: String,
    pub oid: String,
    /// Branch it was taken from (from the reflog note; `None` if unknown).
    pub branch: Option<String>,
    /// `fix-authors`, `lfs-migrate`, `undo`…
    pub operation: Option<String>,
    /// Unix seconds, from the ref name.
    pub created_at: i64,
    pub summary: String,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn is_backup_name(name: &str) -> bool {
    name.strip_prefix(BACKUP_PREFIX).is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .split('-')
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
            && rest.split('-').count() <= 2
    })
}

/// Creates `refs/nanomgg/backup/<ts>` (create-only) at `oid` with a reflog note.
pub fn create_backup(
    git: &GitRunner,
    repo: &Path,
    oid: &str,
    branch: &str,
    operation: &str,
) -> Result<String, OpError> {
    let ts = now_secs();
    let message = format!("{REFLOG_TAG} op={operation} branch={branch}");
    for n in 1..=50u32 {
        let name = if n == 1 {
            format!("{BACKUP_PREFIX}{ts}")
        } else {
            format!("{BACKUP_PREFIX}{ts}-{n}")
        };
        let created = git.run(
            repo,
            &none(),
            [
                "update-ref",
                "--create-reflog",
                "-m",
                &message,
                &name,
                oid,
                "",
            ],
        );
        if created.is_ok() {
            return Ok(name);
        }
        if !ref_exists(git, repo, &name) {
            return Err(classify(created.unwrap_err()));
        }
    }
    Err(rejected("could not allocate a backup ref name"))
}

/// Backup refs, newest first.
pub fn list_backups(git: &GitRunner, repo: &Path) -> Result<Vec<BackupRef>, OpError> {
    let raw = git.run_text(
        repo,
        &none(),
        [
            "for-each-ref",
            "--format=%(refname)%1f%(objectname)%1f%(contents:subject)",
            BACKUP_PREFIX,
        ],
    )?;
    let mut out = Vec::new();
    for line in raw.lines() {
        let f: Vec<&str> = line.splitn(3, '\x1f').collect();
        if f.len() < 2 || !is_backup_name(f[0]) {
            continue;
        }
        let note = git
            .run_text(
                repo,
                &none(),
                ["log", "-g", "-n", "1", "--format=%gs", f[0], "--"],
            )
            .unwrap_or_default();
        let (operation, branch) = parse_backup_note(&note);
        let created_at = f[0][BACKUP_PREFIX.len()..]
            .split('-')
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        out.push(BackupRef {
            name: f[0].to_owned(),
            oid: f[1].to_owned(),
            branch,
            operation,
            created_at,
            summary: f.get(2).copied().unwrap_or("").to_owned(),
        });
    }
    out.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.name.cmp(&a.name))
    });
    Ok(out)
}

/// `nanomgg-backup op=<op> branch=<branch>` → `(op, branch)`.
pub fn parse_backup_note(note: &str) -> (Option<String>, Option<String>) {
    let Some(rest) = note.trim().strip_prefix(REFLOG_TAG) else {
        return (None, None);
    };
    let mut op = None;
    let mut branch = None;
    for word in rest.split_whitespace() {
        if let Some(v) = word.strip_prefix("op=") {
            op = Some(v.to_owned());
        } else if let Some(v) = word.strip_prefix("branch=") {
            branch = Some(v.to_owned());
        }
    }
    (op, branch)
}

/// Deletes a backup ref.
pub fn delete_backup(git: &GitRunner, repo: &Path, name: &str) -> Result<(), OpError> {
    if !is_backup_name(name) {
        return Err(rejected(format!("not a backup ref: {name:?}")));
    }
    git.run(repo, &none(), ["update-ref", "-d", name])
        .map_err(classify)?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RestoreOutcome {
    pub branch: String,
    /// Branch tip before the restore.
    pub previous: String,
    pub restored: String,
    /// Backup of `previous` (so the undo itself can be undone).
    pub backup_ref: String,
}

/// Undo: points `branch` (default: the one recorded with the backup) back at `backup`. The
/// checked-out branch is moved with `git reset --keep` (local changes survive unless they
/// touch files that differ); another branch with `update-ref`.
pub fn restore_backup(
    git: &GitRunner,
    repo: &Path,
    backup: &str,
    branch: Option<&str>,
) -> Result<RestoreOutcome, OpError> {
    if !is_backup_name(backup) || !ref_exists(git, repo, backup) {
        return Err(OpError::rejected(
            FailureKind::NotFound,
            format!("backup ref not found: {backup}"),
        ));
    }
    let recorded = list_backups(git, repo)?
        .into_iter()
        .find(|b| b.name == backup)
        .and_then(|b| b.branch);
    let branch = match branch.map(str::to_owned).or(recorded) {
        Some(b) => b,
        None => return Err(rejected("the backup does not record its branch; pick one")),
    };
    validate_branch_name(git, repo, &branch)?;
    ensure_idle(git, repo)?;
    let target = git.run_text(
        repo,
        &none(),
        ["rev-parse", "--verify", &format!("{backup}^{{commit}}")],
    )?;
    let branch_ref = format!("refs/heads/{branch}");
    let previous = git.run_text(repo, &none(), ["rev-parse", "--verify", &branch_ref])?;
    let backup_ref = create_backup(git, repo, &previous, &branch, "undo")?;
    if current_branch(git, repo).as_deref() == Some(branch.as_str()) {
        writer(git)
            .run(repo, &none(), ["reset", "-q", "--keep", &target])
            .map_err(classify)?;
    } else {
        git.run(
            repo,
            &none(),
            [
                "update-ref",
                "-m",
                "nanomgg: restore backup",
                &branch_ref,
                &target,
                &previous,
            ],
        )
        .map_err(classify)?;
    }
    Ok(RestoreOutcome {
        branch,
        previous,
        restored: target,
        backup_ref,
    })
}

// ----- rewrites ------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum FixAuthorMode {
    /// `git commit --amend --reset-author` on the last commit.
    AmendLast,
    /// `git rebase -r --exec 'git commit --amend --reset-author'` over every unpushed commit.
    RewriteUnpushed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RewriteOutcome {
    pub branch: String,
    /// Backup of the branch tip before the rewrite (undo target).
    pub backup_ref: String,
    pub old_head: String,
    pub new_head: String,
    /// Commits rewritten.
    pub rewritten: u32,
}

/// Branch checked out, idle, no staged/unstaged changes to tracked files.
fn ensure_rewritable_state(git: &GitRunner, repo: &Path, branch: &str) -> Result<String, OpError> {
    if current_branch(git, repo).as_deref() != Some(branch) {
        return Err(OpError::rejected(
            FailureKind::InvalidInput,
            format!("check out '{branch}' first: only the current branch can be rewritten"),
        ));
    }
    ensure_idle(git, repo)?;
    let dirty = git.run_text(
        repo,
        &none(),
        [
            "status",
            "--porcelain",
            "--untracked-files=no",
            "--ignore-submodules",
        ],
    )?;
    if !dirty.is_empty() {
        return Err(OpError::rejected(
            FailureKind::LocalChanges,
            "commit or stash your changes before rewriting commits",
        ));
    }
    Ok(git.run_text(repo, &none(), ["rev-parse", "--verify", "HEAD"])?)
}

fn shared_refusal(count: usize) -> OpError {
    rejected(format!(
        "{count} of these commits are already on a remote branch; pushed history is never rewritten"
    ))
}

/// Commits `rev-list <args>` that are reachable from a remote-tracking ref.
fn shared_in(
    git: &GitRunner,
    repo: &Path,
    args: &[String],
    local_only: &HashSet<String>,
) -> Result<(Vec<String>, usize), OpError> {
    let commits = rev_list(git, repo, args)?;
    let shared = commits.iter().filter(|c| !local_only.contains(*c)).count();
    Ok((commits, shared))
}

/// Base for rewriting every unpushed commit: `None` → `--root`.
fn rewrite_base(
    git: &GitRunner,
    repo: &Path,
    range: &PushRange,
) -> Result<Option<String>, OpError> {
    let mut args = vec!["rev-list".to_owned(), "--boundary".to_owned()];
    args.extend(range.rev_args());
    args.push("--".into());
    let out = git.run_text(repo, &none(), &args)?;
    let boundary: Vec<&str> = out.lines().filter_map(|l| l.strip_prefix('-')).collect();
    match boundary.as_slice() {
        [] => Ok(None),
        [one] => Ok(Some((*one).to_owned())),
        many => {
            let mut mb = vec!["merge-base", "--octopus"];
            mb.extend(many.iter().copied());
            Ok(Some(git.run_text(repo, &none(), &mb)?))
        }
    }
}

/// "Sửa tác giả": re-authors unpushed commits of the checked-out `branch` with the identity of
/// `ctx` (`--reset-author` takes `user.name`/`user.email` from the injected config; without an
/// identity, git's own config is used). See the module docs for the safety rules.
pub fn fix_authors(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    range: &PushRange,
    mode: FixAuthorMode,
) -> Result<RewriteOutcome, OpError> {
    let old_head = ensure_rewritable_state(git, repo, &range.branch)?;
    let local_only = local_only_set(git, repo, &range.branch_ref())?;
    let pending = rev_list(git, repo, &range.rev_args())?;
    if pending.is_empty() {
        return Err(OpError::rejected(
            FailureKind::NothingToCommit,
            "there are no unpushed commits to fix",
        ));
    }
    let w = writer(git).with_env("GIT_SEQUENCE_EDITOR", ":");
    match mode {
        FixAuthorMode::AmendLast => {
            if !local_only.contains(&old_head) {
                return Err(shared_refusal(1));
            }
            let backup_ref = create_backup(git, repo, &old_head, &range.branch, "fix-authors")?;
            w.run(
                repo,
                ctx,
                [
                    "commit",
                    "-q",
                    "--amend",
                    "--only",
                    "--no-edit",
                    "--reset-author",
                    "--allow-empty",
                    "--no-verify",
                ],
            )
            .map_err(classify)?;
            let new_head = git.run_text(repo, &none(), ["rev-parse", "HEAD"])?;
            Ok(RewriteOutcome {
                branch: range.branch.clone(),
                backup_ref,
                old_head,
                new_head,
                rewritten: 1,
            })
        }
        FixAuthorMode::RewriteUnpushed => {
            let base = rewrite_base(git, repo, range)?;
            let mut rewrite_args = vec![range.branch_ref()];
            if let Some(b) = &base {
                rewrite_args.push(format!("^{b}"));
            }
            let (commits, shared) = shared_in(git, repo, &rewrite_args, &local_only)?;
            if shared > 0 {
                return Err(shared_refusal(shared));
            }
            let backup_ref = create_backup(git, repo, &old_head, &range.branch, "fix-authors")?;
            let exec =
                "git commit -q --amend --only --no-edit --reset-author --allow-empty --no-verify";
            let mut args = vec!["rebase", "-q", "-r", "--exec", exec];
            match &base {
                Some(b) => args.push(b),
                None => args.push("--root"),
            }
            if let Err(e) = w.run(repo, ctx, &args) {
                let err = classify(e);
                let _ = w.run(repo, &none(), ["rebase", "--abort"]);
                if git
                    .run_text(repo, &none(), ["rev-parse", "HEAD"])
                    .ok()
                    .as_deref()
                    != Some(old_head.as_str())
                {
                    let _ = w.run(repo, &none(), ["reset", "-q", "--keep", &old_head]);
                }
                return Err(match err {
                    OpError::Rejected { kind, message } => OpError::Rejected {
                        kind,
                        message: format!(
                            "{message}\n(rewrite aborted; backup kept at {backup_ref})"
                        ),
                    },
                    other => other,
                });
            }
            let new_head = git.run_text(repo, &none(), ["rev-parse", "HEAD"])?;
            Ok(RewriteOutcome {
                branch: range.branch.clone(),
                backup_ref,
                old_head,
                new_head,
                rewritten: commits.len() as u32,
            })
        }
    }
}

/// Paths accepted by [`lfs_migrate_import`]: repository-relative, no option/list syntax.
fn validate_migrate_path(path: &str) -> Result<String, OpError> {
    let p = path.trim().replace('\\', "/");
    let p = p.trim_start_matches("./").to_owned();
    if p.is_empty()
        || p.starts_with('-')
        || p.starts_with('/')
        || p.contains(',')
        || p.chars().any(char::is_control)
        || p.split('/').any(|s| s == "..")
    {
        return Err(rejected(format!("invalid path: {path:?}")));
    }
    Ok(p)
}

/// Local refs (not remote-tracking, not `except`) → object id.
fn local_refs(
    git: &GitRunner,
    repo: &Path,
    except: &str,
) -> Result<HashMap<String, String>, OpError> {
    let raw = git.run_text(
        repo,
        &none(),
        ["for-each-ref", "--format=%(refname) %(objectname)"],
    )?;
    Ok(raw
        .lines()
        .filter_map(|l| l.rsplit_once(' '))
        .filter(|(name, _)| *name != except && !name.starts_with("refs/remotes/"))
        .map(|(n, o)| (n.to_owned(), o.to_owned()))
        .collect())
}

/// Points refs moved by `git lfs migrate` back to where they were.
fn restore_moved_refs(
    git: &GitRunner,
    repo: &Path,
    except: &str,
    before: &HashMap<String, String>,
) {
    let Ok(after) = local_refs(git, repo, except) else {
        return;
    };
    for (name, old) in before {
        if let Some(new) = after.get(name)
            && new != old
            && let Err(e) = git.run(
                repo,
                &none(),
                ["update-ref", "-m", "nanomgg: keep ref", name, old, new],
            )
        {
            tracing_free_warn(name, &e.to_string());
        }
    }
}

/// git-engine has no logging dependency: surface the problem on stderr (debug builds) only.
fn tracing_free_warn(name: &str, error: &str) {
    if cfg!(debug_assertions) {
        eprintln!("guard: could not restore {name}: {error}");
    }
}

/// Whether `git lfs` is installed.
pub fn lfs_available(git: &GitRunner, repo: &Path) -> bool {
    git.run(repo, &none(), ["lfs", "version"]).is_ok()
}

/// "Chuyển sang LFS" (PLAN §6.3): `git lfs migrate import --include=<paths>
/// --include-ref=refs/heads/<branch> --exclude-ref=<remote-tracking refs>` — rewrites only
/// the unpushed commits of the checked-out `branch`, after a backup.
pub fn lfs_migrate_import(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    range: &PushRange,
    paths: &[String],
) -> Result<RewriteOutcome, OpError> {
    if paths.is_empty() {
        return Err(rejected("no paths to move to LFS"));
    }
    let paths = paths
        .iter()
        .map(|p| validate_migrate_path(p))
        .collect::<Result<Vec<_>, _>>()?;
    if !lfs_available(git, repo) {
        return Err(OpError::rejected(
            FailureKind::NotFound,
            "git-lfs is not installed",
        ));
    }
    let old_head = ensure_rewritable_state(git, repo, &range.branch)?;
    let local_only = local_only_set(git, repo, &range.branch_ref())?;
    let (pending, shared) = shared_in(git, repo, &range.rev_args(), &local_only)?;
    if pending.is_empty() {
        return Err(OpError::rejected(
            FailureKind::NothingToCommit,
            "there are no unpushed commits to rewrite",
        ));
    }
    if shared > 0 {
        return Err(shared_refusal(shared));
    }
    let excludes: Vec<String> = match &range.upstream {
        Some(up) => vec![up.clone()],
        None => git
            .run_text(
                repo,
                &none(),
                ["for-each-ref", "--format=%(refname)", "refs/remotes/"],
            )?
            .lines()
            .filter(|r| !r.ends_with("/HEAD"))
            .map(str::to_owned)
            .collect(),
    };
    let exclude_bytes: usize = excludes.iter().map(|e| e.len() + 16).sum();
    if exclude_bytes > MAX_EXCLUDE_ARG_BYTES {
        return Err(rejected(
            "too many remote branches to exclude; push the branch once or set its upstream first",
        ));
    }
    let backup_ref = create_backup(git, repo, &old_head, &range.branch, "lfs-migrate")?;
    // git-lfs moves *every* local ref that points at a rewritten commit (other branches, tags,
    // even our backups): remember them and put them back afterwards, so only `branch` changes.
    let branch_ref = range.branch_ref();
    let before = local_refs(git, repo, &branch_ref)?;
    let mut args = vec![
        "lfs".to_owned(),
        "migrate".to_owned(),
        "import".to_owned(),
        format!("--include={}", paths.join(",")),
        format!("--include-ref={branch_ref}"),
    ];
    args.extend(excludes.iter().map(|e| format!("--exclude-ref={e}")));
    let result = writer(git).run(repo, ctx, &args);
    restore_moved_refs(git, repo, &branch_ref, &before);
    if let Err(e) = result {
        // Put the branch back if git-lfs got as far as moving it.
        let now = git
            .run_text(repo, &none(), ["rev-parse", "HEAD"])
            .unwrap_or_default();
        if now != old_head {
            let _ = writer(git).run(repo, &none(), ["reset", "-q", "--keep", &old_head]);
        }
        return Err(match classify(e) {
            OpError::Rejected { kind, message } => OpError::Rejected {
                kind,
                message: format!("{message}\n(backup kept at {backup_ref})"),
            },
            other => other,
        });
    }
    // migrate's own checkout leaves pointer files in the working tree: fill in the content
    // from the local LFS store (best effort; `git lfs pull` would do it too).
    let mut checkout = vec!["lfs".to_owned(), "checkout".to_owned()];
    checkout.extend(paths.iter().cloned());
    let _ = writer(git).run(repo, ctx, &checkout);
    let new_head = git.run_text(repo, &none(), ["rev-parse", "HEAD"])?;
    Ok(RewriteOutcome {
        branch: range.branch.clone(),
        backup_ref,
        old_head,
        new_head,
        rewritten: pending.len() as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Identity;
    use crate::progress::test_support::TestRepo;

    const MIB: usize = 1024 * 1024;

    /// `work` with `main` pushed to a bare `origin` (upstream set), autocrlf off.
    struct Setup {
        _dir: tempfile::TempDir,
        work: TestRepo,
    }

    fn setup() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let work = TestRepo::init_at(&dir.path().join("work"));
        work.git(&["config", "core.autocrlf", "false"]);
        work.git
            .run(
                dir.path(),
                &work.ctx,
                ["init", "-q", "--bare", "-b", "main", "remote.git"],
            )
            .unwrap();
        let url = dir
            .path()
            .join("remote.git")
            .to_string_lossy()
            .replace('\\', "/");
        work.git(&["remote", "add", "origin", &url]);
        work.commit("pushed.txt", "pushed\n", "pushed commit");
        work.git(&["push", "-q", "-u", "origin", "main"]);
        Setup { _dir: dir, work }
    }

    fn as_ctx(name: &str, email: &str) -> AccountContext {
        AccountContext {
            identity: Some(Identity {
                name: name.into(),
                email: email.into(),
            }),
            ..Default::default()
        }
    }

    fn commit_as(repo: &TestRepo, ctx: &AccountContext, file: &str, msg: &str) -> String {
        repo.write(file, &format!("{msg}\n"));
        repo.git(&["add", "--", file]);
        repo.git
            .run(&repo.path, ctx, ["commit", "-q", "-m", msg])
            .unwrap();
        repo.head()
    }

    fn pseudo_random(len: usize, seed: u64) -> Vec<u8> {
        let mut x = seed | 1;
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x as u8
            })
            .collect()
    }

    fn range(s: &Setup) -> PushRange {
        PushRange::resolve(&s.work.git, &s.work.path, "origin", "main").unwrap()
    }

    fn author_of(repo: &TestRepo, rev: &str) -> String {
        repo.git(&["log", "-1", "--format=%ae|%ce", rev])
    }

    #[test]
    fn parses_records_and_batches() {
        let local: HashSet<String> = ["a1".to_owned()].into();
        let raw = "a1\x1fa\x1fAnn\x1fann@x.dev\x1fAnn\x1fann@x.dev\x1f100\x1fhello\x1e\nb2\x1fb\x1fBob\x1fbob@y.dev\x1fC\x1fc@y.dev\x1f200\x1fworld\x1e\n";
        let commits = parse_commit_records(raw, &local);
        assert_eq!(commits.len(), 2);
        assert!(!commits[0].shared && commits[1].shared);
        assert_eq!(commits[1].committer_email, "c@y.dev");
        assert!(!commits[0].mismatches("ANN@x.dev"));
        assert!(commits[1].mismatches("bob@y.dev"));
        assert_eq!(author_mismatches(&commits, "ann@x.dev").len(), 1);

        let batch = "blob aaa 2097152 big/file.bin\nblob bbb 10 small.txt\ntree ccc 99999999 dir\nblob ddd 1048576 path with space.psd\n";
        let heavy = parse_heavy_batch(batch, MIB as u64);
        assert_eq!(
            heavy,
            vec![
                ("aaa".to_owned(), 2_097_152, "big/file.bin".to_owned()),
                (
                    "ddd".to_owned(),
                    1_048_576,
                    "path with space.psd".to_owned()
                )
            ]
        );

        let wanted: HashSet<&str> = ["n1"].into();
        let raw = "\x1ec2\n\n:000000 100644 0000 n1 A\tbig.bin\n\x1ec1\n\n:100644 000000 n1 0000 D\tbig.bin\n";
        let intro = parse_raw_introductions(raw, &wanted);
        assert_eq!(intro.get("n1").unwrap(), &vec!["c2".to_owned()]);

        assert_eq!(
            parse_backup_note("nanomgg-backup op=fix-authors branch=feature/x"),
            (Some("fix-authors".into()), Some("feature/x".into()))
        );
        assert_eq!(parse_backup_note("reset: moving"), (None, None));
        assert!(is_backup_name("refs/nanomgg/backup/1700000000"));
        assert!(is_backup_name("refs/nanomgg/backup/1700000000-2"));
        assert!(!is_backup_name("refs/heads/main"));
        assert!(!is_backup_name("refs/nanomgg/backup/../heads/main"));
        assert!(validate_migrate_path("--include=x").is_err());
        assert!(validate_migrate_path("a,b").is_err());
        assert!(validate_migrate_path("../x").is_err());
        assert_eq!(
            validate_migrate_path(r"assets\big.bin").unwrap(),
            "assets/big.bin"
        );
    }

    #[test]
    fn detects_unpushed_commits_and_author_mismatch() {
        let s = setup();
        let w = &s.work;
        let personal = as_ctx("Me", "me@gmail.com");
        commit_as(w, &w.ctx, "a.txt", "work 1");
        let bad = commit_as(w, &personal, "b.txt", "personal 2");
        let r = range(&s);
        assert_eq!(r.upstream.as_deref(), Some("refs/remotes/origin/main"));
        let report = unpushed(&w.git, &w.path, &r).unwrap();
        assert_eq!(report.total, 2);
        assert!(report.rewritable);
        assert_eq!(report.commits[0].oid, bad);
        assert_eq!(report.commits[0].author_email, "me@gmail.com");
        assert_eq!(report.commits[1].summary, "work 1");
        let mismatched = author_mismatches(&report.commits, "tester@example.com");
        assert_eq!(mismatched.len(), 1);
        assert_eq!(mismatched[0].oid, bad);

        // New branch: everything not on a remote ref.
        w.git(&["checkout", "-q", "-b", "feature"]);
        commit_as(w, &w.ctx, "c.txt", "feature 1");
        let fr = PushRange::resolve(&w.git, &w.path, "origin", "feature").unwrap();
        assert_eq!(fr.upstream, None);
        assert_eq!(unpushed(&w.git, &w.path, &fr).unwrap().total, 3);

        // Validation.
        assert!(PushRange::resolve(&w.git, &w.path, "--upload-pack=x", "main").is_err());
        assert!(PushRange::resolve(&w.git, &w.path, "origin", "nope").is_err());
    }

    #[test]
    fn heavy_blob_scan_finds_2mb_blob_over_1mb() {
        let s = setup();
        let w = &s.work;
        std::fs::create_dir_all(w.path.join("assets")).unwrap();
        std::fs::write(w.path.join("assets/big.bin"), pseudo_random(2 * MIB, 7)).unwrap();
        std::fs::write(w.path.join("small.bin"), pseudo_random(1000, 3)).unwrap();
        w.git(&["add", "."]);
        w.git(&["commit", "-q", "-m", "add big"]);
        let intro = w.head();
        w.commit("later.txt", "x\n", "later");

        let blobs = heavy_blobs(&w.git, &w.path, &range(&s), MIB as u64).unwrap();
        assert_eq!(blobs.len(), 1, "{blobs:?}");
        assert_eq!(blobs[0].path, "assets/big.bin");
        assert_eq!(blobs[0].size, 2 * MIB as u64);
        assert_eq!(blobs[0].commits, vec![intro]);
        assert!(!blobs[0].in_shared_commit);
        assert!(
            heavy_blobs(&w.git, &w.path, &range(&s), 3 * MIB as u64)
                .unwrap()
                .is_empty()
        );

        // Once pushed, it is no longer in the range.
        w.git(&["push", "-q", "origin", "main"]);
        assert!(
            heavy_blobs(&w.git, &w.path, &range(&s), MIB as u64)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn fix_author_rewrites_only_unpushed_commits() {
        let s = setup();
        let w = &s.work;
        let pushed = w.head();
        let personal = as_ctx("Me", "me@gmail.com");
        for i in 0..3 {
            commit_as(w, &personal, &format!("f{i}.txt"), &format!("unpushed {i}"));
        }
        let before = w.head();
        let work = as_ctx("Work Me", "me@acme.dev");
        let out = fix_authors(
            &w.git,
            &w.path,
            &work,
            &range(&s),
            FixAuthorMode::RewriteUnpushed,
        )
        .unwrap();
        assert_eq!(out.rewritten, 3);
        assert_eq!(out.old_head, before);
        assert!(out.backup_ref.starts_with(BACKUP_PREFIX));
        assert_eq!(w.git(&["rev-parse", &out.backup_ref]), before);
        for i in 0..3 {
            assert_eq!(
                author_of(w, &format!("HEAD~{i}")),
                "me@acme.dev|me@acme.dev"
            );
        }
        // Pushed commit untouched; messages kept.
        assert_eq!(w.git(&["rev-parse", "HEAD~3"]), pushed);
        assert_eq!(w.git(&["log", "-1", "--format=%s", "HEAD~1"]), "unpushed 1");
        let report = unpushed(&w.git, &w.path, &range(&s)).unwrap();
        assert!(author_mismatches(&report.commits, "me@acme.dev").is_empty());

        // Backups listed with their branch; undo restores the old tip.
        let backups = list_backups(&w.git, &w.path).unwrap();
        assert_eq!(backups[0].name, out.backup_ref);
        assert_eq!(backups[0].branch.as_deref(), Some("main"));
        assert_eq!(backups[0].operation.as_deref(), Some("fix-authors"));
        let undo = restore_backup(&w.git, &w.path, &out.backup_ref, None).unwrap();
        assert_eq!(undo.previous, out.new_head);
        assert_eq!(w.head(), before);
        assert_eq!(author_of(w, "HEAD"), "me@gmail.com|me@gmail.com");
        assert_eq!(list_backups(&w.git, &w.path).unwrap().len(), 2);
        delete_backup(&w.git, &w.path, &undo.backup_ref).unwrap();
        assert_eq!(list_backups(&w.git, &w.path).unwrap().len(), 1);

        // Amend last only touches HEAD.
        let parent = w.git(&["rev-parse", "HEAD~1"]);
        let out =
            fix_authors(&w.git, &w.path, &work, &range(&s), FixAuthorMode::AmendLast).unwrap();
        assert_eq!(out.rewritten, 1);
        assert_eq!(author_of(w, "HEAD"), "me@acme.dev|me@acme.dev");
        assert_eq!(w.git(&["rev-parse", "HEAD~1"]), parent);
    }

    #[test]
    fn refuses_rewrites_touching_pushed_commits() {
        let s = setup();
        let w = &s.work;
        let personal = as_ctx("Me", "me@gmail.com");
        commit_as(w, &personal, "a.txt", "a");
        // Also pushed to another remote branch: shared history.
        w.git(&["push", "-q", "origin", "main:other"]);
        commit_as(w, &personal, "b.txt", "b");
        let before = w.head();
        let report = unpushed(&w.git, &w.path, &range(&s)).unwrap();
        assert_eq!(report.total, 2);
        assert!(!report.rewritable);
        assert!(report.commits[1].shared && !report.commits[0].shared);
        let err = fix_authors(
            &w.git,
            &w.path,
            &w.ctx,
            &range(&s),
            FixAuthorMode::RewriteUnpushed,
        )
        .unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::InvalidInput), "{err}");
        assert!(err.to_string().contains("already on a remote branch"));
        assert_eq!(w.head(), before);
        assert!(list_backups(&w.git, &w.path).unwrap().is_empty());
        // The last commit alone is still local → amend is allowed.
        fix_authors(
            &w.git,
            &w.path,
            &w.ctx,
            &range(&s),
            FixAuthorMode::AmendLast,
        )
        .unwrap();

        // Nothing unpushed / dirty tree / other branch.
        w.git(&["push", "-q", "-f", "origin", "main"]);
        let err = fix_authors(
            &w.git,
            &w.path,
            &w.ctx,
            &range(&s),
            FixAuthorMode::AmendLast,
        )
        .unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NothingToCommit));
        commit_as(w, &personal, "c.txt", "c");
        w.write("c.txt", "dirty\n");
        let err = fix_authors(
            &w.git,
            &w.path,
            &w.ctx,
            &range(&s),
            FixAuthorMode::AmendLast,
        )
        .unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::LocalChanges));
        w.git(&["checkout", "--", "c.txt"]);
        w.git(&["branch", "side"]);
        let side = PushRange::resolve(&w.git, &w.path, "origin", "side").unwrap();
        let err =
            fix_authors(&w.git, &w.path, &w.ctx, &side, FixAuthorMode::AmendLast).unwrap_err();
        assert!(err.to_string().contains("check out"));
    }

    #[test]
    fn lfs_migrate_import_with_backup_and_undo() {
        let s = setup();
        let w = &s.work;
        if !lfs_available(&w.git, &w.path) {
            eprintln!("git-lfs not installed; skipping");
            return;
        }
        let pushed = w.head();
        std::fs::write(w.path.join("big.bin"), pseudo_random(2 * MIB, 11)).unwrap();
        w.git(&["add", "big.bin"]);
        w.git(&["commit", "-q", "-m", "add big"]);
        w.commit("note.txt", "n\n", "note");
        let before = w.head();
        w.git(&["branch", "keep-me"]);
        let r = range(&s);
        let out = lfs_migrate_import(&w.git, &w.path, &w.ctx, &r, &["big.bin".to_owned()]).unwrap();
        assert_eq!(out.rewritten, 2);
        assert_eq!(w.git(&["rev-parse", &out.backup_ref]), before);
        assert_eq!(w.git(&["rev-parse", "keep-me"]), before);
        assert_ne!(out.new_head, before);
        // Pushed history untouched, the blob is now a pointer, the file on disk is intact.
        assert_eq!(w.git(&["rev-parse", "HEAD~2"]), pushed);
        let size: u64 = w.git(&["cat-file", "-s", "HEAD:big.bin"]).parse().unwrap();
        assert!(size < 1024, "still {size} bytes");
        assert!(
            w.git(&["show", "HEAD:.gitattributes"])
                .contains("big.bin filter=lfs")
        );
        assert_eq!(
            std::fs::metadata(w.path.join("big.bin")).unwrap().len(),
            2 * MIB as u64
        );
        assert!(
            heavy_blobs(&w.git, &w.path, &r, MIB as u64)
                .unwrap()
                .is_empty()
        );

        let undo = restore_backup(&w.git, &w.path, &out.backup_ref, Some("main")).unwrap();
        assert_eq!(undo.restored, before);
        assert_eq!(w.head(), before);
        assert_eq!(
            heavy_blobs(&w.git, &w.path, &r, MIB as u64).unwrap().len(),
            1
        );

        // Refused when an unpushed commit is on another remote branch.
        w.git(&["push", "-q", "origin", "HEAD~1:refs/heads/shared"]);
        let err =
            lfs_migrate_import(&w.git, &w.path, &w.ctx, &r, &["big.bin".to_owned()]).unwrap_err();
        assert!(
            err.to_string().contains("already on a remote branch"),
            "{err}"
        );
        assert_eq!(w.head(), before);
    }
}
