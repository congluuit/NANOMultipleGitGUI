//! Creating and amending commits.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{AccountContext, GitError, GitRunner};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CommitOptions {
    /// Replace the last commit instead of creating a new one.
    pub amend: bool,
    /// Add a `Signed-off-by:` trailer.
    pub signoff: bool,
    /// Allow a commit without changes (amending only the message needs this off).
    #[serde(default)]
    pub allow_empty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CommitResult {
    pub hash: String,
    pub summary: String,
}

/// The identity git will record, as configured for this repository.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct GitIdentity {
    pub name: Option<String>,
    pub email: Option<String>,
}

fn config_value(git: &GitRunner, root: &Path, ctx: &AccountContext, key: &str) -> Option<String> {
    git.run_text(root, ctx, ["config", "--get", key])
        .ok()
        .filter(|v| !v.is_empty())
}

pub fn identity(git: &GitRunner, root: &Path, ctx: &AccountContext) -> GitIdentity {
    GitIdentity {
        name: config_value(git, root, ctx, "user.name"),
        email: config_value(git, root, ctx, "user.email"),
    }
}

/// Full message of HEAD (to prefill an amend), `None` before the first commit.
pub fn head_message(git: &GitRunner, root: &Path) -> Option<String> {
    git.run(
        root,
        &AccountContext::default(),
        ["log", "-1", "--format=%B", "HEAD", "--"],
    )
    .ok()
    .map(|o| String::from_utf8_lossy(&o.stdout).trim_end().to_owned())
}

/// Runs `git commit` with the message on stdin. On failure the error carries stdout and stderr
/// together: hooks may print to either, and "nothing to commit" goes to stdout.
fn run_commit(
    git: &GitRunner,
    root: &Path,
    ctx: &AccountContext,
    args: &[&str],
    message: &str,
) -> Result<(), GitError> {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = git
        .command(root, ctx, args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let data = message.as_bytes().to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&data));
    let out = child.wait_with_output()?;
    let _ = writer.join();
    if out.status.success() {
        return Ok(());
    }
    let text = [&out.stderr[..], &out.stdout[..]]
        .iter()
        .map(|b| String::from_utf8_lossy(b).trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    Err(GitError::Failed {
        args: args.join(" "),
        status: out.status.to_string(),
        stderr: text,
    })
}

/// Commits the index with `message` (passed on stdin, so any text is safe). Hook output and
/// errors come back in [`GitError::Failed`]'s `stderr`.
pub fn commit(
    git: &GitRunner,
    root: &Path,
    ctx: &AccountContext,
    message: &str,
    opts: &CommitOptions,
) -> Result<CommitResult, GitError> {
    if message.trim().is_empty() {
        return Err(GitError::Failed {
            args: "commit".into(),
            status: "rejected".into(),
            stderr: "the commit message is empty".into(),
        });
    }
    // `whitespace` keeps lines starting with `#` (e.g. "#123 fix"), unlike `strip`.
    let mut args = vec!["commit", "--quiet", "--cleanup=whitespace", "-F", "-"];
    if opts.amend {
        args.push("--amend");
    }
    if opts.signoff {
        args.push("--signoff");
    }
    if opts.allow_empty {
        args.push("--allow-empty");
    }
    run_commit(git, root, ctx, &args, message)?;
    let line = git.run_text(root, ctx, ["log", "-1", "--format=%H%x00%s", "HEAD", "--"])?;
    let (hash, summary) = line.split_once('\0').unwrap_or((&line, ""));
    Ok(CommitResult {
        hash: hash.to_owned(),
        summary: summary.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Identity;

    #[test]
    fn commit_amend_and_signoff() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        let git = GitRunner::default();
        let ctx = AccountContext {
            identity: Some(Identity {
                name: "Trần B".into(),
                email: "b@x.dev".into(),
            }),
            ..Default::default()
        };
        git.run(p, &ctx, ["init", "-q", "-b", "main"]).unwrap();
        assert_eq!(head_message(&git, p), None);
        let id = identity(&git, p, &ctx);
        assert_eq!(id.email.as_deref(), Some("b@x.dev"));

        assert!(commit(&git, p, &ctx, "  \n", &CommitOptions::default()).is_err());
        let err = commit(&git, p, &ctx, "empty", &CommitOptions::default()).unwrap_err();
        assert!(err.to_string().contains("nothing to commit"), "{err}");

        std::fs::write(p.join("a.txt"), "a\n").unwrap();
        git.run(p, &ctx, ["add", "a.txt"]).unwrap();
        let msg = "#12 first line\n\nbody with \"quotes\" and `ticks`\n";
        let c = commit(&git, p, &ctx, msg, &CommitOptions::default()).unwrap();
        assert_eq!(c.summary, "#12 first line");
        assert_eq!(c.hash.len(), 40);
        assert_eq!(
            head_message(&git, p).as_deref(),
            Some("#12 first line\n\nbody with \"quotes\" and `ticks`")
        );

        let amended = commit(
            &git,
            p,
            &ctx,
            "reworded",
            &CommitOptions {
                amend: true,
                signoff: true,
                allow_empty: false,
            },
        )
        .unwrap();
        assert_ne!(amended.hash, c.hash);
        let msg = head_message(&git, p).unwrap();
        assert!(msg.starts_with("reworded"));
        assert!(msg.contains("Signed-off-by: Trần B <b@x.dev>"));
        let count = git
            .run_text(p, &ctx, ["rev-list", "--count", "HEAD"])
            .unwrap();
        assert_eq!(count, "1");
    }
}
