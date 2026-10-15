//! Undo / redo of recent operations, driven by the HEAD reflog plus a small journal of our own.
//!
//! # Design
//! The journal lives in the repository's (per-worktree) git dir: `.git/nanomgg/undo.json`.
//! - It survives restarts, follows the repository when it is moved, is never pushed and never
//!   seen by `git status`; deleting the repository deletes it. Linked worktrees each have their
//!   own (their HEAD reflog is their own too).
//! - **Source of truth is the HEAD reflog** (`logs/HEAD`). Every time the journal is read, new
//!   reflog lines are turned into entries: commit, amend, reset, checkout / switch, merge,
//!   rebase (one entry for the whole `(start)`…`(finish)` group; aborted groups vanish),
//!   cherry-pick, revert, pull. That covers operations made from a terminal or another tool too.
//!   A cursor (byte offset + last line) remembers what was read; if the reflog is rewritten
//!   (`git reflog expire`) the cursor is reset and nothing old is guessed.
//! - Operations the reflog of HEAD does not show — creating, deleting and renaming branches,
//!   fast-forwarding a branch that is not checked out — are recorded by the app through
//!   [`record`], which also keeps the SHA (and upstream) of a deleted branch so it can be
//!   brought back.
//!
//! An entry stores where HEAD was before / after (branch + commit) and the local refs it
//! changed. **Undo** checks that the repository is still exactly in the "after" state, then goes
//! back: commit / amend with `git reset --soft` (the changes come back staged), other moves of
//! the current branch with `git reset --keep` (refuses instead of overwriting uncommitted
//! changes), checkouts with `git switch`, refs with `git update-ref` (old value checked).
//! **Redo** is offered only while the repository is exactly in the "before" state of the entry
//! that was undone; any new operation drops the redo entries. Our own reflog lines are written
//! with `GIT_REFLOG_ACTION=nanomgg-undo|nanomgg-redo` and skipped when reading.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::ops::{
    FailureKind, OpError, classify, current_branch, current_operation, git_dir, head_oid,
    ref_exists, writer,
};
use crate::{AccountContext, GitRunner};

const JOURNAL_DIR: &str = "nanomgg";
const JOURNAL_FILE: &str = "undo.json";
const MAX_ENTRIES: usize = 100;
/// Reflog lines imported the first time a repository is seen.
const IMPORT_LINES: usize = 20;
const ACTION_UNDO: &str = "nanomgg-undo";
const ACTION_REDO: &str = "nanomgg-redo";

