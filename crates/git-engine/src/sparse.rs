//! Sparse checkout (cone mode): keep only some folders of a big repository on disk.

use std::path::Path;

use serde::Serialize;

use crate::ops::{self, FailureKind, OpError};
use crate::{AccountContext, GitRunner};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SparseState {
    pub enabled: bool,
    /// Folders kept on disk (files at the top level are always kept).
    pub folders: Vec<String>,
    /// Top-level folders of the current commit, to choose from.
    pub available: Vec<String>,
}

fn config_true(git: &GitRunner, repo: &Path, key: &str) -> bool {
    git.run_text(
        repo,
        &AccountContext::default(),
        ["config", "--type=bool", "--get", key],
    )
    .is_ok_and(|v| v == "true")
}

/// Whether sparse checkout is on, the folders kept and the folders that exist.
pub fn state(git: &GitRunner, repo: &Path) -> Result<SparseState, OpError> {
    let ctx = AccountContext::default();
    let enabled = config_true(git, repo, "core.sparseCheckout");
    let folders = if enabled {
        git.run_text(repo, &ctx, ["sparse-checkout", "list"])
            .map(|out| {
                out.lines()
                    .map(|l| l.trim().trim_matches('/').to_owned())
                    .filter(|l| !l.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let available = match ops::head_oid(git, repo) {
        Some(_) => {
            let out = git.run(repo, &ctx, ["ls-tree", "-d", "-z", "--name-only", "HEAD"])?;
            String::from_utf8_lossy(&out.stdout)
                .split('\0')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        }
        None => Vec::new(),
    };
    Ok(SparseState {
        enabled,
        folders,
        available,
    })
}

/// Normalises a folder typed by the user: `/`-separated, no leading or trailing slash.
pub fn clean_folder(raw: &str) -> Option<String> {
    let path = raw.trim().replace('\\', "/");
    let path = path.trim_matches('/');
    let bad = path.is_empty()
        || path.starts_with('-')
        || path.chars().any(char::is_control)
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..");
    (!bad).then(|| path.to_owned())
}

/// Keeps only `folders` (and top-level files) in the working tree. Refused while there are
/// uncommitted changes outside of them: git would have to throw them away.
pub fn set(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    folders: &[String],
) -> Result<SparseState, OpError> {
    ops::ensure_idle(git, repo)?;
    let mut clean = Vec::new();
    for raw in folders {
        let Some(folder) = clean_folder(raw) else {
            return Err(OpError::rejected(
                FailureKind::InvalidInput,
                format!("not a folder inside the repository: {raw:?}"),
            ));
        };
        if !clean.contains(&folder) {
            clean.push(folder);
        }
    }
    if clean.is_empty() {
        return Err(OpError::rejected(
            FailureKind::InvalidInput,
            "choose at least one folder to keep",
        ));
    }
    let mut args = vec!["sparse-checkout", "set", "--cone", "--end-of-options"];
    args.extend(clean.iter().map(String::as_str));
    ops::run(git, repo, ctx, &args)?;
    state(git, repo)
}

/// Turns sparse checkout off: every file comes back.
pub fn disable(git: &GitRunner, repo: &Path, ctx: &AccountContext) -> Result<SparseState, OpError> {
    ops::ensure_idle(git, repo)?;
    ops::run(git, repo, ctx, &["sparse-checkout", "disable"])?;
    state(git, repo)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::test_support::TestRepo;

    #[test]
    fn keeps_only_the_chosen_folders() {
        let repo = TestRepo::new();
        repo.commit("docs/guide.md", "g\n", "docs");
        repo.commit("src/main.rs", "m\n", "src");
        repo.commit("assets/big.bin", "b\n", "assets");

        let before = state(&repo.git, &repo.path).unwrap();
        assert!(!before.enabled);
        assert_eq!(before.available, ["assets", "docs", "src"]);

        let st = set(
            &repo.git,
            &repo.path,
            &repo.ctx,
            &["src\\".into(), "/docs/".into(), "src".into()],
        )
        .unwrap();
        assert!(st.enabled);
        assert_eq!(st.folders, ["docs", "src"]);
        assert!(repo.path.join("src/main.rs").exists());
        assert!(repo.path.join("README.md").exists());
        assert!(!repo.path.join("assets/big.bin").exists());

        let st = disable(&repo.git, &repo.path, &repo.ctx).unwrap();
        assert!(!st.enabled);
        assert!(repo.path.join("assets/big.bin").exists());
    }

    #[test]
    fn rejects_paths_outside_the_repository() {
        assert_eq!(clean_folder(" a\\b/ "), Some("a/b".into()));
        assert_eq!(clean_folder("../x"), None);
        assert_eq!(clean_folder("--cone"), None);
        assert_eq!(clean_folder("  "), None);
        let repo = TestRepo::new();
        let err = set(&repo.git, &repo.path, &repo.ctx, &[]).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::InvalidInput));
    }
}
