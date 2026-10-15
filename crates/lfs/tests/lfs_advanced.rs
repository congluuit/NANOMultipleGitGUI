//! Integration tests of LFS files / maintenance / lock helpers against real repositories.
//! They need git-lfs (Git for Windows ships it); without it they print a note and pass.
//! The test git server has no LFS lock API, so lock / unlock against a server are covered by
//! parser tests in `locks.rs`; here a `file://` remote checks the "unsupported" translation.

use std::path::{Path, PathBuf};

use nanomgg_git_engine::{AccountContext, GitRunner, Identity};
use nanomgg_lfs::locks::{self, LockError, LockFailureKind};
use nanomgg_lfs::maintenance::{self, FsckProblemKind};
use nanomgg_lfs::{detect, files};

struct Repo {
    _dir: tempfile::TempDir,
    base: PathBuf,
    path: PathBuf,
    git: GitRunner,
    ctx: AccountContext,
}

impl Repo {
    fn new() -> Option<Self> {
        let git = GitRunner::default();
        if detect::lfs_tool(&git, &std::env::temp_dir())
            .version
            .is_none()
        {
            eprintln!("git-lfs not installed: skipping");
            return None;
        }
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().to_owned();
        let path = base.join("work");
        std::fs::create_dir_all(&path).unwrap();
        let repo = Self {
            _dir: dir,
            base,
            path,
            git,
            ctx: AccountContext {
                identity: Some(Identity {
                    name: "LFS Tester".into(),
                    email: "lfs@x.dev".into(),
                }),
                ..Default::default()
            },
        };
        repo.run(&["init", "-q", "-b", "main"]);
        repo.run(&["config", "core.autocrlf", "false"]);
        repo.run(&["lfs", "install", "--local"]);
        Some(repo)
    }

    fn run(&self, args: &[&str]) -> String {
        self.git.run_text(&self.path, &self.ctx, args).unwrap()
    }

    fn write(&self, name: &str, content: &[u8]) {
        let p = self.path.join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        // Lockable files are read-only after checkout.
        if p.exists() {
            let mut perm = std::fs::metadata(&p).unwrap().permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            std::fs::set_permissions(&p, perm).unwrap();
        }
        std::fs::write(p, content).unwrap();
    }

    fn commit(&self, msg: &str) {
        self.run(&["add", "-A"]);
        self.run(&["commit", "-q", "-m", msg]);
    }
}

