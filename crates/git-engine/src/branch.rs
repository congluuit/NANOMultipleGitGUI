//! Local branches: create, checkout (local, from a remote branch, detached), rename, delete and
//! upstream configuration.
//!
//! Every name from the UI is validated (`git check-ref-format --branch`, no leading `-`) and
//! passed after `--end-of-options`, so it can never be read as an option.

use std::path::Path;

use crate::commit::is_valid_oid;
use crate::ops::{
    FailureKind, OpError, check_arg, current_branch, ref_exists, resolve_commit, run,
};
use crate::remote::validate_remote;
use crate::{AccountContext, GitRunner};

fn invalid_name(name: &str) -> OpError {
    OpError::rejected(
        FailureKind::InvalidName,
        format!("invalid branch name: {name:?}"),
    )
}

/// Checks a branch name with `git check-ref-format --branch`. Also rejects names starting with
/// `-`, `HEAD` and `@{…}` shorthands (which `--branch` would expand instead of validating).
pub fn validate_branch_name(git: &GitRunner, repo: &Path, name: &str) -> Result<(), OpError> {
    check_arg(name, "branch name")?;
    if name == "HEAD" || name.contains("@{") {
        return Err(invalid_name(name));
    }
    let normalized = git
        .run_text(
            repo,
            &AccountContext::default(),
            ["check-ref-format", "--branch", name],
        )
        .map_err(|_| invalid_name(name))?;
    if normalized != name {
        return Err(invalid_name(name));
    }
    Ok(())
}

fn ensure_local(git: &GitRunner, repo: &Path, name: &str) -> Result<(), OpError> {
    validate_branch_name(git, repo, name)?;
    if !ref_exists(git, repo, &format!("refs/heads/{name}")) {
        return Err(OpError::rejected(
            FailureKind::NotFound,
            format!("branch '{name}' not found"),
        ));
    }
    Ok(())
}

fn ensure_absent(git: &GitRunner, repo: &Path, name: &str) -> Result<(), OpError> {
    validate_branch_name(git, repo, name)?;
    if ref_exists(git, repo, &format!("refs/heads/{name}")) {
        return Err(OpError::rejected(
            FailureKind::AlreadyExists,
            format!("a branch named '{name}' already exists"),
        ));
    }
    Ok(())
}

/// Creates branch `name` at `start` (commit id, branch, tag, remote branch…), optionally
/// switching to it. The start point is resolved to a commit first, so no upstream is set.
pub fn create_branch(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    name: &str,
    start: &str,
    checkout: bool,
) -> Result<(), OpError> {
    ensure_absent(git, repo, name)?;
    let oid = resolve_commit(git, repo, start)?;
    if checkout {
        run(
            git,
            repo,
            ctx,
            &["switch", "-c", name, "--end-of-options", &oid],
        )?;
    } else {
        run(git, repo, ctx, &["branch", "--end-of-options", name, &oid])?;
    }
    Ok(())
}

/// Switches to local branch `name`. Local changes that would be overwritten are reported as
/// [`FailureKind::LocalChanges`] / [`FailureKind::UntrackedFiles`]; nothing is touched then.
pub fn checkout_branch(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    name: &str,
) -> Result<(), OpError> {
    ensure_local(git, repo, name)?;
    if current_branch(git, repo).as_deref() == Some(name) {
        return Ok(());
    }
    run(
        git,
        repo,
        ctx,
        &["switch", "--no-guess", "--end-of-options", name],
    )?;
    Ok(())
}

