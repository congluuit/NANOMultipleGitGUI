//! What the model sees: the staged diff (or the unstaged one on request), cut to a size small
//! models handle, plus the branch name, an issue number found in it and recent subjects.
//!
//! - Lock files, minified bundles, source maps and binary files only appear by name with their
//!   line counts.
//! - The summary of every file comes first; then the head of each file's patch, sharing the
//!   budget fairly (small files whole, big files cut) so one huge file does not hide the rest.
//! - git output is read up to [`READ_CAP`]: a staged 200 MB text dump does not fill the memory.

use std::io::Read;
use std::path::Path;
use std::process::Stdio;

use nanomgg_git_engine::{AccountContext, GitRunner};
use serde::Serialize;
use specta::Type;

use super::AiError;

/// Bytes of patch text sent to the model (the file summary comes on top).
pub const DIFF_BUDGET: usize = 14 * 1024;
/// Most files listed one by one in the summary.
const SUMMARY_FILES: usize = 80;
/// Most bytes read from one git command.
const READ_CAP: usize = 8 * 1024 * 1024;
const RECENT_SUBJECTS: usize = 8;

const LOCK_FILES: &[&str] = &[
    "pnpm-lock.yaml",
    "package-lock.json",
    "npm-shrinkwrap.json",
    "yarn.lock",
    "bun.lockb",
    "bun.lock",
    "cargo.lock",
    "composer.lock",
    "gemfile.lock",
    "poetry.lock",
    "pipfile.lock",
    "uv.lock",
    "go.sum",
    "flake.lock",
    "packages.lock.json",
    "podfile.lock",
    "pubspec.lock",
    "mix.lock",
];

/// Which changes the message describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AiDiffSource {
    Staged,
    /// Nothing was staged and the user asked to use the unstaged changes.
    Unstaged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Omit {
    Binary,
    Generated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStat {
    pub path: String,
    /// `None` for binary files.
    pub added: Option<u64>,
    pub deleted: Option<u64>,
    pub omit: Option<Omit>,
}

#[derive(Debug, Clone)]
pub struct CommitContext {
    pub source: AiDiffSource,
    pub branch: Option<String>,
    pub issue: Option<String>,
    pub recent_subjects: Vec<String>,
    pub files: Vec<FileStat>,
    /// Untracked files (unstaged mode only), listed by name.
    pub untracked: Vec<String>,
    /// File summary + cut patches, ready for the prompt.
    pub diff: String,
    /// Something was left out to fit the budget.
    pub truncated: bool,
}

/// Lock files / minified bundles / source maps: noise for a commit message.
pub fn is_generated(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    LOCK_FILES.contains(&name.as_str())
        || name.ends_with(".min.js")
        || name.ends_with(".min.css")
        || name.ends_with(".min.mjs")
        || name.ends_with(".js.map")
        || name.ends_with(".css.map")
}

/// `git diff --numstat -z`: `A\tD\tpath\0`, renames `A\tD\t\0old\0new\0`, binary `-\t-\t`.
pub fn parse_numstat_z(raw: &str) -> Vec<FileStat> {
    let mut out = Vec::new();
    let mut parts = raw.split('\0');
    while let Some(head) = parts.next() {
        let head = head.trim_start_matches('\n');
        if head.is_empty() {
            continue;
        }
        let mut cols = head.splitn(3, '\t');
        let (Some(a), Some(d), Some(rest)) = (cols.next(), cols.next(), cols.next()) else {
            continue;
        };
        let path = if rest.is_empty() {
            let old = parts.next().unwrap_or_default();
            let new = parts.next().unwrap_or_default();
            format!("{old} → {new}")
        } else {
            rest.to_owned()
        };
        let (added, deleted) = (a.parse().ok(), d.parse().ok());
        let target = path.rsplit(" → ").next().unwrap_or(&path).to_owned();
        let omit = if added.is_none() && a == "-" {
            Some(Omit::Binary)
        } else if is_generated(&target) {
            Some(Omit::Generated)
        } else {
            None
        };
        out.push(FileStat {
            path,
            added,
            deleted,
            omit,
        });
    }
    out
}

/// Splits a patch at its `diff --git` headers (one chunk per file, in order).
pub fn split_patch(patch: &str) -> Vec<&str> {
    let mut starts: Vec<usize> = Vec::new();
    let mut pos = 0;
    for line in patch.split_inclusive('\n') {
        if line.starts_with("diff --git ") || line.starts_with("diff --cc ") {
            starts.push(pos);
        }
        pos += line.len();
    }
    starts
        .iter()
        .enumerate()
        .map(|(i, &s)| &patch[s..starts.get(i + 1).copied().unwrap_or(patch.len())])
        .collect()
}

/// Head of `text` within `max` bytes, cut at a line end (or a char boundary).
fn head(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    match text[..end].rfind('\n') {
        Some(nl) if nl > 0 => &text[..=nl],
        _ => &text[..end],
    }
}

/// Fair share of `budget` between items of `sizes`: small items get all they need, the rest
/// share what is left equally.
pub fn water_fill(sizes: &[usize], budget: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&i| sizes[i]);
    let mut out = vec![0; sizes.len()];
    let mut left = budget;
    for (k, &i) in order.iter().enumerate() {
        let share = left / (sizes.len() - k);
        out[i] = sizes[i].min(share);
        left -= out[i];
    }
    out
}

