//! Runs the multi-user test server until Ctrl+C, for manual checks of the desktop app.
//!
//! `cargo run -p nanomgg-test-git-server --example serve -- <root dir>`
//! Users `alice`/`alice-token` and `bob`/`bob-token`, each owning `<user>/demo.git`.
//! Pushes are logged as they happen so you can see which account did what.

use std::path::PathBuf;
use std::time::Duration;

use nanomgg_test_git_server::TestGitServer;

fn main() -> std::io::Result<()> {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("nanomgg-test-server"));
    let server = TestGitServer::start(&root, &[("alice", "alice-token"), ("bob", "bob-token")])?;
    for user in ["alice", "bob"] {
        server.create_repo(user, "demo")?;
        println!("{user}: {}", server.url(user, "demo"));
    }
    println!("host: {}", server.host());
    let mut seen = 0;
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let events = server.events();
        for e in &events[seen..] {
            println!("{e:?}");
        }
        seen = events.len();
    }
}
