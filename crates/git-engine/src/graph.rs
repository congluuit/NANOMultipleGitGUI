//! Commit history reading and lane layout for the commit graph.
//!
//! The layout is row-local: each row stores the line segments of its upper half (from the lanes
//! entering the row to the node or straight through) and of its lower half (from the node or
//! the lanes passing through to the lanes leaving the row). A renderer can therefore draw any
//! window of rows independently, which is what makes virtualized scrolling cheap.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::{AccountContext, GitError, GitRunner};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommitInfo {
    pub hash: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    /// Author time, seconds since the Unix epoch.
    pub author_time: i64,
    pub subject: String,
}

const FIELD: char = '\u{1f}';

pub(crate) const LOG_FORMAT: &str = "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%s";

/// Reads the history of all refs in topological order (children before parents).
pub fn read_log(
    git: &GitRunner,
    repo: &Path,
    max_count: Option<usize>,
) -> Result<Vec<CommitInfo>, GitError> {
    let mut args = vec![
        "-c".to_owned(),
        "log.showSignature=false".to_owned(),
        "log".to_owned(),
        // Stashes are listed in the sidebar; their internal WIP/index commits would only add noise.
        "--exclude=refs/stash".to_owned(),
        // Backups made before history rewrites (rebase, LFS migrate, author fix) stay hidden.
        "--exclude=refs/nanomgg/*".to_owned(),
        "--all".to_owned(),
        "--topo-order".to_owned(),
        "-z".to_owned(),
        LOG_FORMAT.to_owned(),
    ];
    if let Some(n) = max_count {
        args.push(format!("--max-count={n}"));
    }
    let out = git.run(repo, &AccountContext::default(), &args)?;
    Ok(parse_log(&String::from_utf8_lossy(&out.stdout)))
}

pub fn parse_log(raw: &str) -> Vec<CommitInfo> {
    raw.split('\0')
        .filter(|r| !r.trim().is_empty())
        .filter_map(|record| {
            let mut f = record.trim_start_matches('\n').splitn(7, FIELD);
            let hash = f.next()?.to_owned();
            let parents = f.next()?.split_whitespace().map(str::to_owned).collect();
            let author_name = f.next()?.to_owned();
            let author_email = f.next()?.to_owned();
            let author_time = f.next()?.parse().unwrap_or(0);
            let subject = f.next().unwrap_or("").to_owned();
            Some(CommitInfo {
                hash,
                parents,
                author_name,
                author_email,
                author_time,
                subject,
            })
        })
        .collect()
}