fn stat_line(f: &FileStat) -> String {
    match (f.added, f.deleted, f.omit) {
        (_, _, Some(Omit::Binary)) | (None, _, _) | (_, None, _) => {
            format!("{} (binary file)", f.path)
        }
        (Some(a), Some(d), Some(Omit::Generated)) => {
            format!("{} (+{a} -{d}, generated/lock file, diff omitted)", f.path)
        }
        (Some(a), Some(d), None) => format!("{} (+{a} -{d})", f.path),
    }
}

/// File summary followed by the patches of the files worth reading, within `budget`.
/// Returns the text and whether anything was cut.
pub fn render_diff(
    files: &[FileStat],
    untracked: &[String],
    patch: &str,
    budget: usize,
) -> (String, bool) {
    let mut truncated = false;
    let mut out = String::new();
    let total = files.len() + untracked.len();
    let (adds, dels) = files.iter().fold((0u64, 0u64), |(a, d), f| {
        (a + f.added.unwrap_or(0), d + f.deleted.unwrap_or(0))
    });
    out.push_str(&format!("Files changed: {total} (+{adds} -{dels} lines)\n"));
    for f in files.iter().take(SUMMARY_FILES) {
        out.push_str("- ");
        out.push_str(&stat_line(f));
        out.push('\n');
    }
    for u in untracked
        .iter()
        .take(SUMMARY_FILES.saturating_sub(files.len()))
    {
        out.push_str(&format!("- {u} (new file, not added yet)\n"));
    }
    if total > SUMMARY_FILES {
        truncated = true;
        out.push_str(&format!("- ... and {} more files\n", total - SUMMARY_FILES));
    }

    // Chunks pair with numstat entries by position (same diff options, same order).
    let chunks = split_patch(patch);
    let readable: Vec<&str> = chunks
        .iter()
        .enumerate()
        .filter(|(i, _)| files.get(*i).is_none_or(|f| f.omit.is_none()))
        .map(|(_, c)| *c)
        .collect();
    if chunks.len() < files.iter().filter(|f| f.omit.is_none()).count() {
        // The patch hit the read cap.
        truncated = true;
    }
    if readable.is_empty() {
        return (out, truncated);
    }
    out.push_str("\nDiff:\n");
    let sizes: Vec<usize> = readable.iter().map(|c| c.len()).collect();
    let shares = water_fill(&sizes, budget);
    for (chunk, share) in readable.iter().zip(shares) {
        if share < 120 {
            // Not even the header fits; the file is still in the summary.
            truncated = true;
            continue;
        }
        let cut = head(chunk, share);
        out.push_str(cut);
        if cut.len() < chunk.len() {
            truncated = true;
            if !cut.ends_with('\n') {
                out.push('\n');
            }
            let lines = chunk[cut.len()..].lines().count();
            out.push_str(&format!("[... {lines} more lines of this file cut]\n"));
        }
    }
    (out, truncated)
}

