//! Runs the system `git` with an [`AccountContext`].

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use crate::context::AccountContext;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("folder does not exist: {}", .0.display())]
    PathNotFound(PathBuf),
    #[error("not a git repository: {}", .0.display())]
    NotARepository(PathBuf),
    #[error("bare repositories are not supported yet: {}", .0.display())]
    BareRepository(PathBuf),
    #[error("git refuses to use this repository because it is owned by another user (safe.directory): {}", .0.display())]
    DubiousOwnership(PathBuf),
    #[error("`git {args}` failed ({status}): {stderr}")]
    Failed {
        args: String,
        status: String,
        stderr: String,
    },
}

/// How long a command waits for another process's `index.lock` to go away.
const INDEX_LOCK_WAIT: Duration = Duration::from_secs(3);
const INDEX_LOCK_POLL: Duration = Duration::from_millis(250);

/// `fatal: Unable to create '…/.git/index.lock': File exists.`
pub fn is_index_lock_busy(stderr: &str) -> bool {
    let s = stderr.to_ascii_lowercase();
    s.contains("index.lock': file exists")
}

/// Commands that take `index.lock` before changing anything and hold it to the end, so one
/// that failed on a busy lock did nothing and can simply run again. Multi-step commands
/// (stash, merge, rebase, cherry-pick…) could fail half-way and are not retried.
fn retries_on_index_lock(args: &[std::ffi::OsString]) -> bool {
    const SAFE: &[&str] = &[
        "add",
        "rm",
        "mv",
        "reset",
        "restore",
        "commit",
        "apply",
        "update-index",
        "read-tree",
        "checkout",
        "switch",
    ];
    let mut it = args.iter().map(|a| a.to_string_lossy());
    while let Some(arg) = it.next() {
        match arg.as_ref() {
            // `git -c key=value <cmd>` / `git -C dir <cmd>`: skip the option's value.
            "-c" | "-C" => {
                it.next();
            }
            a if a.starts_with('-') => {}
            a => return SAFE.contains(&a),
        }
    }
    false
}

/// Handle to a git executable. Cheap to clone.
#[derive(Debug, Clone)]
pub struct GitRunner {
    program: PathBuf,
    extra_env: Vec<(String, String)>,
}

impl Default for GitRunner {
    fn default() -> Self {
        Self::new("git")
    }
}

