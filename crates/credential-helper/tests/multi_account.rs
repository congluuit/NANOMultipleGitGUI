//! End-to-end proof of the multi-account design (PLAN §4.4) against a local Git + LFS server:
//! two accounts on the same host push, clone and transfer LFS objects side by side, each with
//! its own token served by `nanomgg-credential`, while a decoy helper in the "global" git config
//! would hand out a wrong password if our `credential.helper` reset did not work.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;

use nanomgg_credentials::{AccountCredential, CredentialServer, InMemorySource};
use nanomgg_git_engine::{AccountContext, GitRunner, HttpsAuth, Identity};
use nanomgg_test_git_server::{EventKind, TestGitServer};

const ALICE_TOKEN: &str = "alice-secret-token";
const BOB_TOKEN: &str = "bob-secret-token";

struct Harness {
    tmp: tempfile::TempDir,
    server: TestGitServer,
    creds: CredentialServer,
    git: GitRunner,
}

impl Harness {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let server = TestGitServer::start(
            &tmp.path().join("server"),
            &[("alice", ALICE_TOKEN), ("bob", BOB_TOKEN)],
        )
        .unwrap();
        server.create_repo("alice", "app").unwrap();
        server.create_repo("bob", "site").unwrap();

        let source = InMemorySource::default();
        for (id, user, token) in [
            ("acc-alice", "alice", ALICE_TOKEN),
            ("acc-bob", "bob", BOB_TOKEN),
        ] {
            source.insert(
                id,
                AccountCredential {
                    host: server.host(),
                    username: user.into(),
                    token: token.to_owned().into(),
                },
            );
        }
        let creds = CredentialServer::start(Arc::new(source)).unwrap();

        // Isolated "global" config with a decoy helper returning a wrong password.
        let global = tmp.path().join("global.gitconfig");
        std::fs::write(
            &global,
            "[credential]\n\thelper = \"!f() { echo username=alice; echo password=WRONG; }; f\"\n\
             [init]\n\tdefaultBranch = main\n",
        )
        .unwrap();
        let git = GitRunner::default()
            .with_env("GIT_CONFIG_NOSYSTEM", "1")
            .with_env("GIT_CONFIG_GLOBAL", global.to_string_lossy());

        Self {
            tmp,
            server,
            creds,
            git,
        }
    }

    fn ctx(&self, account: &str) -> AccountContext {
        let (name, email) = match account {
            "acc-alice" => ("Alice", "alice@acme.dev"),
            _ => ("Bob", "bob@personal.dev"),
        };
        AccountContext {
            identity: Some(Identity {
                name: name.into(),
                email: email.into(),
            }),
            https: Some(HttpsAuth {
                helper_path: PathBuf::from(env!("CARGO_BIN_EXE_nanomgg-credential")),
                env: self.creds.env_for(account),
            }),
            ssh: None,
        }
    }

    fn dir(&self, name: &str) -> PathBuf {
        let d = self.tmp.path().join(name);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Creates a repo with an LFS-tracked `asset.bin` of `lfs_bytes` and returns its LFS oid.
    fn make_lfs_repo(
        &self,
        dir: &Path,
        ctx: &AccountContext,
        lfs_bytes: usize,
        seed: u64,
    ) -> String {
        let g = |args: &[&str]| self.git.run(dir, ctx, args).unwrap();
        g(&["init", "-q", "-b", "main"]);
        g(&["lfs", "install", "--local"]);
        g(&["lfs", "track", "*.bin"]);
        std::fs::write(dir.join("README.md"), "multi-account test\n").unwrap();
        write_random(&dir.join("asset.bin"), lfs_bytes, seed);
        g(&["add", "."]);
        g(&["commit", "-q", "-m", "initial commit"]);
        let ls = self
            .git
            .run_text(dir, ctx, ["lfs", "ls-files", "-l"])
            .unwrap();
        ls.split_whitespace().next().unwrap().to_owned()
    }
}

fn write_random(path: &Path, len: usize, seed: u64) {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut x = seed | 1;
    let mut buf = vec![0u8; 1 << 20];
    let mut left = len;
    while left > 0 {
        for chunk in buf.chunks_mut(8) {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let bytes = x.to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
        let n = left.min(buf.len());
        f.write_all(&buf[..n]).unwrap();
        left -= n;
    }
}

fn git_lfs_available() -> bool {
    let ok = std::process::Command::new("git")
        .args(["lfs", "version"])
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        eprintln!("git-lfs not installed; skipping");
    }
    ok
}

