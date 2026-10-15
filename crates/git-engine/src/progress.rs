//! Progress of long-running git commands (fetch / pull / push): parsing git's `--progress`
//! output, cooperative cancellation and a runner that streams stderr while the process runs.
//!
//! Cancelling kills the whole process tree, including hook children started through MSYS `sh`
//! on Windows (see [`crate::job`]).

use std::io::Read;
use std::path::Path;
use std::process::{Child, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::job::ProcessTree;
use crate::ops::{OpError, classify};
use crate::{AccountContext, GitError, GitRunner};

/// How often the runner checks the cancel flag and the process state.
const POLL: Duration = Duration::from_millis(50);
/// How long to keep draining output after git exited (a grandchild may still hold the pipe).
const GRACE: Duration = Duration::from_secs(2);

/// Stage of a transfer, derived from the label git prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ProgressPhase {
    Enumerating,
    Counting,
    Compressing,
    Receiving,
    Resolving,
    Writing,
    Unpacking,
    /// `Updating files` / `Checking out files`.
    CheckingOut,
    /// `Filtering content` (LFS smudge during checkout).
    Filtering,
    /// `Uploading LFS objects` / `Downloading LFS objects`.
    Lfs,
    Other,
}

/// One parsed progress line, e.g. `Receiving objects:  45% (450/1000), 1.20 MiB | 2.00 MiB/s`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: ProgressPhase,
    /// Label as printed by git, e.g. `Receiving objects`.
    pub label: String,
    /// The line came from the other side (`remote: ` prefix).
    pub remote: bool,
    pub percent: Option<u8>,
    pub current: Option<u64>,
    pub total: Option<u64>,
    /// Transferred size as printed by git, e.g. `1.20 MiB`.
    pub bytes: Option<String>,
    /// Transfer rate as printed by git, e.g. `2.00 MiB/s`.
    pub throughput: Option<String>,
    /// The phase finished (`, done.`).
    pub done: bool,
}

fn phase_of(label: &str) -> ProgressPhase {
    let l = label.to_ascii_lowercase();
    if l.contains("lfs") {
        ProgressPhase::Lfs
    } else if l.starts_with("enumerating") {
        ProgressPhase::Enumerating
    } else if l.starts_with("counting") {
        ProgressPhase::Counting
    } else if l.starts_with("compressing") {
        ProgressPhase::Compressing
    } else if l.starts_with("receiving") {
        ProgressPhase::Receiving
    } else if l.starts_with("resolving") {
        ProgressPhase::Resolving
    } else if l.starts_with("writing") {
        ProgressPhase::Writing
    } else if l.starts_with("unpacking") {
        ProgressPhase::Unpacking
    } else if l.starts_with("updating files") || l.starts_with("checking out") {
        ProgressPhase::CheckingOut
    } else if l.starts_with("filtering") {
        ProgressPhase::Filtering
    } else {
        ProgressPhase::Other
    }
}

/// Parses one progress line (already split on `\r` / `\n`). Returns `None` for anything that
/// is not a progress line (errors, hints, ref updates, `Total ...` summaries).
pub fn parse_progress(line: &str) -> Option<Progress> {
    let line = line.trim_end().trim_end_matches("\x1b[K").trim_end();
    let (remote, line) = match line.strip_prefix("remote:") {
        Some(rest) => (true, rest.trim_start()),
        None => (false, line.trim_start()),
    };
    let (label, rest) = line.split_once(": ")?;
    let label = label.trim();
    // Progress labels are short capitalised phrases; messages like `error: 404` are not.
    if label.is_empty()
        || label.len() > 48
        || !label.starts_with(|c: char| c.is_ascii_uppercase())
        || matches!(label, "From" | "To")
    {
        return None;
    }
    let mut rest = rest.trim();
    let mut done = false;
    for suffix in [", done.", ", done"] {
        if let Some(r) = rest.strip_suffix(suffix) {
            rest = r.trim_end();
            done = true;
            break;
        }
    }
    let (head, tail) = match rest.split_once(", ") {
        Some((h, t)) => (h.trim(), Some(t.trim())),
        None => (rest, None),
    };
    let mut progress = Progress {
        phase: phase_of(label),
        label: label.to_owned(),
        remote,
        percent: None,
        current: None,
        total: None,
        bytes: None,
        throughput: None,
        done,
    };
    let first = head.split_whitespace().next()?;
    if let Some(pct) = first.strip_suffix('%') {
        progress.percent = Some(pct.parse::<u8>().ok()?.min(100));
        if let Some((cur, total)) = head
            .split_once('(')
            .and_then(|(_, r)| r.strip_suffix(')'))
            .and_then(|r| r.split_once('/'))
        {
            progress.current = cur.trim().parse().ok();
            progress.total = total.trim().parse().ok();
        }
    } else {
        // `Enumerating objects: 11` — a plain counter; nothing else may follow it.
        if head.split_whitespace().count() != 1 {
            return None;
        }
        progress.current = Some(first.parse().ok()?);
    }
    if let Some(tail) = tail {
        match tail.split_once(" | ") {
            Some((bytes, rate)) => {
                progress.bytes = Some(bytes.trim().to_owned());
                progress.throughput = Some(rate.trim().to_owned());
            }
            None => progress.bytes = Some(tail.to_owned()),
        }
    }
    Some(progress)
}