/// A line segment inside one row, between lane positions. Colors index the 8 lane colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Segment {
    pub from: u16,
    pub to: u16,
    pub color: u8,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RowLayout {
    /// Lane of this row's commit node.
    pub lane: u16,
    pub color: u8,
    /// Top edge (`from` lane) → row center (`to` lane).
    pub top: Vec<Segment>,
    /// Row center (`from` lane) → bottom edge (`to` lane).
    pub bottom: Vec<Segment>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct GraphLayout {
    pub rows: Vec<RowLayout>,
    /// Widest number of simultaneously active lanes.
    pub max_lanes: u16,
}

pub const LANE_COLORS: u8 = 8;

#[derive(Clone, Copy)]
struct Lane {
    /// Index of the commit this lane is waiting for.
    expects: usize,
    color: u8,
}

/// Computes lanes for commits in topological order. Parents that are not part of `commits`
/// (shallow clones, `--max-count`) simply end their lane at the bottom of the graph.
pub fn layout(commits: &[CommitInfo]) -> GraphLayout {
    let index: HashMap<&str, usize> = commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.hash.as_str(), i))
        .collect();
    let mut lanes: Vec<Option<Lane>> = Vec::new();
    let mut next_color: u8 = 0;
    let mut new_color = || {
        let c = next_color;
        next_color = (next_color + 1) % LANE_COLORS;
        c
    };
    let mut rows = Vec::with_capacity(commits.len());
    let mut max_lanes = 0usize;

    for (i, commit) in commits.iter().enumerate() {
        let before = lanes.clone();
        let matching: Vec<usize> = before
            .iter()
            .enumerate()
            .filter_map(|(l, lane)| lane.filter(|x| x.expects == i).map(|_| l))
            .collect();
        let lane = match matching.first() {
            Some(&l) => l,
            None => lanes
                .iter()
                .position(Option::is_none)
                .unwrap_or(lanes.len()),
        };
        let color = match matching.first() {
            Some(&l) => before[l].map(|x| x.color).unwrap_or(0),
            None => new_color(),
        };

        let mut row = RowLayout {
            lane: lane as u16,
            color,
            ..Default::default()
        };
        for (l, entry) in before.iter().enumerate() {
            if let Some(entry) = entry {
                let to = if matching.contains(&l) { lane } else { l };
                row.top.push(Segment {
                    from: l as u16,
                    to: to as u16,
                    color: entry.color,
                });
            }
        }

        for &l in &matching {
            lanes[l] = None;
        }
        if lanes.len() <= lane {
            lanes.resize(lane + 1, None);
        }

        let parents: Vec<usize> = commit
            .parents
            .iter()
            .filter_map(|p| index.get(p.as_str()).copied())
            .collect();
        let mut assigned = vec![lane];
        for (k, &parent) in parents.iter().enumerate() {
            let existing = lanes
                .iter()
                .position(|x| x.is_some_and(|x| x.expects == parent));
            match existing {
                Some(target) => row.bottom.push(Segment {
                    from: lane as u16,
                    to: target as u16,
                    color: lanes[target].map(|x| x.color).unwrap_or(color),
                }),
                None => {
                    let (target, lane_color) = if k == 0 {
                        (lane, color)
                    } else {
                        let free = lanes
                            .iter()
                            .enumerate()
                            .position(|(idx, x)| x.is_none() && idx != lane)
                            .unwrap_or(lanes.len());
                        if lanes.len() <= free {
                            lanes.resize(free + 1, None);
                        }
                        (free, new_color())
                    };
                    lanes[target] = Some(Lane {
                        expects: parent,
                        color: lane_color,
                    });
                    assigned.push(target);
                    row.bottom.push(Segment {
                        from: lane as u16,
                        to: target as u16,
                        color: lane_color,
                    });
                }
            }
        }

        for (l, entry) in lanes.iter().enumerate() {
            if let Some(entry) = entry
                && !assigned.contains(&l)
            {
                row.bottom.push(Segment {
                    from: l as u16,
                    to: l as u16,
                    color: entry.color,
                });
            }
        }

        while lanes.last().is_some_and(Option::is_none) {
            lanes.pop();
        }
        max_lanes = max_lanes.max(lanes.len()).max(lane + 1);
        rows.push(row);
    }

    GraphLayout {
        rows,
        max_lanes: max_lanes as u16,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(hash: &str, parents: &[&str]) -> CommitInfo {
        CommitInfo {
            hash: hash.into(),
            parents: parents.iter().map(|p| (*p).into()).collect(),
            author_name: "A".into(),
            author_email: "a@x".into(),
            author_time: 0,
            subject: hash.into(),
        }
    }

    fn seg(from: u16, to: u16) -> (u16, u16) {
        (from, to)
    }

    fn pairs(s: &[Segment]) -> Vec<(u16, u16)> {
        s.iter().map(|x| (x.from, x.to)).collect()
    }

    #[test]
    fn linear_history_stays_in_one_lane() {
        let g = layout(&[c("c", &["b"]), c("b", &["a"]), c("a", &[])]);
        assert_eq!(g.max_lanes, 1);
        assert!(g.rows.iter().all(|r| r.lane == 0));
        assert_eq!(pairs(&g.rows[0].top), vec![]);
        assert_eq!(pairs(&g.rows[0].bottom), vec![seg(0, 0)]);
        assert_eq!(pairs(&g.rows[1].top), vec![seg(0, 0)]);
        assert_eq!(pairs(&g.rows[2].bottom), vec![]);
        assert!(g.rows.iter().all(|r| r.color == g.rows[0].color));
    }

    #[test]
    fn merge_opens_and_closes_a_second_lane() {
        // m = merge(b, f); f branches from a.
        //  m
        //  |\
        //  b f
        //  |/
        //  a
        let g = layout(&[
            c("m", &["b", "f"]),
            c("b", &["a"]),
            c("f", &["a"]),
            c("a", &[]),
        ]);
        assert_eq!(g.max_lanes, 2);
        assert_eq!(pairs(&g.rows[0].bottom), vec![seg(0, 0), seg(0, 1)]);
        assert_eq!(g.rows[1].lane, 0);
        assert_eq!(pairs(&g.rows[1].bottom), vec![seg(0, 0), seg(1, 1)]);
        assert_eq!(g.rows[2].lane, 1);
        // f's parent `a` is already expected in lane 0, so f bends back into it
        // while lane 0 keeps running straight through.
        assert_eq!(pairs(&g.rows[2].bottom), vec![seg(1, 0), seg(0, 0)]);
        assert_eq!(g.rows[3].lane, 0);
        assert_eq!(pairs(&g.rows[3].top), vec![seg(0, 0)]);
        assert_ne!(g.rows[0].bottom[1].color, g.rows[0].color);
    }

    #[test]
    fn two_children_of_one_commit_converge_on_its_node() {
        // Two branch tips x and y both have parent a.
        let g = layout(&[c("x", &["a"]), c("y", &["a"]), c("a", &[])]);
        assert_eq!(g.rows[0].lane, 0);
        assert_eq!(g.rows[1].lane, 1);
        assert_eq!(pairs(&g.rows[1].bottom), vec![seg(1, 0), seg(0, 0)]);
        assert_eq!(g.rows[2].lane, 0);
    }

    #[test]
    fn free_lanes_are_reused() {
        // After branch f is merged, a later branch g reuses lane 1 instead of opening lane 2.
        let g = layout(&[
            c("m2", &["m1", "g"]),
            c("g", &["m1"]),
            c("m1", &["b", "f"]),
            c("f", &["b"]),
            c("b", &[]),
        ]);
        assert_eq!(g.max_lanes, 2);
        assert_eq!(g.rows[1].lane, 1);
        assert_eq!(g.rows[3].lane, 1);
    }

    #[test]
    fn missing_parents_end_at_the_bottom() {
        let g = layout(&[c("b", &["a-not-loaded"])]);
        assert_eq!(pairs(&g.rows[0].bottom), vec![]);
    }

    #[test]
    fn parses_nul_separated_log() {
        let raw = "aaa\u{1f}bbb ccc\u{1f}Nguyễn Văn A\u{1f}a@x.dev\u{1f}1700000000\u{1f}feat: xin chào, thế giới\0\n\
                   bbb\u{1f}\u{1f}B\u{1f}b@x\u{1f}1600000000\u{1f}root\0";
        let commits = parse_log(raw);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].parents, vec!["bbb", "ccc"]);
        assert_eq!(commits[0].author_name, "Nguyễn Văn A");
        assert_eq!(commits[0].subject, "feat: xin chào, thế giới");
        assert!(commits[1].parents.is_empty());
    }

    #[test]
    fn reads_a_real_repository() {
        let dir = tempfile::tempdir().unwrap();
        let git = GitRunner::default();
        let ctx = AccountContext {
            identity: Some(crate::Identity {
                name: "T".into(),
                email: "t@x".into(),
            }),
            ..Default::default()
        };
        let run = |args: &[&str]| git.run(dir.path(), &ctx, args).unwrap();
        run(&["init", "-q", "-b", "main"]);
        run(&["commit", "-q", "--allow-empty", "-m", "one"]);
        run(&["checkout", "-q", "-b", "topic"]);
        run(&["commit", "-q", "--allow-empty", "-m", "two"]);
        run(&["checkout", "-q", "main"]);
        run(&["commit", "-q", "--allow-empty", "-m", "three"]);
        run(&["merge", "-q", "--no-ff", "-m", "merge topic", "topic"]);
        std::fs::write(dir.path().join("wip.txt"), "wip").unwrap();
        run(&["add", "wip.txt"]);
        run(&["stash", "push", "-q", "-m", "wip"]);

        let commits = read_log(&git, dir.path(), None).unwrap();
        assert_eq!(commits.len(), 4);
        assert_eq!(commits[0].subject, "merge topic");
        assert_eq!(commits[0].parents.len(), 2);
        let g = layout(&commits);
        assert_eq!(g.rows.len(), 4);
        assert_eq!(g.max_lanes, 2);
    }
}
