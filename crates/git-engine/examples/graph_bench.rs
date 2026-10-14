//! Measures history loading + lane layout on a repository.
//!
//!   node scripts/gen-history-fixture.mjs            # creates target/fixtures/graph-50k
//!   cargo run --release -p nanomgg-git-engine --example graph_bench -- target/fixtures/graph-50k

use std::path::PathBuf;
use std::time::Instant;

use nanomgg_git_engine::GitRunner;
use nanomgg_git_engine::graph::{layout, read_log};

fn main() {
    let repo = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/fixtures/graph-50k".into()),
    );
    let git = GitRunner::default();
    let runs = 5;
    let (mut read_ms, mut layout_ms) = (Vec::new(), Vec::new());
    let mut summary = (0, 0, 0);
    for _ in 0..runs {
        let t = Instant::now();
        let commits = read_log(&git, &repo, None).expect("git log");
        read_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        let t = Instant::now();
        let g = layout(&commits);
        layout_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        let segments: usize = g.rows.iter().map(|r| r.top.len() + r.bottom.len()).sum();
        summary = (commits.len(), g.max_lanes as usize, segments);
    }
    let median = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    println!(
        "{} commits, max {} lanes, {} segments | git log + parse: {:.0} ms | layout: {:.0} ms (median of {runs})",
        summary.0,
        summary.1,
        summary.2,
        median(&mut read_ms),
        median(&mut layout_ms)
    );
}