/// Shared flag that asks a running command to stop. Cheap to clone; all clones share the flag.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Output of a streamed command.
#[derive(Debug, Clone, Default)]
pub struct StreamOutput {
    pub stdout: String,
    /// Stderr without the progress lines (ref updates, hints, warnings).
    pub stderr: String,
}

fn cancelled(tree: ProcessTree, child: &mut Child) -> OpError {
    tree.kill(child);
    OpError::Cancelled
}

/// Runs `git <args>`, delivering parsed progress lines to `on_progress` while it runs.
///
/// The cancel flag is checked every [`POLL`]; when set, the process tree is killed and
/// [`OpError::Cancelled`] is returned. Failures are classified (see [`classify`]) with the
/// non-progress part of stderr as the message.
pub fn run_streaming<F: FnMut(Progress)>(
    git: &GitRunner,
    cwd: &Path,
    ctx: &AccountContext,
    args: &[String],
    cancel: &CancelToken,
    on_progress: F,
) -> Result<StreamOutput, OpError> {
    run_streaming_with(git, cwd, ctx, args, &[], cancel, on_progress, || {})
}

/// [`run_streaming`] with extra environment for this command (e.g. `GIT_LFS_PROGRESS`) and an
/// `on_tick` callback invoked about every [`POLL`] while the process runs (and once after it
/// exited), so callers can follow side channels such as a progress file.
#[allow(clippy::too_many_arguments)]
pub fn run_streaming_with<F: FnMut(Progress), T: FnMut()>(
    git: &GitRunner,
    cwd: &Path,
    ctx: &AccountContext,
    args: &[String],
    env: &[(String, String)],
    cancel: &CancelToken,
    mut on_progress: F,
    mut on_tick: T,
) -> Result<StreamOutput, OpError> {
    if cancel.is_cancelled() {
        return Err(OpError::Cancelled);
    }
    let mut cmd = git.command(cwd, ctx, args);
    let mut child = ProcessTree::prepare(&mut cmd)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(GitError::from)?;
    // Right after spawn, before git can start subprocesses (see `job` for the race handling).
    let tree = ProcessTree::attach(&child);

    let mut stderr = child.stderr.take().expect("piped stderr");
    let (line_tx, line_rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut pending = Vec::new();
        while let Ok(n) = stderr.read(&mut buf) {
            if n == 0 {
                break;
            }
            for &b in &buf[..n] {
                if b == b'\r' || b == b'\n' {
                    if !pending.is_empty() {
                        let line = String::from_utf8_lossy(&pending).into_owned();
                        pending.clear();
                        if line_tx.send(line).is_err() {
                            return;
                        }
                    }
                } else {
                    pending.push(b);
                }
            }
        }
        if !pending.is_empty() {
            let _ = line_tx.send(String::from_utf8_lossy(&pending).into_owned());
        }
    });
    let mut stdout = child.stdout.take().expect("piped stdout");
    let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        let _ = out_tx.send(buf);
    });

    let mut messages: Vec<String> = Vec::new();
    let mut exited: Option<(ExitStatus, Instant)> = None;
    loop {
        if cancel.is_cancelled() {
            return Err(cancelled(tree, &mut child));
        }
        match line_rx.recv_timeout(POLL) {
            Ok(line) => match parse_progress(&line) {
                Some(p) => on_progress(p),
                None => {
                    let text = line.trim_end().trim_end_matches("\x1b[K").trim_end();
                    if !text.trim().is_empty() {
                        messages.push(text.to_owned());
                    }
                }
            },
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        on_tick();
        match exited {
            None => {
                if let Some(status) = child.try_wait().map_err(GitError::from)? {
                    exited = Some((status, Instant::now()));
                }
            }
            Some((_, at)) if at.elapsed() > GRACE => break,
            Some(_) => {}
        }
    }
    let status = loop {
        if let Some((status, _)) = exited {
            break status;
        }
        if cancel.is_cancelled() {
            return Err(cancelled(tree, &mut child));
        }
        match child.try_wait().map_err(GitError::from)? {
            Some(status) => break status,
            None => {
                std::thread::sleep(POLL);
                on_tick();
            }
        }
    };
    tree.release();
    on_tick();
    let stdout = out_rx.recv_timeout(GRACE).unwrap_or_default();
    let output = StreamOutput {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: messages.join("\n"),
    };
    if status.success() {
        Ok(output)
    } else {
        Err(classify(GitError::Failed {
            args: args.join(" "),
            status: status.to_string(),
            stderr: output.stderr,
        }))
    }
}