#[test]
fn each_account_pushes_to_its_own_repo_with_lfs() {
    if !git_lfs_available() {
        return;
    }
    let h = Harness::new();
    for (account, owner, repo, seed) in [
        ("acc-alice", "alice", "app", 1u64),
        ("acc-bob", "bob", "site", 2u64),
    ] {
        let ctx = h.ctx(account);
        let dir = h.dir(&format!("work-{owner}"));
        let oid = h.make_lfs_repo(&dir, &ctx, 3 << 20, seed);
        h.git
            .run(
                &dir,
                &ctx,
                ["remote", "add", "origin", &h.server.url(owner, repo)],
            )
            .unwrap();
        h.git
            .run(&dir, &ctx, ["push", "-q", "-u", "origin", "main"])
            .unwrap();

        let email = h
            .git
            .run_text(&dir, &ctx, ["log", "-1", "--format=%ae"])
            .unwrap();
        assert_eq!(email, ctx.identity.as_ref().unwrap().email);

        let events = h.server.events();
        let full = format!("{owner}/{repo}");
        assert!(events.iter().any(|e| e.repo == full
            && e.user.as_deref() == Some(owner)
            && e.kind == EventKind::GitWrite));
        assert!(
            events.iter().any(|e| e.repo == full
                && e.user.as_deref() == Some(owner)
                && e.kind
                    == EventKind::LfsUpload {
                        oid: oid.clone(),
                        size: 3 << 20
                    }),
            "LFS object not uploaded by {owner}: {events:#?}"
        );
        assert!(h.server.lfs_object_path(owner, repo, &oid).exists());
    }
    let events = h.server.events();
    assert!(
        !events.iter().any(|e| e.kind == EventKind::Forbidden),
        "an account touched a repo it does not own: {events:#?}"
    );
}

#[test]
fn pushing_with_the_wrong_account_is_rejected() {
    let h = Harness::new();
    let bob = h.ctx("acc-bob");
    let dir = h.dir("work-wrong");
    let g = |args: &[&str]| h.git.run(&dir, &bob, args);
    g(&["init", "-q", "-b", "main"]).unwrap();
    std::fs::write(dir.join("a.txt"), "x").unwrap();
    g(&["add", "."]).unwrap();
    g(&["commit", "-q", "-m", "x"]).unwrap();
    let err = g(&["push", &h.server.url("alice", "app"), "main"]).unwrap_err();
    assert!(err.to_string().contains("403"), "{err}");

    let events = h.server.events();
    assert!(events.iter().any(|e| e.repo == "alice/app"
        && e.user.as_deref() == Some("bob")
        && e.kind == EventKind::Forbidden));
    assert!(!events.iter().any(|e| e.kind == EventKind::GitWrite));
}

#[test]
fn clone_and_lfs_pull_use_the_bound_account() {
    if !git_lfs_available() {
        return;
    }
    let h = Harness::new();
    let alice = h.ctx("acc-alice");
    let src = h.dir("src");
    let oid = h.make_lfs_repo(&src, &alice, 1 << 20, 7);
    h.git
        .run(
            &src,
            &alice,
            ["push", "-q", &h.server.url("alice", "app"), "main"],
        )
        .unwrap();
    h.server.clear_events();

    // "Download later" clone mode (PLAN §6.5): skip smudge, then pull LFS objects.
    let dest = h.tmp.path().join("clone");
    let status = h
        .git
        .command(
            h.tmp.path(),
            &alice,
            ["clone", "-q", &h.server.url("alice", "app")],
        )
        .arg(&dest)
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        .status()
        .unwrap();
    assert!(status.success());
    h.git
        .run(&dest, &alice, ["lfs", "install", "--local"])
        .unwrap();
    h.git.run(&dest, &alice, ["lfs", "pull"]).unwrap();
    assert_eq!(
        std::fs::read(dest.join("asset.bin")).unwrap(),
        std::fs::read(src.join("asset.bin")).unwrap()
    );
    assert!(
        h.server
            .events()
            .iter()
            .any(|e| e.user.as_deref() == Some("alice")
                && e.kind == EventKind::LfsDownload { oid: oid.clone() })
    );

    let bob_dest = h.tmp.path().join("clone-bob");
    let status = h
        .git
        .command(
            h.tmp.path(),
            &h.ctx("acc-bob"),
            ["clone", "-q", &h.server.url("alice", "app")],
        )
        .arg(&bob_dest)
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(
        !status.success(),
        "bob must not be able to clone alice's private repo"
    );
}

