//! LFS transfers against the local multi-user Git + LFS test server: estimate, upload with
//! progress, cancel + resume, single-file pull, `fetch --all`, and a wrong-account 403.
//!
//! Credentials come from the [`AccountContext`] (a tiny `sh` credential helper reading the
//! account's token from the context's environment), while the isolated "global" config holds a
//! decoy helper: if git-lfs did not inherit the context, every request would fail.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use nanomgg_git_engine::progress::CancelToken;
use nanomgg_git_engine::{AccountContext, GitRunner, HttpsAuth, Identity};
use nanomgg_lfs::transfer::{
    self, DownloadRequest, LfsFailureKind, LfsTransferStats, TransferError, TransferOptions,
};
use nanomgg_test_git_server::{EventKind, TestGitServer};

const MIB: usize = 1 << 20;

struct Harness {
    tmp: tempfile::TempDir,
    server: TestGitServer,
    git: GitRunner,
    helper: PathBuf,
}

fn lfs_available() -> bool {
    let ok = std::process::Command::new("git")
        .args(["lfs", "version"])
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        eprintln!("git-lfs not installed; skipping");
    }
    ok
}

impl Harness {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let server = TestGitServer::start(
            &tmp.path().join("server"),
            &[("alice", "alice-token"), ("bob", "bob-token")],
        )
        .unwrap();
        server.create_repo("alice", "game").unwrap();
        let global = tmp.path().join("global.gitconfig");
        std::fs::write(
            &global,
            "[credential]\n\thelper = \"!f() { echo username=alice; echo password=WRONG; }; f\"\n\
             [core]\n\tautocrlf = false\n[init]\n\tdefaultBranch = main\n",
        )
        .unwrap();
        let helper = tmp.path().join("helper.sh");
        std::fs::write(
            &helper,
            "#!/bin/sh\ntest \"$1\" = get || exit 0\n\
             echo \"username=$TEST_ACCOUNT_USER\"\necho \"password=$TEST_ACCOUNT_TOKEN\"\n",
        )
        .unwrap();
        let git = GitRunner::default()
            .with_env("GIT_CONFIG_NOSYSTEM", "1")
            .with_env("GIT_CONFIG_GLOBAL", global.to_string_lossy());
        Self {
            tmp,
            server,
            git,
            helper,
        }
    }

    fn ctx(&self, user: &str) -> AccountContext {
        AccountContext {
            identity: Some(Identity {
                name: user.into(),
                email: format!("{user}@test.dev"),
            }),
            https: Some(HttpsAuth {
                helper_path: self.helper.clone(),
                env: vec![
                    ("TEST_ACCOUNT_USER".into(), user.into()),
                    ("TEST_ACCOUNT_TOKEN".into(), format!("{user}-token")),
                ],
            }),
            ssh: None,
        }
    }

    fn run(&self, dir: &Path, ctx: &AccountContext, args: &[&str]) -> String {
        self.git.run_text(dir, ctx, args).unwrap()
    }

    /// A repo with LFS-tracked files of the given sizes (`f0.bin`, `f1.bin`…), committed and
    /// with `origin` pointing at alice/game. Returns the repo path and oids in file order.
    fn lfs_repo(
        &self,
        name: &str,
        ctx: &AccountContext,
        sizes: &[usize],
    ) -> (PathBuf, Vec<String>) {
        let dir = self.tmp.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        self.run(&dir, ctx, &["init", "-q", "-b", "main"]);
        self.run(&dir, ctx, &["lfs", "install", "--local"]);
        self.run(&dir, ctx, &["lfs", "track", "*.bin"]);
        for (i, size) in sizes.iter().enumerate() {
            write_random(&dir.join(format!("f{i}.bin")), *size, 2 * i as u64 + 7);
        }
        std::fs::write(dir.join("README.md"), "lfs transfer test\n").unwrap();
        self.run(&dir, ctx, &["add", "."]);
        self.run(&dir, ctx, &["commit", "-q", "-m", "assets"]);
        self.run(
            &dir,
            ctx,
            &["remote", "add", "origin", &self.server.url("alice", "game")],
        );
        let ls = self.run(&dir, ctx, &["lfs", "ls-files", "-l"]);
        let mut by_name: Vec<(String, String)> = ls
            .lines()
            .map(|l| {
                let mut parts = l.splitn(3, ' ');
                let oid = parts.next().unwrap().to_owned();
                let name = parts.nth(1).unwrap().to_owned();
                (name, oid)
            })
            .collect();
        by_name.sort();
        (dir, by_name.into_iter().map(|(_, oid)| oid).collect())
    }

    fn uploads(&self) -> Vec<String> {
        self.server
            .events()
            .into_iter()
            .filter_map(|e| match e.kind {
                EventKind::LfsUpload { oid, .. } if e.user.as_deref() == Some("alice") => Some(oid),
                _ => None,
            })
            .collect()
    }
}