impl GitRunner {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            extra_env: Vec::new(),
        }
    }

    /// Extra environment applied to every command (e.g. test isolation variables).
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_env.push((key.into(), value.into()));
        self
    }

    /// A `Command` for `git <args>` in `cwd`, carrying the account context.
    pub fn command<I, S>(&self, cwd: &Path, ctx: &AccountContext, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut cmd = Command::new(&self.program);
        cmd.current_dir(cwd)
            .args(args)
            .stdin(Stdio::null())
            // Read commands (status, diff) must not rewrite .git/index behind the user's back;
            // that would also wake our own file watcher in a loop.
            .env("GIT_OPTIONAL_LOCKS", "0")
            .envs(self.extra_env.iter().map(|(k, v)| (k, v)))
            .envs(ctx.env());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd
    }

    /// Runs `git <args>` and returns its output, or an error carrying stderr on failure.
    pub fn run<I, S>(&self, cwd: &Path, ctx: &AccountContext, args: I) -> Result<Output, GitError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args: Vec<_> = args.into_iter().map(|a| a.as_ref().to_owned()).collect();
        let retry_on_lock = retries_on_index_lock(&args);
        let mut waited = Duration::ZERO;
        loop {
            let output = self.command(cwd, ctx, &args).output()?;
            if output.status.success() {
                return Ok(output);
            }
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            // Another git (an editor's, a terminal's, our own previous click) holds
            // `.git/index.lock` for a moment: wait for it instead of failing at once. A lock
            // still there after the wait is reported (the UI offers to remove a stale one).
            if retry_on_lock && waited < INDEX_LOCK_WAIT && is_index_lock_busy(&stderr) {
                std::thread::sleep(INDEX_LOCK_POLL);
                waited += INDEX_LOCK_POLL;
                continue;
            }
            return Err(GitError::Failed {
                args: args
                    .iter()
                    .map(|a| a.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" "),
                status: output.status.to_string(),
                stderr,
            });
        }
    }

    /// Like [`run`](Self::run) but feeds `input` to the process's stdin
    /// (for `--stdin` revision lists that could exceed the OS command-line limit).
    pub fn run_with_input<I, S>(
        &self,
        cwd: &Path,
        ctx: &AccountContext,
        args: I,
        input: &[u8],
    ) -> Result<Output, GitError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        use std::io::Write;
        let args: Vec<_> = args.into_iter().map(|a| a.as_ref().to_owned()).collect();
        let mut child = self
            .command(cwd, ctx, &args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let data = input.to_vec();
        let writer = std::thread::spawn(move || stdin.write_all(&data));
        let output = child.wait_with_output()?;
        let _ = writer.join();
        if output.status.success() {
            Ok(output)
        } else {
            Err(GitError::Failed {
                args: args
                    .iter()
                    .map(|a| a.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" "),
                status: output.status.to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            })
        }
    }

    /// Like [`run`](Self::run) but returns trimmed stdout as a string.
    pub fn run_text<I, S>(
        &self,
        cwd: &Path,
        ctx: &AccountContext,
        args: I,
    ) -> Result<String, GitError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let out = self.run(cwd, ctx, args)?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Identity;

    #[test]
    fn injected_config_is_visible_to_git() {
        let dir = tempfile::tempdir().unwrap();
        let git = GitRunner::default();
        let ctx = AccountContext {
            identity: Some(Identity {
                name: "Injected Name".into(),
                email: "injected@example.com".into(),
            }),
            ..Default::default()
        };
        git.run(dir.path(), &ctx, ["init", "-q"]).unwrap();
        assert_eq!(
            git.run_text(dir.path(), &ctx, ["config", "user.email"])
                .unwrap(),
            "injected@example.com"
        );
        // Nothing was written to the repository's own config.
        let local = git.run(
            dir.path(),
            &AccountContext::default(),
            ["config", "--local", "user.email"],
        );
        assert!(local.is_err());
    }

    #[test]
    fn waits_for_a_busy_index_lock() {
        let dir = tempfile::tempdir().unwrap();
        let git = GitRunner::default();
        let ctx = AccountContext::default();
        git.run(dir.path(), &ctx, ["init", "-q"]).unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        let lock = dir.path().join(".git/index.lock");
        std::fs::write(&lock, "").unwrap();
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(600));
            std::fs::remove_file(lock).unwrap();
        });
        git.run(dir.path(), &ctx, ["add", "-A"]).unwrap();
        releaser.join().unwrap();
    }

    #[test]
    fn only_lock_safe_commands_are_retried() {
        let args = |v: &[&str]| v.iter().map(std::ffi::OsString::from).collect::<Vec<_>>();
        assert!(retries_on_index_lock(&args(&["add", "-A"])));
        assert!(retries_on_index_lock(&args(&[
            "-c", "core.x=y", "commit", "-m", "x"
        ])));
        assert!(!retries_on_index_lock(&args(&["stash", "pop"])));
        assert!(!retries_on_index_lock(&args(&["rebase", "main"])));
        assert!(is_index_lock_busy(
            "fatal: Unable to create '/r/.git/index.lock': File exists."
        ));
        assert!(!is_index_lock_busy("fatal: not a git repository"));
    }

    #[test]
    fn failure_reports_stderr() {
        let dir = tempfile::tempdir().unwrap();
        let err = GitRunner::default()
            .run(
                dir.path(),
                &AccountContext::default(),
                ["rev-parse", "HEAD"],
            )
            .unwrap_err();
        assert!(matches!(err, GitError::Failed { ref stderr, .. } if !stderr.is_empty()));
    }
}