fn bytes(n: usize, seed: u8) -> Vec<u8> {
    (0..n)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

fn url_of(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

#[test]
fn files_lockable_patterns_and_pointer_state() {
    let Some(r) = Repo::new() else { return };
    r.run(&["lfs", "track", "*.bin"]);
    r.run(&["lfs", "track", "--lockable", "*.psd"]);
    r.write("a.bin", &bytes(1000, 1));
    r.write("art/sp ace.psd", &bytes(2000, 2));
    r.write("plain.txt", b"text\n");
    r.commit("lfs files");

    let list = files::list(&r.git, &r.path, &r.ctx).unwrap();
    assert_eq!(list.len(), 2, "{list:?}");
    let a = list.iter().find(|f| f.path == "a.bin").unwrap();
    assert!(a.downloaded && a.checked_out && !a.lockable);
    assert_eq!(a.size, 1000);
    let psd = list.iter().find(|f| f.path == "art/sp ace.psd").unwrap();
    assert!(psd.lockable);

    // Turn the .psd pattern into a normal one and back.
    assert!(locks::set_lockable(&r.git, &r.path, "*.psd", false).unwrap());
    let attrs = std::fs::read_to_string(r.path.join(".gitattributes")).unwrap();
    assert!(!attrs.contains("lockable"), "{attrs}");
    assert!(locks::set_lockable(&r.git, &r.path, "*.psd", true).unwrap());
    assert!(
        !locks::set_lockable(&r.git, &r.path, "*.psd", true).unwrap(),
        "no change"
    );

    // A clone without smudge has pointers only.
    let clone = r.base.join("clone");
    let out = r
        .git
        .command(
            &r.base,
            &AccountContext::default(),
            ["clone", "-q", &url_of(&r.path), &url_of(&clone)],
        )
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let list = files::list(&r.git, &clone, &AccountContext::default()).unwrap();
    assert_eq!(list.len(), 2);
    assert!(
        list.iter().all(|f| f.is_pointer() && !f.downloaded),
        "{list:?}"
    );
}

#[test]
fn storage_prune_preview_prune_and_fsck() {
    let Some(r) = Repo::new() else { return };
    r.run(&["lfs", "track", "*.bin"]);
    for (i, n) in [1000usize, 1500, 1700].into_iter().enumerate() {
        r.write("a.bin", &bytes(n, i as u8));
        r.commit(&format!("v{i}"));
    }
    let s = maintenance::storage(&r.git, &r.path);
    assert_eq!(s.objects, 3);
    assert_eq!(s.bytes, 1000 + 1500 + 1700);

    // Nothing is pushed: everything is retained.
    let p = maintenance::prune_preview(&r.git, &r.path, &r.ctx, false).unwrap();
    assert_eq!(p.local_objects, 3);
    assert!(p.objects.is_empty(), "{p:?}");

    // Push to a bare remote (git-lfs uploads to a local path through its standalone agent),
    // then old versions may go.
    let remote = r.base.join("remote.git");
    r.git
        .run(
            &r.base,
            &AccountContext::default(),
            ["init", "-q", "--bare", &url_of(&remote)],
        )
        .unwrap();
    r.run(&["remote", "add", "origin", &url_of(&remote)]);
    r.run(&["push", "-q", "origin", "main"]);
    r.run(&["config", "lfs.fetchrecentcommitsdays", "0"]);
    r.run(&["config", "lfs.fetchrecentrefsdays", "0"]);
    r.run(&["config", "lfs.pruneoffsetdays", "0"]);
    let p = maintenance::prune_preview(&r.git, &r.path, &r.ctx, false).unwrap();
    assert!(!p.objects.is_empty(), "{p:?}");
    assert!(p.bytes > 0);
    let done = maintenance::prune(&r.git, &r.path, &r.ctx, false).unwrap();
    assert_eq!(done.bytes, p.bytes, "{done:?}");
    let after = maintenance::storage(&r.git, &r.path);
    assert_eq!(after.bytes, s.bytes - done.bytes);

    let ok = maintenance::fsck(&r.git, &r.path, true, true).unwrap();
    assert!(ok.ok, "{ok:?}");

    // Delete the checked-out object: fsck reports it as missing (and does not move anything).
    let media = PathBuf::from(after.media_dir.unwrap());
    let current = files::list(&r.git, &r.path, &r.ctx).unwrap()[0].oid.clone();
    std::fs::remove_file(
        media
            .join(&current[0..2])
            .join(&current[2..4])
            .join(&current),
    )
    .unwrap();
    let bad = maintenance::fsck(&r.git, &r.path, true, false).unwrap();
    assert!(!bad.ok);
    assert_eq!(bad.problems[0].kind, FsckProblemKind::Missing, "{bad:?}");
    assert_eq!(bad.problems[0].path.as_deref(), Some("a.bin"));

    // The file remote has no lock API: a recognisable "unsupported".
    match locks::lock(&r.git, &r.path, &r.ctx, None, "a.bin") {
        Err(LockError::Failed(f)) => assert_eq!(f.kind, LockFailureKind::Unsupported, "{f:?}"),
        other => panic!("expected an unsupported failure, got {other:?}"),
    }
    match locks::list(&r.git, &r.path, &r.ctx, None, Some("me")) {
        Ok(l) => assert!(l.locks.is_empty()),
        Err(LockError::Failed(f)) => assert_eq!(f.kind, LockFailureKind::Unsupported, "{f:?}"),
        Err(e) => panic!("{e}"),
    }
}
