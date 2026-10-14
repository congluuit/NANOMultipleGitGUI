//! Whole-file staging: stage, unstage and discard working-tree changes.
//!
//! Paths go through `--pathspec-from-file` on stdin (no command-line length limit on Windows)
//! with literal pathspecs, so names like `*.txt` or `:(glob)x` are never interpreted.

use std::path::Path;

use crate::{AccountContext, GitError, GitRunner};

fn nul_list(paths: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    for p in paths {
        out.extend_from_slice(p.as_bytes());
        out.push(0);
    }
    out
}

fn check(paths: &[String]) -> Result<(), GitError> {
    if let Some(bad) = paths
        .iter()
        .find(|p| p.is_empty() || p.contains('\0') || Path::new(p).is_absolute())
    {
        return Err(GitError::Failed {
            args: "pathspec".into(),
            status: "rejected".into(),
            stderr: format!("invalid path: {bad:?}"),
        });
    }
    Ok(())
}

fn run_with_paths(
    git: &GitRunner,
    root: &Path,
    args: &[&str],
    paths: &[String],
) -> Result<(), GitError> {
    if paths.is_empty() {
        return Ok(());
    }
    check(paths)?;
    let mut full = vec!["--literal-pathspecs"];
    full.extend_from_slice(args);
    full.extend(["--pathspec-from-file=-", "--pathspec-file-nul"]);
    git.run_with_input(root, &AccountContext::default(), &full, &nul_list(paths))?;
    Ok(())
}

/// Stages the given paths (new, modified and deleted files alike).
pub fn stage(git: &GitRunner, root: &Path, paths: &[String]) -> Result<(), GitError> {
    run_with_paths(git, root, &["add", "-A"], paths)
}

/// Stages every change in the working tree.
pub fn stage_all(git: &GitRunner, root: &Path) -> Result<(), GitError> {
    git.run(root, &AccountContext::default(), ["add", "-A"])?;
    Ok(())
}

/// Removes the given paths from the index, keeping the working tree. Works before the first
/// commit too. For a staged rename pass both the old and the new path.
pub fn unstage(git: &GitRunner, root: &Path, paths: &[String]) -> Result<(), GitError> {
    run_with_paths(git, root, &["reset", "-q"], paths)
}

pub fn unstage_all(git: &GitRunner, root: &Path) -> Result<(), GitError> {
    git.run(root, &AccountContext::default(), ["reset", "-q"])?;
    Ok(())
}

/// Throws away unstaged changes: tracked files are restored from the index, untracked files are
/// deleted (`git clean`, which never touches tracked or ignored files).
pub fn discard(
    git: &GitRunner,
    root: &Path,
    tracked: &[String],
    untracked: &[String],
) -> Result<(), GitError> {
    run_with_paths(git, root, &["restore", "--worktree"], tracked)?;
    check(untracked)?;
    for chunk in untracked.chunks(100) {
        let mut args = vec!["--literal-pathspecs", "clean", "-f", "-q", "--"];
        args.extend(chunk.iter().map(String::as_str));
        git.run(root, &AccountContext::default(), &args)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Identity;
    use crate::status::{ChangeKind, read_status};

    #[test]
    fn stage_unstage_discard_files() {
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
        run(&["config", "core.autocrlf", "false"]);

        // Before the first commit: stage and unstage still work.
        std::fs::write(p.join("[ab].txt"), "star\n").unwrap();
        std::fs::write(p.join("a.txt"), "a\n").unwrap();
        stage(&git, p, &["[ab].txt".into()]).unwrap();
        let st = read_status(&git, p).unwrap();
        assert_eq!(st.staged.len(), 1, "literal pathspec must not glob");
        assert_eq!(st.staged[0].path, "[ab].txt");
        unstage(&git, p, &["[ab].txt".into()]).unwrap();
        assert!(read_status(&git, p).unwrap().staged.is_empty());

        stage_all(&git, p).unwrap();
        run(&["commit", "-q", "-m", "init"]);

        std::fs::write(p.join("a.txt"), "changed\n").unwrap();
        std::fs::remove_file(p.join("[ab].txt")).unwrap();
        std::fs::write(p.join("new file.txt"), "n\n").unwrap();
        stage(&git, p, &["[ab].txt".into(), "a.txt".into()]).unwrap();
        let st = read_status(&git, p).unwrap();
        let kinds: Vec<_> = st
            .staged
            .iter()
            .map(|e| (e.path.as_str(), e.kind))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("[ab].txt", ChangeKind::Deleted),
                ("a.txt", ChangeKind::Modified)
            ]
        );

        unstage_all(&git, p).unwrap();
        discard(
            &git,
            p,
            &["a.txt".into(), "[ab].txt".into()],
            &["new file.txt".into()],
        )
        .unwrap();
        assert!(read_status(&git, p).unwrap().is_clean());
        assert_eq!(std::fs::read_to_string(p.join("a.txt")).unwrap(), "a\n");
        assert!(p.join("[ab].txt").exists());
        assert!(!p.join("new file.txt").exists());

        assert!(stage(&git, p, &[String::new()]).is_err());
    }
}