#[test]
fn helper_never_hands_tokens_to_other_hosts() {
    let h = Harness::new();
    let alice = h.ctx("acc-alice");
    let fill = |input: String| {
        let mut child = h
            .git
            .command(h.tmp.path(), &alice, ["credential", "fill"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    };

    let ok = fill(format!("protocol=http\nhost={}\n\n", h.server.host()));
    let out = String::from_utf8_lossy(&ok.stdout);
    assert!(ok.status.success());
    assert!(out.contains("username=alice"), "{out}");
    assert!(out.contains(&format!("password={ALICE_TOKEN}")), "{out}");

    for (protocol, host) in [
        ("https", "evil.example.com"),
        ("https", "lfs.thirdparty.io"),
        ("http", "github.com"),
    ] {
        let res = fill(format!("protocol={protocol}\nhost={host}\n\n"));
        let out = String::from_utf8_lossy(&res.stdout);
        assert!(
            !res.status.success(),
            "{protocol}://{host} unexpectedly succeeded"
        );
        assert!(!out.contains(ALICE_TOKEN), "token leaked to {host}: {out}");
        assert!(
            !out.contains("WRONG"),
            "decoy helper was consulted for {host}"
        );
    }
}

/// PLAN §14 prototype: two accounts on the same host, one of them pushing a 200 MB LFS file.
/// Slow, so ignored by default: `cargo test -p nanomgg-credential-helper -- --ignored --nocapture`.
#[test]
#[ignore]
fn large_lfs_file_200mb_two_accounts() {
    if !git_lfs_available() {
        return;
    }
    let h = Harness::new();
    const SIZE: usize = 200 << 20;

    let alice = h.ctx("acc-alice");
    let a = h.dir("big-alice");
    let started = Instant::now();
    let oid = h.make_lfs_repo(&a, &alice, SIZE, 42);
    let prepared = started.elapsed();
    let push_start = Instant::now();
    h.git
        .run(
            &a,
            &alice,
            ["push", "-q", &h.server.url("alice", "app"), "main"],
        )
        .unwrap();
    let push = push_start.elapsed();

    let bob = h.ctx("acc-bob");
    let b = h.dir("big-bob");
    h.make_lfs_repo(&b, &bob, 1 << 20, 43);
    h.git
        .run(
            &b,
            &bob,
            ["push", "-q", &h.server.url("bob", "site"), "main"],
        )
        .unwrap();

    let uploaded = h
        .server
        .lfs_object_path("alice", "app", &oid)
        .metadata()
        .unwrap()
        .len();
    assert_eq!(uploaded, SIZE as u64);
    assert!(
        h.server
            .events()
            .iter()
            .any(|e| e.user.as_deref() == Some("alice")
                && e.kind
                    == EventKind::LfsUpload {
                        oid: oid.clone(),
                        size: SIZE as u64
                    })
    );
    let mib = SIZE as f64 / (1024.0 * 1024.0);
    eprintln!(
        "200 MB LFS: commit prepared in {:.1}s, push (git + LFS upload) in {:.1}s = {:.0} MiB/s",
        prepared.as_secs_f64(),
        push.as_secs_f64(),
        mib / push.as_secs_f64()
    );

    // Re-pushing uploads nothing: objects are content-addressed, so an interrupted push only
    // needs to send what is missing (PLAN §6.4 "resume").
    h.server.clear_events();
    std::fs::write(a.join("note.txt"), "second commit").unwrap();
    h.git.run(&a, &alice, ["add", "."]).unwrap();
    h.git
        .run(&a, &alice, ["commit", "-q", "-m", "second"])
        .unwrap();
    h.git
        .run(
            &a,
            &alice,
            ["push", "-q", &h.server.url("alice", "app"), "main"],
        )
        .unwrap();
    assert!(
        !h.server
            .events()
            .iter()
            .any(|e| matches!(e.kind, EventKind::LfsUpload { .. }))
    );
}