/// `#123` / `ABC-123` from a branch like `feature/123-login`, `fix/#45`, `ABC-123-crash`.
pub fn issue_from_branch(branch: &str) -> Option<String> {
    let name = branch.rsplit('/').next().unwrap_or(branch);
    let tokens: Vec<&str> = name.split(['-', '_']).collect();
    // JIRA style: an all-caps word followed by a number.
    for w in tokens.windows(2) {
        let (key, num) = (w[0], w[1]);
        if key.len() >= 2
            && key.len() <= 10
            && key.chars().all(|c| c.is_ascii_uppercase())
            && !num.is_empty()
            && num.len() <= 7
            && num.chars().all(|c| c.is_ascii_digit())
        {
            return Some(format!("{key}-{num}"));
        }
    }
    let is_num = |t: &str| !t.is_empty() && t.len() <= 7 && t.chars().all(|c| c.is_ascii_digit());
    // `#45`, then a bare number token (`123-login`, `issue-12`) but not version-like `v2`.
    let check = |s: &str| {
        s.split(['-', '_', '.', '/'])
            .find_map(|t| t.strip_prefix('#').filter(|n| is_num(n)).map(str::to_owned))
    };
    if let Some(n) = check(branch) {
        return Some(format!("#{n}"));
    }
    tokens
        .iter()
        .find(|t| is_num(t))
        .map(|n| format!("#{n}"))
        .or_else(|| {
            // `fix/123/login`: a numeric path segment.
            branch
                .split('/')
                .find(|s| is_num(s))
                .map(|n| format!("#{n}"))
        })
}

/// Runs `git <args>`, reading at most `cap` bytes of stdout. `None` when git failed.
fn git_capped(
    git: &GitRunner,
    root: &Path,
    args: &[&str],
    cap: usize,
) -> Result<Option<String>, AiError> {
    let mut child = git
        .command(root, &AccountContext::default(), args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| AiError::git(e.to_string()))?;
    let mut buf = Vec::new();
    let mut hit_cap = false;
    if let Some(out) = child.stdout.take() {
        let mut limited = out.take(cap as u64);
        limited
            .read_to_end(&mut buf)
            .map_err(|e| AiError::git(e.to_string()))?;
        hit_cap = buf.len() >= cap;
    }
    if hit_cap {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| AiError::git(e.to_string()))?;
    if !status.success() && !hit_cap {
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&buf).into_owned()))
}

const DIFF_ARGS: &[&str] = &[
    "-c",
    "core.quotePath=false",
    "-c",
    "diff.noprefix=false",
    "diff",
    "--no-ext-diff",
    "--no-textconv",
    "--no-color",
    "-M",
];

/// Staged changes, or (with `unstaged`) the working-tree changes when nothing is staged.
/// Fails with `NothingStaged` when the index matches HEAD.
pub fn collect(git: &GitRunner, root: &Path, unstaged: bool) -> Result<CommitContext, AiError> {
    let numstat = |cached: bool| -> Result<Vec<FileStat>, AiError> {
        let mut args = DIFF_ARGS.to_vec();
        if cached {
            args.push("--cached");
        }
        args.extend(["--numstat", "-z"]);
        let raw = git_capped(git, root, &args, READ_CAP)?
            .ok_or_else(|| AiError::git("git diff failed"))?;
        Ok(parse_numstat_z(&raw))
    };
    let mut files = numstat(true)?;
    let mut source = AiDiffSource::Staged;
    let mut untracked = Vec::new();
    if files.is_empty() {
        let tracked = numstat(false)?;
        untracked = git_capped(
            git,
            root,
            &[
                "-c",
                "core.quotePath=false",
                "ls-files",
                "--others",
                "--exclude-standard",
                "-z",
            ],
            READ_CAP,
        )?
        .unwrap_or_default()
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
        let has_unstaged = !tracked.is_empty() || !untracked.is_empty();
        if !unstaged || !has_unstaged {
            return Err(AiError::NothingStaged {
                message: "nothing is staged".into(),
                has_unstaged,
            });
        }
        files = tracked;
        source = AiDiffSource::Unstaged;
    }
    let mut args = DIFF_ARGS.to_vec();
    if source == AiDiffSource::Staged {
        args.push("--cached");
    }
    let patch = git_capped(git, root, &args, READ_CAP)?.unwrap_or_default();
    let (diff, truncated) = render_diff(&files, &untracked, &patch, DIFF_BUDGET);

    let branch = git_capped(git, root, &["symbolic-ref", "--short", "-q", "HEAD"], 4096)?
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    let recent_subjects = git_capped(
        git,
        root,
        &[
            "log",
            &format!("-n{RECENT_SUBJECTS}"),
            "--no-merges",
            "--format=%s",
        ],
        64 * 1024,
    )?
    .unwrap_or_default()
    .lines()
    .map(|l| l.trim().chars().take(120).collect::<String>())
    .filter(|l| !l.is_empty())
    .collect();
    Ok(CommitContext {
        source,
        issue: branch.as_deref().and_then(issue_from_branch),
        branch,
        recent_subjects,
        files,
        untracked,
        diff,
        truncated,
    })
}