/// Helpers shared by the tests of the write-operation modules.
#[cfg(test)]
pub(crate) mod test_support {
    use std::path::{Path, PathBuf};

    use crate::{AccountContext, GitRunner, Identity};

    pub fn ctx() -> AccountContext {
        AccountContext {
            identity: Some(Identity {
                name: "Tester".into(),
                email: "tester@example.com".into(),
            }),
            ..Default::default()
        }
    }

    /// A repository on `main` with one commit (`README.md`).
    pub struct TestRepo {
        _dir: Option<tempfile::TempDir>,
        pub path: PathBuf,
        pub git: GitRunner,
        pub ctx: AccountContext,
    }

    impl TestRepo {
        pub fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("repo");
            let mut repo = Self::init_at(&path);
            repo._dir = Some(dir);
            repo
        }

        /// Initialises a repository at `path` (inside a caller-owned temp dir).
        pub fn init_at(path: &Path) -> Self {
            std::fs::create_dir_all(path).unwrap();
            let repo = Self {
                _dir: None,
                path: path.to_owned(),
                git: GitRunner::default(),
                ctx: ctx(),
            };
            repo.git(&["init", "-q", "-b", "main"]);
            repo.commit("README.md", "hello\n", "init");
            repo
        }

        /// Opens an existing clone (no init).
        pub fn open(path: &Path) -> Self {
            Self {
                _dir: None,
                path: path.to_owned(),
                git: GitRunner::default(),
                ctx: ctx(),
            }
        }

        pub fn git(&self, args: &[&str]) -> String {
            self.git.run_text(&self.path, &self.ctx, args).unwrap()
        }

        pub fn try_git(&self, args: &[&str]) -> bool {
            self.git.run(&self.path, &self.ctx, args).is_ok()
        }

        pub fn write(&self, file: &str, content: &str) {
            let p = self.path.join(file);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(p, content).unwrap();
        }

        /// Writes `file`, commits it and returns the new HEAD.
        pub fn commit(&self, file: &str, content: &str, msg: &str) -> String {
            self.write(file, content);
            self.git(&["add", "--", file]);
            self.git(&["commit", "-q", "-m", msg]);
            self.head()
        }

        pub fn head(&self) -> String {
            self.git(&["rev-parse", "HEAD"])
        }

        pub fn branch(&self) -> String {
            self.git(&["branch", "--show-current"])
        }

