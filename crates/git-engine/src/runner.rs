//! Runs the system `git` with an [`AccountContext`].

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

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
        let output = self.command(cwd, ctx, &args).output()?;
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