static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn none() -> AccountContext {
    AccountContext::default()
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Where HEAD points: a branch (and its commit) or a detached commit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct HeadPos {
    pub branch: Option<String>,
    pub oid: Option<String>,
}

/// What an entry did, for the tooltip ("Undo: commit 'abc'").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UndoKind {
    Commit { subject: String },
    Amend { subject: String },
    Reset { target: String },
    Checkout { from: String, to: String },
    Merge { rev: String },
    Rebase { branch: String },
    CherryPick { subject: String },
    Revert { subject: String },
    Pull { detail: String },
    CreateBranch { name: String },
    DeleteBranch { name: String },
    RenameBranch { from: String, to: String },
    FastForward { branch: String, source: String },
    Other { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Upstream {
    remote: String,
    merge: String,
}

/// A local branch changed by an entry (`None` = absent).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RefChange {
    name: String,
    before: Option<String>,
    after: Option<String>,
    #[serde(default)]
    upstream: Option<Upstream>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    id: u64,
    time: i64,
    kind: UndoKind,
    before: HeadPos,
    after: HeadPos,
    #[serde(default)]
    refs: Vec<RefChange>,
    /// Undo with `reset --soft` (commit / amend: the changes come back staged).
    #[serde(default)]
    soft: bool,
    #[serde(default)]
    undone: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Cursor {
    len: u64,
    tail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Journal {
    version: u32,
    cursor: Option<Cursor>,
    next_id: u64,
    entries: Vec<Entry>,
}

/// Why the next undo / redo cannot run right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum UndoBlock {
    /// The repository changed since (another branch moved, HEAD elsewhere…).
    StateChanged,
    /// A merge / rebase / cherry-pick… is in progress.
    OperationInProgress,
    /// Nothing to go back to (e.g. the very first commit).
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct UndoStep {
    pub id: u64,
    /// What the step does.
    pub op: UndoKind,
    /// Unix seconds of the original operation.
    pub time: i64,
    /// Undo would take away commits that are already on a remote: only the local branch
    /// changes, the remote keeps them.
    pub pushed: bool,
    pub blocked: Option<UndoBlock>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct UndoState {
    pub undo: Option<UndoStep>,
    pub redo: Option<UndoStep>,
}

// ----- journal file --------------------------------------------------------------------------

fn journal_path(dir: &Path) -> PathBuf {
    dir.join(JOURNAL_DIR).join(JOURNAL_FILE)
}

fn load(dir: &Path) -> Journal {
    std::fs::read(journal_path(dir))
        .ok()
        .and_then(|b| serde_json::from_slice::<Journal>(&b).ok())
        .unwrap_or_default()
}

fn save(dir: &Path, j: &Journal) -> Result<(), OpError> {
    let path = journal_path(dir);
    let parent = path.parent().expect("journal has a parent");
    let io = |e: std::io::Error| OpError::Git(e.into());
    std::fs::create_dir_all(parent).map_err(io)?;
    let tmp = parent.join(format!("{JOURNAL_FILE}.tmp"));
    let data = serde_json::to_vec(j).map_err(|e| OpError::Git(std::io::Error::other(e).into()))?;
    std::fs::write(&tmp, data).map_err(io)?;
    std::fs::rename(&tmp, &path).map_err(io)
}

fn push_entry(j: &mut Journal, mut e: Entry) {
    j.entries.retain(|e| !e.undone);
    j.next_id += 1;
    e.id = j.next_id;
    j.entries.push(e);
    if j.entries.len() > MAX_ENTRIES {
        let drop = j.entries.len() - MAX_ENTRIES;
        j.entries.drain(..drop);
    }
}

// ----- reflog --------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct Line {
    old: String,
    new: String,
    time: i64,
    msg: String,
    start: usize,
    end: usize,
}

fn is_zero(oid: &str) -> bool {
    !oid.is_empty() && oid.bytes().all(|b| b == b'0')
}

/// Parses `<old> <new> <name> <<email>> <ts> <tz>\t<msg>` lines from `start`.
fn parse_lines(data: &[u8], start: usize) -> Vec<Line> {
    let mut out = Vec::new();
    let mut pos = start;
    while pos < data.len() {
        let Some(nl) = data[pos..].iter().position(|&b| b == b'\n') else {
            break; // incomplete last line: read it next time
        };
        let end = pos + nl + 1;
        let text = String::from_utf8_lossy(&data[pos..end - 1]);
        let (head, msg) = text.split_once('\t').unwrap_or((&text, ""));
        let tokens: Vec<&str> = head.split_whitespace().collect();
        if tokens.len() >= 4 {
            out.push(Line {
                old: tokens[0].to_owned(),
                new: tokens[1].to_owned(),
                time: tokens[tokens.len() - 2].parse().unwrap_or(0),
                msg: msg.trim_end_matches('\r').to_owned(),
                start: pos,
                end,
            });
        }
        pos = end;
    }
    out
}

/// The line that ends right before `end` (without its newline).
fn line_before(data: &[u8], end: usize) -> String {
    if end == 0 {
        return String::new();
    }
    let body = &data[..end - 1];
    let start = body.iter().rposition(|&b| b == b'\n').map_or(0, |p| p + 1);
    String::from_utf8_lossy(&body[start..]).into_owned()
}

fn cursor_at(data: &[u8], offset: usize) -> Cursor {
    Cursor {
        len: offset as u64,
        tail: line_before(data, offset),
    }
}

fn cursor_valid(data: &[u8], c: &Cursor) -> bool {
    let len = c.len as usize;
    len == 0 || (len <= data.len() && data[len - 1] == b'\n' && line_before(data, len) == c.tail)
}

/// Offset where the last `n` complete lines start.
fn last_lines_start(data: &[u8], n: usize) -> usize {
    let mut end = data.len();
    // Ignore an incomplete trailing line.
    while end > 0 && data[end - 1] != b'\n' {
        end -= 1;
    }
    let mut count = 0;
    let mut i = end;
    while i > 0 {
        i -= 1;
        if i > 0 && data[i - 1] == b'\n' {
            count += 1;
            if count == n {
                return i;
            }
        }
    }
    0
}

/// `X (start): …` / `X (finish): …` / `X (abort): …` → `(X, marker)`.
fn group_marker(msg: &str) -> Option<(&str, &str)> {
    for marker in ["start", "finish", "abort"] {
        let token = format!(" ({marker}):");
        if let Some(p) = msg.find(&token) {
            let prefix = &msg[..p];
            if !prefix.is_empty() && !prefix.contains(':') {
                return Some((prefix, marker));
            }
        }
    }
    None
}

/// `checkout: moving from A to B` (also `switch:`) → `(A, B)`.
fn checkout_move(msg: &str) -> Option<(&str, &str)> {
    let rest = msg
        .strip_prefix("checkout: moving from ")
        .or_else(|| msg.strip_prefix("switch: moving from "))?;
    rest.split_once(" to ")
}

/// `Branch: renamed refs/heads/a to refs/heads/b` → `(a, b)`.
fn rename_move(msg: &str) -> Option<(&str, &str)> {
    let rest = msg.strip_prefix("Branch: renamed ")?;
    let (a, b) = rest.split_once(" to ")?;
    Some((
        a.strip_prefix("refs/heads/").unwrap_or(a),
        b.strip_prefix("refs/heads/").unwrap_or(b),
    ))
}

#[derive(Debug)]
enum Event {
    Single(Line),
    Group {
        prefix: String,
        first: Line,
        last: Line,
        aborted: bool,
        branch: Option<String>,
    },
}

/// Groups lines into events; stops before a group that has not finished yet.
fn group_events(lines: Vec<Line>) -> (Vec<Event>, Option<usize>) {
    let mut events = Vec::new();
    let mut it = lines.into_iter().peekable();
    while let Some(line) = it.next() {
        match group_marker(&line.msg) {
            Some((prefix, "start")) => {
                let prefix = prefix.to_owned();
                let first = line;
                let mut closing = None;
                for next in it.by_ref() {
                    if let Some((_, m @ ("finish" | "abort"))) = group_marker(&next.msg) {
                        let aborted = m == "abort";
                        closing = Some((next, aborted));
                        break;
                    }
                }
                let Some((last, aborted)) = closing else {
                    return (events, Some(first.start));
                };
                let branch = last
                    .msg
                    .split_once("returning to ")
                    .and_then(|(_, r)| r.trim().strip_prefix("refs/heads/"))
                    .map(str::to_owned);
                events.push(Event::Group {
                    prefix,
                    first,
                    last,
                    aborted,
                    branch,
                });
            }
            _ => events.push(Event::Single(line)),
        }
    }
    (events, None)
}

fn short(oid: &str) -> String {
    oid.chars().take(7).collect()
}

fn label(pos: &HeadPos) -> String {
    pos.branch
        .clone()
        .unwrap_or_else(|| pos.oid.as_deref().map(short).unwrap_or_default())
}

/// `(kind, soft)` of a single reflog line (not a checkout / rename / group).
fn classify_line(msg: &str) -> (UndoKind, bool) {
    let after_colon = |m: &str| m.split_once(": ").map_or("", |(_, s)| s).to_owned();
    if msg.starts_with("commit (amend):") {
        (
            UndoKind::Amend {
                subject: after_colon(msg),
            },
            true,
        )
    } else if msg.starts_with("commit (merge):") {
        (
            UndoKind::Merge {
                rev: after_colon(msg),
            },
            false,
        )
    } else if msg.starts_with("commit") {
        (
            UndoKind::Commit {
                subject: after_colon(msg),
            },
            true,
        )
    } else if let Some(target) = msg.strip_prefix("reset: moving to ") {
        (
            UndoKind::Reset {
                target: target.to_owned(),
            },
            false,
        )
    } else if let Some(rest) = msg.strip_prefix("merge ") {
        let rev = rest.split_once(':').map_or(rest, |(r, _)| r).to_owned();
        (UndoKind::Merge { rev }, false)
    } else if msg.starts_with("pull") {
        (
            UndoKind::Pull {
                detail: after_colon(msg),
            },
            false,
        )
    } else if msg.starts_with("cherry-pick") {
        (
            UndoKind::CherryPick {
                subject: after_colon(msg),
            },
            false,
        )
    } else if msg.starts_with("revert") {
        (
            UndoKind::Revert {
                subject: after_colon(msg),
            },
            false,
        )
    } else {
        (
            UndoKind::Other {
                message: msg.to_owned(),
            },
            false,
        )
    }
}

fn current_pos(git: &GitRunner, repo: &Path) -> HeadPos {
    HeadPos {
        branch: current_branch(git, repo),
        oid: head_oid(git, repo),
    }
}

/// Branch a rebase in progress will return to.
fn rebase_head_name(dir: &Path) -> Option<String> {
    ["rebase-merge", "rebase-apply"].iter().find_map(|d| {
        let s = std::fs::read_to_string(dir.join(d).join("head-name")).ok()?;
        s.trim().strip_prefix("refs/heads/").map(str::to_owned)
    })
}

/// Where `branch` was before the rebase that finished at `new` (from the branch's reflog).
fn branch_tip_before(git: &GitRunner, repo: &Path, branch: &str, new: &str) -> Option<String> {
    let out = git
        .run_text(
            repo,
            &none(),
            [
                "log",
                "-g",
                "-n",
                "50",
                "--format=%H%x1f%gs",
                &format!("refs/heads/{branch}"),
                "--",
            ],
        )
        .ok()?;
    let rows: Vec<(&str, &str)> = out.lines().filter_map(|l| l.split_once('\x1f')).collect();
    let i = rows
        .iter()
        .position(|(oid, gs)| *oid == new && gs.contains("(finish)"))?;
    rows.get(i + 1).map(|(oid, _)| (*oid).to_owned())
}

/// Turns new HEAD reflog lines into entries.
fn sync(git: &GitRunner, repo: &Path, dir: &Path, j: &mut Journal) {
    let data = std::fs::read(dir.join("logs").join("HEAD")).unwrap_or_default();
    let start = match &j.cursor {
        Some(c) if cursor_valid(&data, c) => c.len as usize,
        Some(_) => {
            // Rewritten / expired reflog: forget the position, do not guess.
            let end = data.iter().rposition(|&b| b == b'\n').map_or(0, |p| p + 1);
            j.cursor = Some(cursor_at(&data, end));
            return;
        }
        None => last_lines_start(&data, IMPORT_LINES),
    };
    let lines = parse_lines(&data, start);
    let consumed_end = lines.last().map_or(start, |l| l.end);
    let (events, pending) = group_events(lines);
    let end = pending.unwrap_or(consumed_end);
    j.cursor = Some(cursor_at(&data, end));
    if events.is_empty() {
        return;
    }

    // Walk backwards to know which branch HEAD was on around each event.
    let mut branch = if pending.is_some() {
        rebase_head_name(dir)
    } else {
        current_branch(git, repo)
    };
    let mut around = vec![(None::<String>, None::<String>); events.len()];
    for (i, ev) in events.iter().enumerate().rev() {
        let after = branch.clone();
        match ev {
            Event::Single(l) => {
                if let Some((from, _)) = checkout_move(&l.msg) {
                    let detached = from.len() >= 40
                        && from.bytes().all(|b| b.is_ascii_hexdigit())
                        && from.eq_ignore_ascii_case(&l.old);
                    branch = (!detached).then(|| from.to_owned());
                } else if let Some((from, to)) = rename_move(&l.msg)
                    && branch.as_deref() == Some(to)
                {
                    branch = Some(from.to_owned());
                }
            }
            Event::Group { branch: b, .. } => {
                if b.is_some() {
                    branch = b.clone();
                }
            }
        }
        around[i] = (branch.clone(), after);
    }

    let mut fresh = Vec::new();
    for (ev, (b_before, b_after)) in events.into_iter().zip(around) {
        match ev {
            Event::Single(l) => {
                if l.msg.starts_with("nanomgg-") || rename_move(&l.msg).is_some() {
                    continue;
                }
                let before = HeadPos {
                    branch: b_before,
                    oid: Some(l.old.clone()).filter(|o| !is_zero(o)),
                };
                let after = HeadPos {
                    branch: b_after,
                    oid: Some(l.new.clone()).filter(|o| !is_zero(o)),
                };
                if before == after || before.oid.is_none() {
                    continue;
                }
                let (kind, soft) = if checkout_move(&l.msg).is_some() {
                    (
                        UndoKind::Checkout {
                            from: label(&before),
                            to: label(&after),
                        },
                        false,
                    )
                } else {
                    classify_line(&l.msg)
                };
                fresh.push(Entry {
                    id: 0,
                    time: l.time,
                    kind,
                    before,
                    after,
                    refs: Vec::new(),
                    soft,
                    undone: false,
                });
            }
            Event::Group {
                prefix,
                first,
                last,
                aborted,
                branch,
            } => {
                if aborted || is_zero(&first.old) {
                    continue;
                }
                // `git rebase <onto> <branch>` from another branch starts at that other
                // branch's tip: the branch's own reflog knows where it really was.
                let original = branch
                    .as_deref()
                    .and_then(|b| branch_tip_before(git, repo, b, &last.new))
                    .unwrap_or_else(|| first.old.clone());
                let first = Line {
                    old: original,
                    ..first
                };
                if first.old == last.new {
                    continue;
                }
                let name = branch.clone().unwrap_or_else(|| short(&last.new));
                let kind = if prefix.starts_with("pull") {
                    UndoKind::Pull { detail: prefix }
                } else {
                    UndoKind::Rebase { branch: name }
                };
                fresh.push(Entry {
                    id: 0,
                    time: last.time,
                    kind,
                    before: HeadPos {
                        branch: branch.clone(),
                        oid: Some(first.old),
                    },
                    after: HeadPos {
                        branch,
                        oid: Some(last.new),
                    },
                    refs: Vec::new(),
                    soft: false,
                    undone: false,
                });
            }
        }
    }
    for e in fresh {
        push_entry(j, e);
    }
}

/// Moves the cursor past everything in the reflog (lines written by our own operation).
fn absorb(dir: &Path, j: &mut Journal) {
    let data = std::fs::read(dir.join("logs").join("HEAD")).unwrap_or_default();
    let end = data.iter().rposition(|&b| b == b'\n').map_or(0, |p| p + 1);
    j.cursor = Some(cursor_at(&data, end));
}

// ----- checks & apply ------------------------------------------------------------------------

fn ref_value(git: &GitRunner, repo: &Path, name: &str) -> Option<String> {
    let full = format!("refs/heads/{name}");
    if !ref_exists(git, repo, &full) {
        return None;
    }
    git.run_text(repo, &none(), ["rev-parse", "--verify", &full])
        .ok()
}

fn upstream_of(git: &GitRunner, repo: &Path, name: &str) -> Option<Upstream> {
    let get = |key: &str| {
        git.run_text(
            repo,
            &none(),
            ["config", "--get", &format!("branch.{name}.{key}")],
        )
        .ok()
        .filter(|v| !v.is_empty())
    };
    Some(Upstream {
        remote: get("remote")?,
        merge: get("merge")?,
    })
}

fn moves_head(e: &Entry) -> bool {
    e.before != e.after && !matches!(e.kind, UndoKind::RenameBranch { .. })
}

/// Why entry `e` cannot be undone (`forward = false`) or redone (`forward = true`) now.
fn blocked(git: &GitRunner, repo: &Path, e: &Entry, forward: bool) -> Option<UndoBlock> {
    if current_operation(git, repo).ok().flatten().is_some() {
        return Some(UndoBlock::OperationInProgress);
    }
    let (expected, target) = if forward {
        (&e.before, &e.after)
    } else {
        (&e.after, &e.before)
    };
    if moves_head(e) {
        if target.oid.is_none() || expected.oid.is_none() {
            return Some(UndoBlock::Unsupported);
        }
        if current_pos(git, repo) != *expected {
            return Some(UndoBlock::StateChanged);
        }
        if forward
            && e.soft
            && git
                .run(
                    repo,
                    &none(),
                    [
                        "diff",
                        "--cached",
                        "--quiet",
                        e.after.oid.as_deref().unwrap_or("HEAD"),
                        "--",
                    ],
                )
                .is_err()
        {
            // Redo of a commit: the staged changes must still be exactly that commit.
            return Some(UndoBlock::StateChanged);
        }
    }
    if let UndoKind::RenameBranch { from, to } = &e.kind {
        let (now, then) = if forward { (from, to) } else { (to, from) };
        if ref_value(git, repo, now).is_none() || ref_value(git, repo, then).is_some() {
            return Some(UndoBlock::StateChanged);
        }
    }
    for r in &e.refs {
        let want = if forward { &r.before } else { &r.after };
        if ref_value(git, repo, &r.name) != *want {
            return Some(UndoBlock::StateChanged);
        }
    }
    None
}

/// Undo would remove commits from the branch that are already on a remote.
fn removes_pushed(git: &GitRunner, repo: &Path, e: &Entry) -> bool {
    let mut ranges: Vec<(String, String)> = Vec::new();
    if moves_head(e)
        && e.before.branch == e.after.branch
        && let (Some(b), Some(a)) = (&e.before.oid, &e.after.oid)
    {
        ranges.push((b.clone(), a.clone()));
    }
    for r in &e.refs {
        if let (Some(b), Some(a)) = (&r.before, &r.after) {
            ranges.push((b.clone(), a.clone()));
        }
    }
    ranges.iter().any(|(before, after)| {
        let exclude = format!("^{before}");
        let count = |extra: &[&str]| {
            let mut args = vec!["rev-list", "--count", after.as_str(), exclude.as_str()];
            args.extend_from_slice(extra);
            args.push("--");
            git.run_text(repo, &none(), &args)
                .ok()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0)
        };
        let all = count(&[]);
        all > 0 && count(&["--not", "--remotes"]) < all
    })
}

fn step_of(git: &GitRunner, repo: &Path, e: &Entry, forward: bool) -> UndoStep {
    UndoStep {
        id: e.id,
        op: e.kind.clone(),
        time: e.time,
        pushed: !forward && removes_pushed(git, repo, e),
        blocked: blocked(git, repo, e, forward),
    }
}

fn apply(git: &GitRunner, repo: &Path, e: &Entry, forward: bool) -> Result<(), OpError> {
    let action = if forward { ACTION_REDO } else { ACTION_UNDO };
    let w = writer(git).with_env("GIT_REFLOG_ACTION", action);
    let run = |args: &[&str]| w.run(repo, &none(), args).map(|_| ()).map_err(classify);
    let target = if forward { &e.after } else { &e.before };

    let head_step = || -> Result<(), OpError> {
        if !moves_head(e) {
            return Ok(());
        }
        let oid = target.oid.as_deref().unwrap_or_default();
        if e.before.branch != e.after.branch {
            match &target.branch {
                Some(b) => run(&["switch", "--no-guess", "--end-of-options", b]),
                None => run(&["switch", "--detach", "--end-of-options", oid]),
            }
        } else {
            let mode = if e.soft { "--soft" } else { "--keep" };
            run(&["reset", "-q", mode, oid, "--"])
        }
    };
    let rename_step = || -> Result<(), OpError> {
        if let UndoKind::RenameBranch { from, to } = &e.kind {
            let (now, then) = if forward { (from, to) } else { (to, from) };
            run(&["branch", "-m", "--end-of-options", now, then])?;
        }
        Ok(())
    };
    let refs_step = || -> Result<(), OpError> {
        let message = format!("{action}: {}", entry_title(&e.kind));
        let list: Vec<&RefChange> = if forward {
            e.refs.iter().collect()
        } else {
            e.refs.iter().rev().collect()
        };
        for r in list {
            let (expected, wanted) = if forward {
                (&r.before, &r.after)
            } else {
                (&r.after, &r.before)
            };
            let full = format!("refs/heads/{}", r.name);
            match (expected, wanted) {
                (Some(old), None) => {
                    if current_branch(git, repo).as_deref() == Some(r.name.as_str()) {
                        return Err(OpError::rejected(
                            FailureKind::CurrentBranch,
                            format!(
                                "'{}' is checked out; switch to another branch first",
                                r.name
                            ),
                        ));
                    }
                    git.run(
                        repo,
                        &none(),
                        ["update-ref", "-m", &message, "-d", &full, old],
                    )
                    .map_err(classify)?;
                }
                (None, Some(new)) => {
                    git.run(
                        repo,
                        &none(),
                        ["update-ref", "-m", &message, &full, new, ""],
                    )
                    .map_err(classify)?;
                    if let Some(up) = &r.upstream {
                        let _ = git.run(
                            repo,
                            &none(),
                            ["config", &format!("branch.{}.remote", r.name), &up.remote],
                        );
                        let _ = git.run(
                            repo,
                            &none(),
                            ["config", &format!("branch.{}.merge", r.name), &up.merge],
                        );
                    }
                }
                (Some(old), Some(new)) => {
                    git.run(
                        repo,
                        &none(),
                        ["update-ref", "-m", &message, &full, new, old],
                    )
                    .map_err(classify)?;
                }
                (None, None) => {}
            }
        }
        Ok(())
    };
    if forward {
        refs_step()?;
        rename_step()?;
        head_step()
    } else {
        head_step()?;
        rename_step()?;
        refs_step()
    }
}

/// Short English description (reflog messages of our ref updates).
fn entry_title(kind: &UndoKind) -> String {
    match kind {
        UndoKind::CreateBranch { name } => format!("create branch {name}"),
        UndoKind::DeleteBranch { name } => format!("delete branch {name}"),
        UndoKind::RenameBranch { from, to } => format!("rename {from} to {to}"),
        UndoKind::FastForward { branch, source } => format!("fast-forward {branch} to {source}"),
        other => format!("{other:?}"),
    }
}

// ----- public API ----------------------------------------------------------------------------

fn top_done(j: &Journal) -> Option<usize> {
    j.entries.iter().rposition(|e| !e.undone)
}

fn first_undone(j: &Journal) -> Option<usize> {
    j.entries.iter().position(|e| e.undone)
}

fn state_of(git: &GitRunner, repo: &Path, j: &Journal) -> UndoState {
    UndoState {
        undo: top_done(j).map(|i| step_of(git, repo, &j.entries[i], false)),
        redo: first_undone(j).map(|i| step_of(git, repo, &j.entries[i], true)),
    }
}

/// What Undo / Redo would do now (reads new reflog lines first).
pub fn state(git: &GitRunner, repo: &Path) -> Result<UndoState, OpError> {
    let _guard = lock();
    let dir = git_dir(git, repo)?;
    let mut j = load(&dir);
    let before = j.clone();
    sync(git, repo, &dir, &mut j);
    if j != before {
        save(&dir, &j)?;
    }
    Ok(state_of(git, repo, &j))
}

/// Result of an undo / redo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct UndoOutcome {
    /// The step that was undone / redone.
    pub step: UndoStep,
    pub head: Option<String>,
    /// What the toolbar offers next.
    pub state: UndoState,
}

fn run_step(git: &GitRunner, repo: &Path, forward: bool) -> Result<UndoOutcome, OpError> {
    let _guard = lock();
    let dir = git_dir(git, repo)?;
    let mut j = load(&dir);
    sync(git, repo, &dir, &mut j);
    let index = if forward {
        first_undone(&j)
    } else {
        top_done(&j)
    };
    let Some(index) = index else {
        save(&dir, &j)?;
        return Err(OpError::rejected(
            FailureKind::NoOperation,
            if forward {
                "nothing to redo"
            } else {
                "nothing to undo"
            },
        ));
    };
    let entry = j.entries[index].clone();
    let step = step_of(git, repo, &entry, forward);
    if let Some(block) = step.blocked {
        save(&dir, &j)?;
        let (kind, message) = match block {
            UndoBlock::OperationInProgress => (
                FailureKind::OperationInProgress,
                "finish or abort the operation in progress first",
            ),
            UndoBlock::StateChanged => (
                FailureKind::InvalidInput,
                "the repository changed since this operation; it can no longer be undone automatically",
            ),
            UndoBlock::Unsupported => {
                (FailureKind::InvalidInput, "this operation cannot be undone")
            }
        };
        return Err(OpError::rejected(kind, message));
    }
    let result = apply(git, repo, &entry, forward);
    absorb(&dir, &mut j);
    if result.is_ok() {
        j.entries[index].undone = !forward;
    }
    save(&dir, &j)?;
    result?;
    Ok(UndoOutcome {
        step,
        head: head_oid(git, repo),
        state: state_of(git, repo, &j),
    })
}

/// Undoes the most recent operation (see the module docs for what is checked).
pub fn undo(git: &GitRunner, repo: &Path) -> Result<UndoOutcome, OpError> {
    run_step(git, repo, false)
}

/// Redoes the operation undone last, if nothing changed since.
pub fn redo(git: &GitRunner, repo: &Path) -> Result<UndoOutcome, OpError> {
    run_step(git, repo, true)
}

/// Runs an app operation that changes local branches (`branches`, short names) and records it
/// as one undoable entry, together with any HEAD move it made (e.g. "create and check out").
/// Recording never makes the operation fail: journal problems are ignored.
pub fn record<T>(
    git: &GitRunner,
    repo: &Path,
    kind: UndoKind,
    branches: &[&str],
    f: impl FnOnce() -> Result<T, OpError>,
) -> Result<T, OpError> {
    let _guard = lock();
    let Ok(dir) = git_dir(git, repo) else {
        return f();
    };
    let mut j = load(&dir);
    sync(git, repo, &dir, &mut j);
    let head_before = current_pos(git, repo);
    let refs_before: Vec<(String, Option<String>, Option<Upstream>)> = branches
        .iter()
        .map(|b| {
            (
                (*b).to_owned(),
                ref_value(git, repo, b),
                upstream_of(git, repo, b),
            )
        })
        .collect();
    let result = f()?;
    let head_after = current_pos(git, repo);
    let refs: Vec<RefChange> = refs_before
        .into_iter()
        .filter_map(|(name, before, upstream)| {
            let after = ref_value(git, repo, &name);
            (after != before).then_some(RefChange {
                name,
                before,
                after,
                upstream,
            })
        })
        .collect();
    let rename = matches!(kind, UndoKind::RenameBranch { ref from, ref to } if from != to);
    if !refs.is_empty() || head_before != head_after || rename {
        push_entry(
            &mut j,
            Entry {
                id: 0,
                time: now_secs(),
                kind,
                before: head_before,
                after: head_after,
                refs,
                soft: false,
                undone: false,
            },
        );
    }
    absorb(&dir, &mut j);
    let _ = save(&dir, &j);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::test_support::TestRepo;

    fn repo() -> TestRepo {
        let r = TestRepo::new();
        r.git(&["config", "core.autocrlf", "false"]);
        r
    }

    fn undo_kind(r: &TestRepo) -> Option<UndoKind> {
        state(&r.git, &r.path).unwrap().undo.map(|s| s.op)
    }

    fn redo_kind(r: &TestRepo) -> Option<UndoKind> {
        state(&r.git, &r.path).unwrap().redo.map(|s| s.op)
    }

    #[test]
    fn parses_reflog_lines_and_markers() {
        let data = b"0000 aaaa T <t@x> 100 +0700\tcommit (initial): first\naaaa bbbb T <t@x> 200 +0700\tcheckout: moving from main to topic\npartial";
        let lines = parse_lines(data, 0);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1].time, 200);
        assert_eq!(lines[1].msg, "checkout: moving from main to topic");
        assert_eq!(checkout_move(&lines[1].msg), Some(("main", "topic")));
        let c = cursor_at(data, lines[1].end);
        assert!(cursor_valid(data, &c));
        assert_eq!(last_lines_start(data, 1), lines[1].start);
        assert_eq!(last_lines_start(data, 5), 0);
        assert_eq!(
            group_marker("rebase (start): checkout main"),
            Some(("rebase", "start"))
        );
        assert_eq!(
            group_marker("pull --rebase (finish): returning to refs/heads/x"),
            Some(("pull --rebase", "finish"))
        );
        assert_eq!(group_marker("commit: fix (start): thing"), None);
        assert_eq!(
            rename_move("Branch: renamed refs/heads/a to refs/heads/b"),
            Some(("a", "b"))
        );
        assert_eq!(
            classify_line("merge topic: Fast-forward").0,
            UndoKind::Merge {
                rev: "topic".into()
            }
        );
        assert!(classify_line("commit: hi").1);
    }

    #[test]
    fn undo_and_redo_commit_keeps_changes_staged() {
        let r = repo();
        let before = r.head();
        r.commit("a.txt", "a\n", "add a");
        let after = r.head();
        assert_eq!(
            undo_kind(&r),
            Some(UndoKind::Commit {
                subject: "add a".into()
            })
        );
        let out = undo(&r.git, &r.path).unwrap();
        assert!(!out.step.pushed);
        assert_eq!(r.head(), before);
        assert_eq!(r.git(&["diff", "--cached", "--name-only"]), "a.txt");
        assert_eq!(r.read("a.txt"), "a\n");
        // The initial commit itself cannot be undone; redo is offered.
        let s = state(&r.git, &r.path).unwrap();
        assert_eq!(
            s.redo.as_ref().map(|s| &s.op),
            Some(&UndoKind::Commit {
                subject: "add a".into()
            })
        );
        redo(&r.git, &r.path).unwrap();
        assert_eq!(r.head(), after);
        assert_eq!(r.git(&["status", "--porcelain"]), "");

        // Redo is dropped as soon as something new happens.
        undo(&r.git, &r.path).unwrap();
        r.git(&["commit", "-q", "-m", "different"]);
        assert_eq!(redo_kind(&r), None);
        assert_eq!(
            undo_kind(&r),
            Some(UndoKind::Commit {
                subject: "different".into()
            })
        );
    }

    #[test]
    fn redo_refused_when_index_changed() {
        let r = repo();
        r.commit("a.txt", "a\n", "add a");
        undo(&r.git, &r.path).unwrap();
        r.write("b.txt", "b\n");
        r.git(&["add", "b.txt"]);
        let s = state(&r.git, &r.path).unwrap();
        assert_eq!(s.redo.unwrap().blocked, Some(UndoBlock::StateChanged));
        assert!(redo(&r.git, &r.path).is_err());
    }

    #[test]
    fn amend_and_reset_undo() {
        let r = repo();
        let first = r.commit("a.txt", "a\n", "add a");
        r.write("a.txt", "a2\n");
        r.git(&["commit", "-q", "-a", "--amend", "-m", "add a (amended)"]);
        assert_eq!(
            undo_kind(&r),
            Some(UndoKind::Amend {
                subject: "add a (amended)".into()
            })
        );
        undo(&r.git, &r.path).unwrap();
        assert_eq!(r.head(), first);
        assert_eq!(r.git(&["log", "-1", "--format=%s"]), "add a");
        assert_eq!(r.read("a.txt"), "a2\n", "amended content kept (staged)");
        assert_eq!(r.git(&["diff", "--cached", "--name-only"]), "a.txt");
        r.git(&["reset", "-q", "--hard"]);

        // reset --hard to an older commit, then undo restores the tip.
        let tip = r.commit(
            "b.txt", "b
", "add b",
        );
        r.git(&["reset", "-q", "--hard", "HEAD~1"]);
        assert!(matches!(undo_kind(&r), Some(UndoKind::Reset { .. })));
        // A file the undo would overwrite: refused, nothing lost.
        r.write(
            "b.txt",
            "precious
",
        );
        let err = undo(&r.git, &r.path).unwrap_err();
        assert!(
            matches!(
                err.kind(),
                Some(FailureKind::LocalChanges | FailureKind::UntrackedFiles)
            ),
            "{err}"
        );
        assert_eq!(
            r.read("b.txt"),
            "precious
"
        );
        std::fs::remove_file(r.path.join("b.txt")).unwrap();
        // An unrelated local change survives the undo.
        r.write(
            "README.md",
            "local edit
",
        );
        undo(&r.git, &r.path).unwrap();
        assert_eq!(r.head(), tip);
        assert_eq!(
            r.read("README.md"),
            "local edit
"
        );
    }

    #[test]
    fn checkout_merge_cherry_pick_revert_rebase() {
        let r = repo();
        r.git(&["switch", "-q", "-c", "topic"]);
        let t1 = r.commit("t.txt", "t\n", "topic 1");
        assert!(matches!(undo_kind(&r), Some(UndoKind::Commit { .. })));
        r.git(&["switch", "-q", "main"]);
        assert_eq!(
            undo_kind(&r),
            Some(UndoKind::Checkout {
                from: "topic".into(),
                to: "main".into()
            })
        );
        undo(&r.git, &r.path).unwrap();
        assert_eq!(r.git(&["symbolic-ref", "--short", "HEAD"]), "topic");
        redo(&r.git, &r.path).unwrap();
        assert_eq!(r.git(&["symbolic-ref", "--short", "HEAD"]), "main");

        let m1 = r.commit("m.txt", "m\n", "main 1");
        r.git(&["merge", "-q", "--no-edit", "topic"]);
        assert_eq!(
            undo_kind(&r),
            Some(UndoKind::Merge {
                rev: "topic".into()
            })
        );
        undo(&r.git, &r.path).unwrap();
        assert_eq!(r.head(), m1);

        r.git(&["cherry-pick", &t1]);
        assert!(matches!(undo_kind(&r), Some(UndoKind::CherryPick { .. })));
        undo(&r.git, &r.path).unwrap();
        assert_eq!(r.head(), m1);

        r.git(&["revert", "--no-edit", "HEAD"]);
        assert!(matches!(undo_kind(&r), Some(UndoKind::Revert { .. })));
        undo(&r.git, &r.path).unwrap();
        assert_eq!(r.head(), m1);

        r.git(&["switch", "-q", "topic"]);
        r.git(&["rebase", "-q", "main"]);
        assert_eq!(
            undo_kind(&r),
            Some(UndoKind::Rebase {
                branch: "topic".into()
            })
        );
        undo(&r.git, &r.path).unwrap();
        assert_eq!(r.head(), t1);
        assert_eq!(r.git(&["symbolic-ref", "--short", "HEAD"]), "topic");
        redo(&r.git, &r.path).unwrap();
        assert_eq!(r.git(&["rev-parse", "HEAD~1"]), m1);

        // An aborted rebase leaves no entry.
        r.git(&["switch", "-q", "main"]);
        r.commit("t.txt", "conflict\n", "main conflicting");
        r.git(&["switch", "-q", "topic"]);
        assert!(!r.try_git(&["rebase", "main"]));
        assert_eq!(
            state(&r.git, &r.path).unwrap().undo.unwrap().blocked,
            Some(UndoBlock::OperationInProgress)
        );
        r.git(&["rebase", "--abort"]);
        assert!(matches!(undo_kind(&r), Some(UndoKind::Checkout { .. })));
    }

    #[test]
    fn rebase_of_another_branch_restores_its_own_tip() {
        let r = repo();
        r.git(&["switch", "-q", "-c", "topic"]);
        let t1 = r.commit("t.txt", "t\n", "topic 1");
        r.git(&["switch", "-q", "main"]);
        r.commit("m.txt", "m\n", "main 1");
        // From main, as a terminal user would type it.
        r.git(&["rebase", "-q", "main", "topic"]);
        assert_eq!(
            undo_kind(&r),
            Some(UndoKind::Rebase {
                branch: "topic".into()
            })
        );
        undo(&r.git, &r.path).unwrap();
        assert_eq!(r.git(&["rev-parse", "topic"]), t1);
        assert_eq!(r.git(&["symbolic-ref", "--short", "HEAD"]), "topic");
    }

    #[test]
    fn records_branch_create_delete_rename() {
        let r = repo();
        r.git(&["remote", "add", "origin", "https://example.invalid/r.git"]);
        let head = r.head();
        // Create + checkout is one entry.
        record(
            &r.git,
            &r.path,
            UndoKind::CreateBranch {
                name: "feat".into(),
            },
            &["feat"],
            || {
                r.git(&["switch", "-q", "-c", "feat"]);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            undo_kind(&r),
            Some(UndoKind::CreateBranch {
                name: "feat".into()
            })
        );
        undo(&r.git, &r.path).unwrap();
        assert_eq!(r.git(&["symbolic-ref", "--short", "HEAD"]), "main");
        assert!(!r.try_git(&["rev-parse", "--verify", "refs/heads/feat"]));
        redo(&r.git, &r.path).unwrap();
        assert_eq!(r.git(&["symbolic-ref", "--short", "HEAD"]), "feat");
        r.git(&["switch", "-q", "main"]);

        // Delete keeps the SHA and the upstream.
        r.git(&["config", "branch.feat.remote", "origin"]);
        r.git(&["config", "branch.feat.merge", "refs/heads/feat"]);
        record(
            &r.git,
            &r.path,
            UndoKind::DeleteBranch {
                name: "feat".into(),
            },
            &["feat"],
            || {
                r.git(&["branch", "-q", "-D", "feat"]);
                Ok(())
            },
        )
        .unwrap();
        assert!(matches!(undo_kind(&r), Some(UndoKind::DeleteBranch { .. })));
        undo(&r.git, &r.path).unwrap();
        assert_eq!(r.git(&["rev-parse", "feat"]), head);
        assert_eq!(r.git(&["config", "branch.feat.remote"]), "origin");

        // Rename (of the current branch).
        r.git(&["switch", "-q", "feat"]);
        record(
            &r.git,
            &r.path,
            UndoKind::RenameBranch {
                from: "feat".into(),
                to: "feature".into(),
            },
            &[],
            || {
                r.git(&["branch", "-m", "feat", "feature"]);
                Ok(())
            },
        )
        .unwrap();
        assert!(matches!(undo_kind(&r), Some(UndoKind::RenameBranch { .. })));
        undo(&r.git, &r.path).unwrap();
        assert_eq!(r.git(&["symbolic-ref", "--short", "HEAD"]), "feat");
        redo(&r.git, &r.path).unwrap();
        assert_eq!(r.git(&["symbolic-ref", "--short", "HEAD"]), "feature");

        // A failing operation records nothing.
        let err = record(
            &r.git,
            &r.path,
            UndoKind::CreateBranch { name: "x".into() },
            &["x"],
            || Err::<(), _>(OpError::rejected(FailureKind::InvalidName, "no")),
        )
        .unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::InvalidName));
        assert!(matches!(undo_kind(&r), Some(UndoKind::RenameBranch { .. })));

        // State changed behind our back: blocked, not guessed.
        r.git(&["switch", "-q", "main"]);
        record(
            &r.git,
            &r.path,
            UndoKind::CreateBranch { name: "y".into() },
            &["y"],
            || {
                r.git(&["branch", "y"]);
                Ok(())
            },
        )
        .unwrap();
        r.commit("z.txt", "z\n", "z");
        r.git(&["branch", "-f", "y", "HEAD"]);
        // The commit is on top; undo it, then the create entry is blocked.
        undo(&r.git, &r.path).unwrap();
        let s = state(&r.git, &r.path).unwrap();
        assert!(matches!(
            s.undo.as_ref().map(|s| &s.op),
            Some(UndoKind::CreateBranch { .. })
        ));
        assert_eq!(s.undo.unwrap().blocked, Some(UndoBlock::StateChanged));
    }

    #[test]
    fn pushed_commits_are_flagged_and_pull_is_undoable() {
        let dir = tempfile::tempdir().unwrap();
        let r = TestRepo::init_at(&dir.path().join("work"));
        r.git(&["config", "core.autocrlf", "false"]);
        r.git
            .run(
                dir.path(),
                &r.ctx,
                ["init", "-q", "--bare", "-b", "main", "remote.git"],
            )
            .unwrap();
        let url = dir
            .path()
            .join("remote.git")
            .to_string_lossy()
            .replace('\\', "/");
        r.git(&["remote", "add", "origin", &url]);
        r.git(&["push", "-q", "-u", "origin", "main"]);
        let base = r.head();
        r.commit("a.txt", "a\n", "pushed a");
        r.git(&["push", "-q"]);
        let s = state(&r.git, &r.path).unwrap();
        assert!(s.undo.unwrap().pushed);
        r.commit("b.txt", "b\n", "local b");
        assert!(!state(&r.git, &r.path).unwrap().undo.unwrap().pushed);

        // Pull (fast-forward) from another clone.
        let other = TestRepo::open(&dir.path().join("other"));
        r.git
            .run(dir.path(), &r.ctx, ["clone", "-q", &url, "other"])
            .unwrap();
        other.commit("c.txt", "c\n", "from other");
        other.git(&["push", "-q"]);
        r.git(&["reset", "-q", "--hard", "HEAD~1"]);
        r.git(&["pull", "-q", "--ff-only"]);
        assert!(matches!(undo_kind(&r), Some(UndoKind::Pull { .. })));
        undo(&r.git, &r.path).unwrap();
        assert_ne!(r.head(), base);
        assert_eq!(r.git(&["log", "-1", "--format=%s"]), "pushed a");
    }

    #[test]
    fn journal_survives_reload_and_reflog_rewrite() {
        let r = repo();
        r.commit("a.txt", "a\n", "add a");
        assert!(undo_kind(&r).is_some());
        let dir = git_dir(&r.git, &r.path).unwrap();
        assert!(journal_path(&dir).exists());
        // Rewritten reflog: cursor reset, nothing new invented, old entries still checked.
        r.git(&["reflog", "expire", "--expire=now", "--all"]);
        r.commit("b.txt", "b\n", "add b");
        let s = state(&r.git, &r.path).unwrap();
        // The previous entry no longer matches HEAD → blocked.
        assert_eq!(s.undo.unwrap().blocked, Some(UndoBlock::StateChanged));
        r.commit("c.txt", "c\n", "add c");
        assert_eq!(
            undo_kind(&r),
            Some(UndoKind::Commit {
                subject: "add c".into()
            })
        );
        // Corrupt journal: starts over.
        std::fs::write(journal_path(&dir), b"{not json").unwrap();
        assert!(state(&r.git, &r.path).is_ok());
    }
}