fn write_random(path: &Path, len: usize, seed: u64) {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut x = seed | 1;
    let mut buf = vec![0u8; MIB];
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

#[test]
fn estimate_then_upload_with_progress_as_the_bound_account() {
    if !lfs_available() {
        return;
    }
    let h = Harness::new();
    let ctx = h.ctx("alice");
    let sizes = [3 * MIB, 5 * MIB, 1000];
    let (repo, oids) = h.lfs_repo("work", &ctx, &sizes);

    let est = transfer::push_estimate(&h.git, &repo, &ctx, "origin", "main").unwrap();
    assert_eq!(est.objects.len(), 3);
    assert_eq!(est.unknown_sizes, 0);
    assert_eq!(est.total_bytes, sizes.iter().sum::<usize>() as u64);
    let f1 = est.objects.iter().find(|o| o.path == "f1.bin").unwrap();
    assert_eq!(
        (f1.oid.as_str(), f1.size),
        (oids[1].as_str(), 5 * MIB as u64)
    );

    let mut seen: Vec<LfsTransferStats> = Vec::new();
    let summary = transfer::push(
        &h.git,
        &repo,
        &ctx,
        "origin",
        "main",
        &TransferOptions {
            concurrent_transfers: Some(1),
            ..Default::default()
        },
        Some(&est),
        &CancelToken::new(),
        |s| seen.push(s.clone()),
    )
    .unwrap();
    assert_eq!(summary.files, 3);
    assert_eq!(summary.bytes, est.total_bytes);
    let last = seen.last().unwrap();
    assert!(last.done);
    assert_eq!((last.files_done, last.files_total), (3, 3));
    assert_eq!(last.bytes_done, est.total_bytes);
    assert!(seen.len() >= 2, "progress while running: {}", seen.len());
    assert!(
        seen.windows(2).all(|w| w[0].bytes_done <= w[1].bytes_done),
        "bytes never go backwards"
    );
    let mut uploaded = h.uploads();
    uploaded.sort();
    let mut expected = oids.clone();
    expected.sort();
    assert_eq!(uploaded, expected, "every object uploaded once, by alice");
    assert!(
        !h.server.events().iter().any(|e| matches!(
            e.kind,
            EventKind::Forbidden | EventKind::Unauthorized
        ) && e.user.is_some()),
        "the decoy helper was never used"
    );

    // Second push: the server has everything; git-lfs skips the objects.
    h.server.clear_events();
    transfer::push(
        &h.git,
        &repo,
        &ctx,
        "origin",
        "main",
        &TransferOptions::default(),
        None,
        &CancelToken::new(),
        |_| {},
    )
    .unwrap();
    assert!(h.uploads().is_empty());
}

#[test]
fn cancel_mid_upload_then_rerun_uploads_only_the_rest() {
    if !lfs_available() {
        return;
    }
    let h = Harness::new();
    let ctx = h.ctx("alice");
    let (repo, oids) = h.lfs_repo("work", &ctx, &[16 * MIB, 24 * MIB, 24 * MIB]);
    let opts = TransferOptions {
        concurrent_transfers: Some(1),
        ..Default::default()
    };
    let est = transfer::push_estimate(&h.git, &repo, &ctx, "origin", "main").unwrap();

    let token = CancelToken::new();
    let cancelled_at = Arc::new(Mutex::new(None::<LfsTransferStats>));
    let fired = AtomicBool::new(false);
    let err = transfer::push(
        &h.git,
        &repo,
        &ctx,
        "origin",
        "main",
        &opts,
        Some(&est),
        &token,
        |s| {
            // Cancel as soon as one file is complete and the next one has started.
            if s.files_done >= 1 && !s.active.is_empty() && !fired.swap(true, Ordering::SeqCst) {
                *cancelled_at.lock().unwrap() = Some(s.clone());
                token.cancel();
            }
        },
    )
    .unwrap_err();
    assert!(matches!(err, TransferError::Cancelled), "{err}");
    let at = cancelled_at
        .lock()
        .unwrap()
        .clone()
        .expect("cancelled mid-way");
    assert!(at.bytes_done < at.bytes_total, "{at:?}");

    // Give the server a moment to notice the aborted request.
    std::thread::sleep(std::time::Duration::from_millis(500));
    let first = h.uploads();
    assert!(
        !first.is_empty() && first.len() < oids.len(),
        "partial upload: {first:?}"
    );
    for oid in &first {
        assert!(h.server.lfs_object_path("alice", "game", oid).exists());
    }

    h.server.clear_events();
    let mut last = None;
    transfer::push(
        &h.git,
        &repo,
        &ctx,
        "origin",
        "main",
        &opts,
        Some(&est),
        &CancelToken::new(),
        |s| last = Some(s.clone()),
    )
    .unwrap();
    let second = h.uploads();
    assert!(
        second.iter().all(|oid| !first.contains(oid)),
        "already uploaded objects were sent again: first={first:?} second={second:?}"
    );
    let mut all: Vec<String> = first.iter().chain(&second).cloned().collect();
    all.sort();
    let mut expected = oids.clone();
    expected.sort();
    assert_eq!(all, expected, "every object on the server exactly once");
    assert!(last.unwrap().done);
}

#[test]
fn pull_one_pointer_then_fetch_everything() {
    if !lfs_available() {
        return;
    }
    let h = Harness::new();
    let ctx = h.ctx("alice");
    let sizes = [2 * MIB, 3 * MIB];
    let (repo, oids) = h.lfs_repo("work", &ctx, &sizes);
    h.run(&repo, &ctx, &["push", "-q", "-u", "origin", "main"]);

    // "Download later": clone without smudging, so every LFS file is a pointer.
    let clone = h.tmp.path().join("clone");
    let out = h
        .git
        .command(
            h.tmp.path(),
            &ctx,
            ["clone", "-q", &h.server.url("alice", "game"), "clone"],
        )
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let pointer = std::fs::read_to_string(clone.join("f1.bin")).unwrap();
    assert!(pointer.starts_with("version https://git-lfs.github.com/spec/v1"));

    let one = DownloadRequest {
        include: Some("f1.bin".into()),
        ..Default::default()
    };
    let est = transfer::download_estimate(&h.git, &clone, &ctx, &one).unwrap();
    assert_eq!(est.objects.len(), 1);
    assert_eq!(est.objects[0].oid, oids[1]);
    assert_eq!(est.missing_bytes, 3 * MIB as u64);
    assert_eq!(est.needed_bytes, 6 * MIB as u64);
    assert!(est.enough_space && est.free_bytes.is_some());

    h.server.clear_events();
    let mut last = None;
    let summary = transfer::pull(
        &h.git,
        &clone,
        &ctx,
        Some("f1.bin"),
        &TransferOptions::default(),
        &CancelToken::new(),
        |s| last = Some(s.clone()),
    )
    .unwrap();
    assert_eq!(summary.files, 1);
    let last = last.unwrap();
    assert!(last.done && last.bytes_done == 3 * MIB as u64, "{last:?}");
    // f1.bin is real content now, f0.bin still a pointer.
    assert_eq!(
        std::fs::metadata(clone.join("f1.bin")).unwrap().len(),
        3 * MIB as u64
    );
    assert!(
        std::fs::read_to_string(clone.join("f0.bin"))
            .unwrap()
            .starts_with("version ")
    );
    let downloads: Vec<_> = h
        .server
        .events()
        .into_iter()
        .filter(|e| matches!(e.kind, EventKind::LfsDownload { .. }))
        .collect();
    assert_eq!(downloads.len(), 1);
    assert_eq!(downloads[0].user.as_deref(), Some("alice"));

    // `fetch --all`: the remaining object lands in the store.
    let all = DownloadRequest {
        all: true,
        ..Default::default()
    };
    let est = transfer::download_estimate(&h.git, &clone, &ctx, &all).unwrap();
    assert_eq!(est.objects.len(), 1);
    assert_eq!(est.objects[0].oid, oids[0]);
    transfer::fetch_all(
        &h.git,
        &clone,
        &ctx,
        &TransferOptions::default(),
        &CancelToken::new(),
        |_| {},
    )
    .unwrap();
    let est = transfer::download_estimate(&h.git, &clone, &ctx, &all).unwrap();
    assert!(est.objects.is_empty() && est.missing_bytes == 0);
}

#[test]
fn wrong_account_gets_a_translated_permission_error() {
    if !lfs_available() {
        return;
    }
    let h = Harness::new();
    let alice = h.ctx("alice");
    let (repo, _) = h.lfs_repo("work", &alice, &[MIB]);
    let bob = h.ctx("bob");
    let err = transfer::push(
        &h.git,
        &repo,
        &bob,
        "origin",
        "main",
        &TransferOptions {
            max_retries: Some(0),
            ..Default::default()
        },
        None,
        &CancelToken::new(),
        |_| {},
    )
    .unwrap_err();
    let TransferError::Failed(failure) = err else {
        panic!("expected a translated failure, got {err}");
    };
    assert_eq!(
        failure.kind,
        LfsFailureKind::NoPermission,
        "{}",
        failure.message
    );
    assert!(failure.message.contains("Authorization error"));
    assert!(!failure.retryable);
    assert!(h.uploads().is_empty());
    assert!(
        h.server
            .events()
            .iter()
            .any(|e| e.user.as_deref() == Some("bob") && e.kind == EventKind::Forbidden)
    );

    // Invalid arguments never reach git-lfs.
    assert!(matches!(
        transfer::push_estimate(&h.git, &repo, &alice, "--upload-pack=x", "main"),
        Err(TransferError::Invalid(_))
    ));
    assert!(matches!(
        transfer::push_estimate(&h.git, &repo, &alice, "nope", "main"),
        Err(TransferError::Invalid(_))
    ));
}