/// Checks out `remote/branch` as a new local branch tracking it (`git switch -c <local>
/// --track <remote>/<branch>`). `local_name` defaults to `branch`. Returns the local name.
pub fn checkout_remote_branch(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    remote: &str,
    branch: &str,
    local_name: Option<&str>,
) -> Result<String, OpError> {
    validate_remote(git, repo, remote)?;
    check_arg(branch, "branch name")?;
    let remote_ref = format!("refs/remotes/{remote}/{branch}");
    if !ref_exists(git, repo, &remote_ref) {
        return Err(OpError::rejected(
            FailureKind::NotFound,
            format!("remote branch '{remote}/{branch}' not found"),
        ));
    }
    let local = local_name.unwrap_or(branch);
    ensure_absent(git, repo, local)?;
    run(
        git,
        repo,
        ctx,
        &[
            "switch",
            "-c",
            local,
            "--track",
            "--end-of-options",
            &remote_ref,
        ],
    )?;
    Ok(local.to_owned())
}

/// Checks out commit `oid` with a detached HEAD.
pub fn checkout_detached(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    oid: &str,
) -> Result<(), OpError> {
    if !is_valid_oid(oid) {
        return Err(OpError::rejected(
            FailureKind::InvalidInput,
            format!("invalid commit id: {oid:?}"),
        ));
    }
    let full = resolve_commit(git, repo, oid)?;
    run(
        git,
        repo,
        ctx,
        &["switch", "--detach", "--end-of-options", &full],
    )?;
    Ok(())
}

/// Renames local branch `old` to `new` (works for the current branch too).
pub fn rename_branch(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    old: &str,
    new: &str,
) -> Result<(), OpError> {
    ensure_local(git, repo, old)?;
    if old == new {
        return Ok(());
    }
    ensure_absent(git, repo, new)?;
    run(
        git,
        repo,
        ctx,
        &["branch", "-m", "--end-of-options", old, new],
    )?;
    Ok(())
}

/// Deletes local branch `name`. Refuses the current branch ([`FailureKind::CurrentBranch`]);
/// without `force`, a branch with unmerged commits is refused with
/// [`FailureKind::NotFullyMerged`] (retry with `force` → `git branch -D`).
pub fn delete_branch(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    name: &str,
    force: bool,
) -> Result<(), OpError> {
    ensure_local(git, repo, name)?;
    if current_branch(git, repo).as_deref() == Some(name) {
        return Err(OpError::rejected(
            FailureKind::CurrentBranch,
            format!("cannot delete the checked-out branch '{name}'"),
        ));
    }
    let flag = if force { "-D" } else { "-d" };
    run(git, repo, ctx, &["branch", flag, "--end-of-options", name])?;
    Ok(())
}

