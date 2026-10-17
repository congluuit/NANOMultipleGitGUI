//! Branches, remotes, tags and stashes of a repository (sidebar data).

use std::path::Path;

use serde::Serialize;

use crate::{AccountContext, GitError, GitRunner};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Head {
    /// Current branch, also set for an unborn branch in a fresh repository.
    pub branch: Option<String>,
    pub oid: Option<String>,
    pub detached: bool,
    /// No commit yet.
    pub unborn: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LocalBranch {
    pub name: String,
    pub oid: String,
    /// Short upstream name, e.g. `origin/main`.
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// The upstream is configured but no longer exists on the remote.
    pub upstream_gone: bool,
    pub is_head: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RemoteBranch {
    /// Branch name without the remote prefix (`feature/x`).
    pub name: String,
    pub oid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Remote {
    pub name: String,
    pub url: Option<String>,
    pub branches: Vec<RemoteBranch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub name: String,
    /// Commit the tag points to (peeled for annotated tags).
    pub oid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Stash {
    pub index: u32,
    pub message: String,
    /// When the entry was created, seconds since the Unix epoch (0 if unknown).
    pub time: i64,
    /// Branch the stash was made on (`WIP on <branch>:` / `On <branch>:`); `None` when it
    /// cannot be read from the message.
    pub branch: Option<String>,
}

/// Branch from a stash subject: `WIP on main: abc subject` / `On feature/x: message`.
/// Detached HEAD shows as `(no branch)` and gives `None`.
pub fn stash_branch(message: &str) -> Option<String> {
    let rest = message
        .strip_prefix("WIP on ")
        .or_else(|| message.strip_prefix("On "))?;
    let branch = rest.split_once(':')?.0.trim();
    (!branch.is_empty() && branch != "(no branch)").then(|| branch.to_owned())
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RepoRefs {
    pub head: Head,
    pub local: Vec<LocalBranch>,
    pub remotes: Vec<Remote>,
    pub tags: Vec<Tag>,
    pub stashes: Vec<Stash>,
}

/// Parses `%(upstream:track,nobracket)`: `ahead 1, behind 2`, `behind 3`, `gone`, or empty.
pub fn parse_track(track: &str) -> (u32, u32, bool) {
    if track.trim() == "gone" {
        return (0, 0, true);
    }
    let (mut ahead, mut behind) = (0, 0);
    for part in track.split(',') {
        let mut it = part.split_whitespace();
        match (it.next(), it.next().and_then(|n| n.parse().ok())) {
            (Some("ahead"), Some(n)) => ahead = n,
            (Some("behind"), Some(n)) => behind = n,
            _ => {}
        }
    }
    (ahead, behind, false)
}

const REF_FORMAT: &str = "--format=%(refname)%00%(objectname)%00%(*objectname)%00%(upstream:short)%00%(upstream:track,nobracket)%00%(HEAD)%00%(symref)";

/// Builds [`RepoRefs`] from raw command outputs (kept separate so it is unit-testable).
pub fn parse_refs(
    for_each_ref: &str,
    remote_urls: &str,
    remote_names: &str,
    stash_list: &str,
) -> RepoRefs {
    let mut remotes: Vec<Remote> = remote_names
        .lines()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(|name| Remote {
            name: name.to_owned(),
            url: None,
            branches: Vec::new(),
        })
        .collect();
    for line in remote_urls.lines() {
        let Some((key, url)) = line.split_once(' ') else {
            continue;
        };
        let Some(name) = key
            .strip_prefix("remote.")
            .and_then(|k| k.strip_suffix(".url"))
        else {
            continue;
        };
        if let Some(r) = remotes.iter_mut().find(|r| r.name == name) {
            r.url = Some(url.trim().to_owned());
        }
    }
    // Longest names first so `origin/x` is not matched by a remote called `ori`.
    let mut by_length: Vec<usize> = (0..remotes.len()).collect();
    by_length.sort_by_key(|&i| std::cmp::Reverse(remotes[i].name.len()));

    let mut refs = RepoRefs::default();
    for line in for_each_ref.lines() {
        let f: Vec<&str> = line.split('\0').collect();
        if f.len() < 7 {
            continue;
        }
        let (refname, oid, peeled, upstream, track, head, symref) =
            (f[0], f[1], f[2], f[3], f[4], f[5], f[6]);
        if let Some(name) = refname.strip_prefix("refs/heads/") {
            let (ahead, behind, gone) = parse_track(track);
            refs.local.push(LocalBranch {
                name: name.to_owned(),
                oid: oid.to_owned(),
                upstream: (!upstream.is_empty()).then(|| upstream.to_owned()),
                ahead,
                behind,
                upstream_gone: gone,
                is_head: head == "*",
            });
        } else if let Some(rest) = refname.strip_prefix("refs/remotes/") {
            if !symref.is_empty() {
                continue; // origin/HEAD
            }
            for &i in &by_length {
                let prefix = format!("{}/", remotes[i].name);
                if let Some(branch) = rest.strip_prefix(&prefix) {
                    remotes[i].branches.push(RemoteBranch {
                        name: branch.to_owned(),
                        oid: oid.to_owned(),
                    });
                    break;
                }
            }
        } else if let Some(name) = refname.strip_prefix("refs/tags/") {
            refs.tags.push(Tag {
                name: name.to_owned(),
                oid: if peeled.is_empty() { oid } else { peeled }.to_owned(),
            });
        }
    }
    refs.remotes = remotes;

    for line in stash_list.lines() {
        // `stash@{n}␟<unix time>␟<subject>` (a line without the time field still parses).
        let mut fields = line.splitn(3, '\u{1f}');
        let (Some(selector), Some(second)) = (fields.next(), fields.next()) else {
            continue;
        };
        let (time, message) = match fields.next() {
            Some(message) => (second.trim().parse().unwrap_or(0), message),
            None => (0, second),
        };
        let index = selector
            .trim_start_matches("stash@{")
            .trim_end_matches('}')
            .parse()
            .unwrap_or(refs.stashes.len() as u32);
        refs.stashes.push(Stash {
            index,
            message: message.to_owned(),
            time,
            branch: stash_branch(message),
        });
    }
    refs
}

pub fn read_refs(git: &GitRunner, root: &Path) -> Result<RepoRefs, GitError> {
    let ctx = AccountContext::default();
    let for_each = git.run_text(
        root,
        &ctx,
        [
            "for-each-ref",
            "--sort=refname",
            REF_FORMAT,
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
    )?;
    let names = git.run_text(root, &ctx, ["remote"])?;
    // Exit code 1 just means "no remotes configured".
    let urls = git
        .run_text(root, &ctx, ["config", "--get-regexp", r"^remote\..*\.url$"])
        .unwrap_or_default();
    let stashes = git
        .run_text(root, &ctx, ["stash", "list", "--format=%gd%x1f%ct%x1f%s"])
        .unwrap_or_default();
    let mut refs = parse_refs(&for_each, &urls, &names, &stashes);

    let branch = git
        .run_text(root, &ctx, ["symbolic-ref", "-q", "--short", "HEAD"])
        .ok()
        .filter(|s| !s.is_empty());
    let oid = git
        .run_text(root, &ctx, ["rev-parse", "-q", "--verify", "HEAD"])
        .ok()
        .filter(|s| !s.is_empty());
    refs.head = Head {
        detached: branch.is_none() && oid.is_some(),
        unborn: oid.is_none(),
        branch,
        oid,
    };
    Ok(refs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Identity;

    #[test]
    fn track_parsing() {
        assert_eq!(parse_track(""), (0, 0, false));
        assert_eq!(parse_track("ahead 3"), (3, 0, false));
        assert_eq!(parse_track("behind 2"), (0, 2, false));
        assert_eq!(parse_track("ahead 1, behind 12"), (1, 12, false));
        assert_eq!(parse_track("gone"), (0, 0, true));
    }

    #[test]
    fn parses_raw_outputs() {
        let fer = [
            "refs/heads/feature/x\0aaa\0\0\0\0 \0",
            "refs/heads/main\0bbb\0\0origin/main\0ahead 1, behind 2\0*\0",
            "refs/remotes/origin/HEAD\0bbb\0\0\0\0 \0refs/remotes/origin/main",
            "refs/remotes/origin/main\0ccc\0\0\0\0 \0",
            "refs/remotes/origin-mirror/main\0ddd\0\0\0\0 \0",
            "refs/tags/v1.0\0tagobj\0commit1\0\0\0 \0",
            "refs/tags/v0.9\0commit0\0\0\0\0 \0",
        ]
        .join("\n");
        let urls = "remote.origin.url https://github.com/acme/api.git\nremote.origin-mirror.url git@gitlab.com:acme/api.git";
        let refs = parse_refs(
            &fer,
            urls,
            "origin\norigin-mirror\nempty",
            "stash@{0}\u{1f}WIP on main: abc\nstash@{1}\u{1f}On feature: try",
        );

        assert_eq!(refs.local.len(), 2);
        let main = refs.local.iter().find(|b| b.name == "main").unwrap();
        assert!(main.is_head);
        assert_eq!(main.upstream.as_deref(), Some("origin/main"));
        assert_eq!((main.ahead, main.behind), (1, 2));
        assert!(!refs.local[0].is_head);

        assert_eq!(refs.remotes.len(), 3);
        assert_eq!(
            refs.remotes[0].url.as_deref(),
            Some("https://github.com/acme/api.git")
        );
        assert_eq!(refs.remotes[0].branches.len(), 1, "origin/HEAD skipped");
        assert_eq!(refs.remotes[1].name, "origin-mirror");
        assert_eq!(refs.remotes[1].branches[0].name, "main");
        assert!(refs.remotes[2].branches.is_empty());

        assert_eq!(refs.tags[0].oid, "commit1", "annotated tag is peeled");
        assert_eq!(refs.tags[1].oid, "commit0");
        assert_eq!(refs.stashes.len(), 2);
        assert_eq!(refs.stashes[1].index, 1);
        assert_eq!(refs.stashes[1].message, "On feature: try");
        assert_eq!(refs.stashes[1].branch.as_deref(), Some("feature"));
        assert_eq!(refs.stashes[0].branch.as_deref(), Some("main"));
    }

    #[test]
    fn stash_lines_carry_time_and_branch() {
        let refs = parse_refs(
            "",
            "",
            "",
            "stash@{0}\u{1f}1700000000\u{1f}On feature/x: half: done\nstash@{1}\u{1f}12\u{1f}WIP on (no branch): abc s",
        );
        assert_eq!(refs.stashes[0].time, 1_700_000_000);
        assert_eq!(refs.stashes[0].message, "On feature/x: half: done");
        assert_eq!(refs.stashes[0].branch.as_deref(), Some("feature/x"));
        assert_eq!(refs.stashes[1].branch, None);
        assert_eq!(stash_branch("custom message"), None);
    }

    #[test]
    fn reads_a_real_repository_with_remote_tags_and_stash() {
        let dir = tempfile::tempdir().unwrap();
        let git = GitRunner::default();
        let ctx = AccountContext {
            identity: Some(Identity {
                name: "T".into(),
                email: "t@x".into(),
            }),
            ..Default::default()
        };
        let remote = dir.path().join("remote.git");
        let work = dir.path().join("work");
        let run = |cwd: &Path, args: &[&str]| git.run(cwd, &ctx, args).unwrap();
        run(
            dir.path(),
            &["init", "-q", "--bare", "-b", "main", "remote.git"],
        );
        std::fs::create_dir_all(&work).unwrap();

        // Fresh repo: unborn branch, no refs.
        run(&work, &["init", "-q", "-b", "main"]);
        let refs = read_refs(&git, &work).unwrap();
        assert!(refs.head.unborn);
        assert_eq!(refs.head.branch.as_deref(), Some("main"));
        assert!(refs.local.is_empty());

        run(&work, &["commit", "-q", "--allow-empty", "-m", "one"]);
        run(
            &work,
            &["remote", "add", "origin", &remote.to_string_lossy()],
        );
        run(&work, &["push", "-q", "-u", "origin", "main"]);
        run(&work, &["commit", "-q", "--allow-empty", "-m", "two"]);
        run(&work, &["tag", "-a", "v1", "-m", "release"]);
        run(&work, &["branch", "topic"]);
        std::fs::write(work.join("f.txt"), "x").unwrap();
        run(&work, &["add", "f.txt"]);
        run(&work, &["stash", "push", "-q", "-m", "half done"]);

        let refs = read_refs(&git, &work).unwrap();
        assert!(!refs.head.unborn && !refs.head.detached);
        let main = refs.local.iter().find(|b| b.name == "main").unwrap();
        assert!(main.is_head);
        assert_eq!((main.ahead, main.behind), (1, 0));
        assert_eq!(main.upstream.as_deref(), Some("origin/main"));
        assert!(refs.local.iter().any(|b| b.name == "topic" && !b.is_head));
        assert_eq!(refs.remotes.len(), 1);
        assert_eq!(refs.remotes[0].branches[0].name, "main");
        assert_eq!(refs.tags[0].name, "v1");
        assert_eq!(refs.tags[0].oid, main.oid, "annotated tag peeled to commit");
        assert_eq!(refs.stashes.len(), 1);
        assert!(refs.stashes[0].message.contains("half done"));

        run(&work, &["checkout", "-q", "--detach", "HEAD~1"]);
        let refs = read_refs(&git, &work).unwrap();
        assert!(refs.head.detached);
        assert_eq!(refs.head.branch, None);
    }
}
