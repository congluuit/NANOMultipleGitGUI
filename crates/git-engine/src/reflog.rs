//! Reading the reflog: where HEAD has been (to find "lost" commits again) and which branches
//! were checked out recently.

use std::path::Path;

use serde::Serialize;

use crate::{AccountContext, GitError, GitRunner};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ReflogEntry {
    /// Position in the reflog (0 = newest), as in `HEAD@{index}`.
    pub index: u32,
    pub oid: String,
    /// What moved HEAD: `commit`, `checkout`, `reset`, `rebase (finish)`, `pull`…
    pub action: String,
    /// The rest of the reflog message (`moving from a to b`, the commit subject…).
    pub message: String,
    /// Subject of the commit HEAD pointed to.
    pub subject: String,
    /// When HEAD moved, seconds since the Unix epoch (0 if unknown).
    pub time: i64,
}

/// Splits `commit (amend): fix typo` into the action and the message.
pub fn split_message(raw: &str) -> (String, String) {
    match raw.split_once(": ") {
        Some((action, rest)) => (action.trim().to_owned(), rest.trim().to_owned()),
        None => (raw.trim().to_owned(), String::new()),
    }
}

/// Parses `%H%x00%gd%x00%gs%x00%s` records (with `--date=unix`) separated by `%x01`.
pub fn parse_reflog(raw: &str) -> Vec<ReflogEntry> {
    raw.split('\u{1}')
        .map(|r| r.trim_matches(['\n', '\r']))
        .filter(|r| !r.is_empty())
        .enumerate()
        .filter_map(|(i, record)| {
            let mut f = record.split('\0');
            let oid = f.next()?.to_owned();
            let selector = f.next()?;
            let (action, message) = split_message(f.next()?);
            let subject = f.next().unwrap_or_default().to_owned();
            let time = selector
                .rsplit_once("@{")
                .and_then(|(_, t)| t.trim_end_matches('}').parse().ok())
                .unwrap_or(0);
            Some(ReflogEntry {
                index: i as u32,
                oid,
                action,
                message,
                subject,
                time,
            })
        })
        .collect()
}

/// The newest `limit` entries of HEAD's reflog (empty in a repository without commits).
pub fn read(git: &GitRunner, repo: &Path, limit: u32) -> Result<Vec<ReflogEntry>, GitError> {
    let out = git.run(
        repo,
        &AccountContext::default(),
        [
            "log",
            "-g",
            "--date=unix",
            "--format=%H%x00%gd%x00%gs%x00%s%x01",
            "-n",
            &limit.max(1).to_string(),
            "HEAD",
            "--",
        ],
    );
    match out {
        Ok(o) => Ok(parse_reflog(&String::from_utf8_lossy(&o.stdout))),
        // Unborn branch: there is no HEAD to walk yet.
        Err(GitError::Failed { .. }) => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

/// Branch names from `checkout: moving from <a> to <b>` messages, newest first, without
/// duplicates. Detached checkouts (a full commit id) are skipped.
pub fn recent_branches(entries: &[ReflogEntry], limit: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |name: &str| {
        let is_oid = name.len() >= 40 && name.bytes().all(|b| b.is_ascii_hexdigit());
        if !name.is_empty() && !is_oid && !out.iter().any(|n| n == name) {
            out.push(name.to_owned());
        }
    };
    for e in entries.iter().filter(|e| e.action == "checkout") {
        let Some(rest) = e.message.strip_prefix("moving from ") else {
            continue;
        };
        let Some((from, to)) = rest.rsplit_once(" to ") else {
            continue;
        };
        push(to);
        push(from);
    }
    out.truncate(limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::test_support::TestRepo;

    #[test]
    fn reads_head_movements_and_recent_branches() {
        let repo = TestRepo::new();
        repo.git(&["checkout", "-q", "-b", "feature/a"]);
        let lost = repo.commit("a.txt", "1\n", "work on a");
        repo.git(&["checkout", "-q", "main"]);
        repo.git(&["checkout", "-q", "-b", "topic"]);
        repo.git(&["branch", "-D", "feature/a"]);

        let entries = read(&repo.git, &repo.path, 50).unwrap();
        assert_eq!(entries[0].index, 0);
        assert_eq!(entries[0].action, "checkout");
        assert_eq!(entries[0].message, "moving from main to topic");
        assert!(entries[0].time > 1_600_000_000, "{:?}", entries[0]);
        let commit = entries.iter().find(|e| e.oid == lost).unwrap();
        assert_eq!(commit.subject, "work on a");
        assert!(entries.iter().any(|e| e.action == "commit"));

        assert_eq!(
            recent_branches(&entries, 10),
            ["topic", "main", "feature/a"]
        );
        assert_eq!(recent_branches(&entries, 2), ["topic", "main"]);
    }

    #[test]
    fn splits_actions_from_messages() {
        assert_eq!(
            split_message("commit (amend): fix: typo"),
            ("commit (amend)".into(), "fix: typo".into())
        );
        assert_eq!(split_message("reset"), ("reset".into(), String::new()));
    }
}