/// Sets the upstream of `name` to `upstream` (`origin/main` or a local branch), or removes it
/// when `upstream` is `None` (no error if none was set).
pub fn set_upstream(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    name: &str,
    upstream: Option<&str>,
) -> Result<(), OpError> {
    ensure_local(git, repo, name)?;
    match upstream {
        Some(up) => {
            check_arg(up, "upstream")?;
            let full = [format!("refs/remotes/{up}"), format!("refs/heads/{up}")]
                .into_iter()
                .find(|r| ref_exists(git, repo, r))
                .ok_or_else(|| {
                    OpError::rejected(FailureKind::NotFound, format!("'{up}' not found"))
                })?;
            run(
                git,
                repo,
                ctx,
                &[
                    "branch",
                    &format!("--set-upstream-to={full}"),
                    "--end-of-options",
                    name,
                ],
            )?;
        }
        None => match run(
            git,
            repo,
            ctx,
            &["branch", "--unset-upstream", "--end-of-options", name],
        ) {
            Err(e) if e.kind() == Some(FailureKind::NoUpstream) => {}
            other => {
                other?;
            }
        },
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::test_support::TestRepo;

    #[test]
    fn validates_names_against_option_injection() {
        let r = TestRepo::new();
        for bad in [
            "--output=x",
            "-D",
            "-",
            "HEAD",
            "@{-1}",
            "a..b",
            "a b",
            "x.lock",
            "",
            "new\nline",
        ] {
            let err = validate_branch_name(&r.git, &r.path, bad).unwrap_err();
            assert_eq!(err.kind(), Some(FailureKind::InvalidName), "{bad:?}");
            let err = create_branch(&r.git, &r.path, &r.ctx, bad, "HEAD", true).unwrap_err();
            assert_eq!(err.kind(), Some(FailureKind::InvalidName), "{bad:?}");
        }
        for good in ["feature/x", "fix-1", "nhánh-việt"] {
            validate_branch_name(&r.git, &r.path, good).unwrap();
        }
        // Start points are validated as well.
        let err = create_branch(&r.git, &r.path, &r.ctx, "ok", "--output=x", false).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::InvalidName));
        assert!(!r.path.join("x").exists());
        assert_eq!(r.git(&["branch", "--list"]), "* main");
    }

    #[test]
    fn create_checkout_rename_delete() {
        let r = TestRepo::new();
        let base = r.head();
        let second = r.commit("a.txt", "a\n", "second");

        create_branch(&r.git, &r.path, &r.ctx, "old", &base[..7], false).unwrap();
        assert_eq!(r.git(&["rev-parse", "old"]), base);
        assert_eq!(r.branch(), "main");
        create_branch(&r.git, &r.path, &r.ctx, "topic", "main", true).unwrap();
        assert_eq!(r.branch(), "topic");
        assert_eq!(r.head(), second);
        let err = create_branch(&r.git, &r.path, &r.ctx, "topic", "main", false).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::AlreadyExists));
        let err = create_branch(&r.git, &r.path, &r.ctx, "x", "nope", false).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NotFound));

        checkout_branch(&r.git, &r.path, &r.ctx, "old").unwrap();
        assert_eq!(r.branch(), "old");
        checkout_branch(&r.git, &r.path, &r.ctx, "old").unwrap();
        let err = checkout_branch(&r.git, &r.path, &r.ctx, "missing").unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NotFound));

        rename_branch(&r.git, &r.path, &r.ctx, "old", "renamed").unwrap();
        assert_eq!(r.branch(), "renamed");
        let err = rename_branch(&r.git, &r.path, &r.ctx, "renamed", "main").unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::AlreadyExists));
        let err = rename_branch(&r.git, &r.path, &r.ctx, "renamed", "-D").unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::InvalidName));

        let err = delete_branch(&r.git, &r.path, &r.ctx, "renamed", false).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::CurrentBranch));
        checkout_branch(&r.git, &r.path, &r.ctx, "main").unwrap();
        // `renamed` is behind main: fully merged, plain delete works.
        delete_branch(&r.git, &r.path, &r.ctx, "renamed", false).unwrap();

        r.git(&["switch", "-q", "-c", "unmerged"]);
        r.commit("u.txt", "u\n", "unmerged work");
        r.git(&["switch", "-q", "main"]);
        let err = delete_branch(&r.git, &r.path, &r.ctx, "unmerged", false).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NotFullyMerged), "{err}");
        assert!(ref_exists(&r.git, &r.path, "refs/heads/unmerged"));
        delete_branch(&r.git, &r.path, &r.ctx, "unmerged", true).unwrap();
        assert!(!ref_exists(&r.git, &r.path, "refs/heads/unmerged"));
    }

    #[test]
    fn checkout_refuses_to_overwrite_local_changes() {
        let r = TestRepo::new();
        r.git(&["switch", "-q", "-c", "other"]);
        r.commit("README.md", "other\n", "other readme");
        r.commit("new.txt", "tracked on other\n", "add new");
        r.git(&["switch", "-q", "main"]);

        r.write("README.md", "local edit\n");
        let err = checkout_branch(&r.git, &r.path, &r.ctx, "other").unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::LocalChanges), "{err}");
        assert_eq!(r.branch(), "main");
        assert_eq!(r.read("README.md"), "local edit\n");
        // Creating + switching is atomic: no branch is left behind.
        let err = create_branch(&r.git, &r.path, &r.ctx, "n", "other", true).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::LocalChanges));
        assert!(!ref_exists(&r.git, &r.path, "refs/heads/n"));
        r.git(&["checkout", "--", "README.md"]);

        r.write("new.txt", "untracked here\n");
        let err = checkout_branch(&r.git, &r.path, &r.ctx, "other").unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::UntrackedFiles), "{err}");
        std::fs::remove_file(r.path.join("new.txt")).unwrap();
        checkout_branch(&r.git, &r.path, &r.ctx, "other").unwrap();
    }

    #[test]
    fn detached_checkout() {
        let r = TestRepo::new();
        let base = r.head();
        r.commit("a.txt", "a\n", "a");
        checkout_detached(&r.git, &r.path, &r.ctx, &base).unwrap();
        assert_eq!(r.branch(), "");
        assert_eq!(r.head(), base);
        for bad in ["HEAD", "main", "--detach", "-f"] {
            let err = checkout_detached(&r.git, &r.path, &r.ctx, bad).unwrap_err();
            assert_eq!(err.kind(), Some(FailureKind::InvalidInput), "{bad}");
        }
    }

    #[test]
    fn remote_branch_checkout_and_upstream() {
        let dir = tempfile::tempdir().unwrap();
        let upstream = TestRepo::init_at(&dir.path().join("up"));
        upstream.git(&["switch", "-q", "-c", "feature/x"]);
        let tip = upstream.commit("f.txt", "f\n", "feature");
        upstream.git(&["switch", "-q", "main"]);
        upstream
            .git
            .run(dir.path(), &upstream.ctx, ["clone", "-q", "up", "clone"])
            .unwrap();
        let r = TestRepo::open(&dir.path().join("clone"));

        let local =
            checkout_remote_branch(&r.git, &r.path, &r.ctx, "origin", "feature/x", None).unwrap();
        assert_eq!(local, "feature/x");
        assert_eq!(r.branch(), "feature/x");
        assert_eq!(r.head(), tip);
        assert_eq!(
            r.git(&["rev-parse", "--abbrev-ref", "feature/x@{u}"]),
            "origin/feature/x"
        );
        let err = checkout_remote_branch(&r.git, &r.path, &r.ctx, "origin", "feature/x", None)
            .unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::AlreadyExists));
        let local =
            checkout_remote_branch(&r.git, &r.path, &r.ctx, "origin", "main", Some("main-2"))
                .unwrap();
        assert_eq!(local, "main-2");
        let err =
            checkout_remote_branch(&r.git, &r.path, &r.ctx, "origin", "nope", None).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NotFound));
        let err = checkout_remote_branch(&r.git, &r.path, &r.ctx, "--upload-pack=x", "main", None)
            .unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::InvalidName));
        let err =
            checkout_remote_branch(&r.git, &r.path, &r.ctx, "nowhere", "main", None).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NotFound));

        set_upstream(&r.git, &r.path, &r.ctx, "main-2", None).unwrap();
        assert!(!r.try_git(&["rev-parse", "--abbrev-ref", "main-2@{u}"]));
        set_upstream(&r.git, &r.path, &r.ctx, "main-2", None).unwrap();
        set_upstream(&r.git, &r.path, &r.ctx, "main-2", Some("origin/feature/x")).unwrap();
        assert_eq!(
            r.git(&["rev-parse", "--abbrev-ref", "main-2@{u}"]),
            "origin/feature/x"
        );
        set_upstream(&r.git, &r.path, &r.ctx, "main-2", Some("main")).unwrap();
        assert_eq!(r.git(&["rev-parse", "--abbrev-ref", "main-2@{u}"]), "main");
        let err = set_upstream(&r.git, &r.path, &r.ctx, "main-2", Some("origin/gone")).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NotFound));
        let err =
            set_upstream(&r.git, &r.path, &r.ctx, "main-2", Some("--unset-upstream")).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::InvalidName));
    }
}