        /// File content with line endings normalised (the machine may use `core.autocrlf`).
        pub fn read(&self, file: &str) -> String {
            std::fs::read_to_string(self.path.join(file))
                .unwrap()
                .replace("\r\n", "\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_receiving_with_throughput() {
        let p =
            parse_progress("Receiving objects:  45% (450/1000), 1.20 MiB | 2.00 MiB/s").unwrap();
        assert_eq!(p.phase, ProgressPhase::Receiving);
        assert_eq!(p.label, "Receiving objects");
        assert!(!p.remote && !p.done);
        assert_eq!(p.percent, Some(45));
        assert_eq!((p.current, p.total), (Some(450), Some(1000)));
        assert_eq!(p.bytes.as_deref(), Some("1.20 MiB"));
        assert_eq!(p.throughput.as_deref(), Some("2.00 MiB/s"));
    }

    #[test]
    fn parses_done_lines() {
        let p = parse_progress("Resolving deltas: 100% (10/10), done.").unwrap();
        assert_eq!(p.phase, ProgressPhase::Resolving);
        assert_eq!(p.percent, Some(100));
        assert_eq!((p.current, p.total), (Some(10), Some(10)));
        assert!(p.done);
        assert_eq!(p.bytes, None);

        let p =
            parse_progress("Writing objects: 100% (6/6), 520 bytes | 520.00 KiB/s, done.").unwrap();
        assert_eq!(p.phase, ProgressPhase::Writing);
        assert!(p.done);
        assert_eq!(p.bytes.as_deref(), Some("520 bytes"));
        assert_eq!(p.throughput.as_deref(), Some("520.00 KiB/s"));
    }

    #[test]
    fn parses_writing_and_counters() {
        let p = parse_progress("Writing objects:  50% (3/6)").unwrap();
        assert_eq!(p.phase, ProgressPhase::Writing);
        assert_eq!(
            (p.percent, p.current, p.total),
            (Some(50), Some(3), Some(6))
        );

        let p = parse_progress("Enumerating objects: 11, done.").unwrap();
        assert_eq!(p.phase, ProgressPhase::Enumerating);
        assert_eq!((p.percent, p.current, p.total), (None, Some(11), None));
        assert!(p.done);
    }

    #[test]
    fn parses_remote_sideband_lines_with_padding() {
        // Captured from `git fetch --progress` (git pads sideband lines with spaces).
        let p = parse_progress("remote: Counting objects: 100% (1/1)        ").unwrap();
        assert!(p.remote);
        assert_eq!(p.phase, ProgressPhase::Counting);
        assert_eq!(p.percent, Some(100));
        let p = parse_progress("remote: Compressing objects:  33% (1/3)\x1b[K").unwrap();
        assert_eq!(p.phase, ProgressPhase::Compressing);
        assert_eq!((p.current, p.total), (Some(1), Some(3)));
    }

    #[test]
    fn parses_lfs_and_checkout_lines() {
        let p = parse_progress("Uploading LFS objects:  50% (1/2), 5.0 MB | 1.0 MB/s").unwrap();
        assert_eq!(p.phase, ProgressPhase::Lfs);
        assert_eq!(p.throughput.as_deref(), Some("1.0 MB/s"));
        let p = parse_progress("Updating files:  12% (120/1000)").unwrap();
        assert_eq!(p.phase, ProgressPhase::CheckingOut);
        let p =
            parse_progress("Filtering content: 100% (3/3), 1.2 MiB | 600 KiB/s, done.").unwrap();
        assert_eq!(p.phase, ProgressPhase::Filtering);
    }

    #[test]
    fn ignores_non_progress_lines() {
        for line in [
            "remote: Total 1 (delta 0), reused 0 (delta 0), pack-reused 0 (from 0)",
            "From C:/Users/me/AppData/Local/Temp/r",
            "   bf92b8f..cbc67a7  main       -> origin/main",
            "Delta compression using up to 32 threads",
            "fatal: unable to access 'https://example.com/x.git/': Could not resolve host",
            "error: failed to push some refs to 'origin'",
            "hint: Updates were rejected because the tip of your current branch is behind",
            "To C:/tmp/r.git",
            " ! [rejected]        main -> main (non-fast-forward)",
            "Everything up-to-date",
            "",
        ] {
            assert_eq!(parse_progress(line), None, "{line:?}");
        }
    }

    #[test]
    fn cancel_before_start_never_spawns() {
        let token = CancelToken::new();
        token.cancel();
        let dir = tempfile::tempdir().unwrap();
        let err = run_streaming(
            &GitRunner::default(),
            dir.path(),
            &AccountContext::default(),
            &["init".to_owned(), "-q".to_owned()],
            &token,
            |_| {},
        )
        .unwrap_err();
        assert!(matches!(err, OpError::Cancelled));
        assert!(!dir.path().join(".git").exists());
    }

    #[test]
    fn streaming_failure_carries_stderr() {
        let dir = tempfile::tempdir().unwrap();
        let err = run_streaming(
            &GitRunner::default(),
            dir.path(),
            &AccountContext::default(),
            &["rev-parse".to_owned(), "HEAD".to_owned()],
            &CancelToken::new(),
            |_| {},
        )
        .unwrap_err();
        assert!(err.to_string().contains("not a git repository"), "{err}");
    }

    /// Process ids of `sleep.exe` processes whose command line contains `marker`.
    #[cfg(windows)]
    fn sleeping(marker: &str) -> Vec<u32> {
        use std::os::windows::process::CommandExt;
        let script = format!(
            "Get-CimInstance Win32_Process -Filter \"Name='sleep.exe'\" | \
             Where-Object {{ $_.CommandLine -like '*{marker}*' }} | \
             ForEach-Object {{ $_.ProcessId }}"
        );
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(0x0800_0000)
            .output()
            .expect("powershell");
        String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .filter_map(|p| p.parse().ok())
            .collect()
    }

    /// A hook runs `sleep` through MSYS `sh`; cancelling the git command must leave no
    /// `sleep.exe` behind (the old `taskkill /T` missed MSYS children).
    #[cfg(windows)]
    #[test]
    fn cancel_kills_hook_children_started_through_msys_sh() {
        let repo = test_support::TestRepo::new();
        // Unique duration, so we only ever look at our own sleep process.
        let marker = format!("900.{}7", std::process::id());
        repo.write(
            ".git/hooks/pre-commit",
            &format!("#!/bin/sh\necho hook started >&2\nsleep {marker}\n"),
        );
        let token = CancelToken::new();
        let watcher = {
            let token = token.clone();
            let marker = marker.clone();
            std::thread::spawn(move || {
                let started = Instant::now();
                let mut seen = Vec::new();
                while started.elapsed() < Duration::from_secs(60) {
                    seen = sleeping(&marker);
                    if !seen.is_empty() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
                token.cancel();
                seen
            })
        };
        let args: Vec<String> = ["commit", "--allow-empty", "-m", "hooked"]
            .map(String::from)
            .to_vec();
        let started = Instant::now();
        let err =
            run_streaming(&repo.git, &repo.path, &repo.ctx, &args, &token, |_| {}).unwrap_err();
        let seen = watcher.join().unwrap();
        assert!(!seen.is_empty(), "the hook never started sleep {marker}");
        assert!(matches!(err, OpError::Cancelled), "{err}");
        assert!(started.elapsed() < Duration::from_secs(90));
        // Termination is asynchronous; give Windows a moment to tear the job down.
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut left = sleeping(&marker);
        while !left.is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(300));
            left = sleeping(&marker);
        }
        assert!(
            left.is_empty(),
            "sleep {marker} survived the cancel: {left:?}"
        );
        // The commit never happened.
        assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]), "1");
    }

    #[test]
    fn extra_env_and_ticks_reach_the_caller() {
        let repo = test_support::TestRepo::new();
        let args: Vec<String> = ["config", "user.name"].map(String::from).to_vec();
        let mut ticks = 0;
        let out = run_streaming_with(
            &repo.git,
            &repo.path,
            &repo.ctx,
            &args,
            &[(
                "GIT_CONFIG_PARAMETERS".into(),
                "'user.name'='From Env'".into(),
            )],
            &CancelToken::new(),
            |_| {},
            || ticks += 1,
        )
        .unwrap();
        assert_eq!(out.stdout.trim(), "From Env");
        assert!(ticks >= 1, "on_tick runs at least once after exit");
    }

    #[test]
    fn process_tree_tracks_descendants() {
        let mut cmd = std::process::Command::new("git");
        let mut child = ProcessTree::prepare(&mut cmd)
            .arg("--version")
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let tree = ProcessTree::attach(&child);
        assert!(tree.tracks_descendants());
        tree.kill(&mut child);
    }

    /// Cancelling also kills what git started (on Unix through its process group).
    #[cfg(unix)]
    #[test]
    fn cancel_kills_hook_children() {
        use std::os::unix::fs::PermissionsExt;

        let repo = test_support::TestRepo::new();
        let marker = format!("900.{}3", std::process::id());
        repo.write(
            ".git/hooks/pre-commit",
            &format!("#!/bin/sh\nsleep {marker}\n"),
        );
        let hook = repo.path.join(".git/hooks/pre-commit");
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        let sleepers = move || {
            let out = std::process::Command::new("sh")
                .args(["-c", &format!("pgrep -f '[s]leep {marker}' | wc -l")])
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout)
                .trim()
                .parse::<u32>()
                .unwrap_or(0)
        };
        let token = CancelToken::new();
        let watcher = {
            let token = token.clone();
            let sleepers = sleepers.clone();
            std::thread::spawn(move || {
                let started = Instant::now();
                while sleepers() == 0 && started.elapsed() < Duration::from_secs(20) {
                    std::thread::sleep(Duration::from_millis(50));
                }
                token.cancel();
            })
        };
        let args: Vec<String> = ["commit", "--allow-empty", "-m", "hooked"]
            .map(String::from)
            .to_vec();
        let err =
            run_streaming(&repo.git, &repo.path, &repo.ctx, &args, &token, |_| {}).unwrap_err();
        watcher.join().unwrap();
        assert!(matches!(err, OpError::Cancelled), "{err}");
        let deadline = Instant::now() + Duration::from_secs(5);
        while sleepers() > 0 {
            assert!(
                Instant::now() < deadline,
                "the hook's sleep survived the cancel"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
