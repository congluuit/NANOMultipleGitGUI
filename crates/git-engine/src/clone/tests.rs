use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nanomgg_test_git_server::TestGitServer;

use super::*;
use crate::{HttpsAuth, Identity};

const ALICE_TOKEN: &str = "alice-clone-token-0123";

/// Git isolated from the machine's config (no system LFS filter, no autocrlf, no helpers).
struct Env {
    dir: tempfile::TempDir,
    git: GitRunner,
}

impl Env {
    fn new() -> Self {
        Self::with_global("")
    }

    fn with_global(extra: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let global = dir.path().join("global.gitconfig");
        std::fs::write(
            &global,
            format!(
                "[core]\n\tautocrlf = false\n[init]\n\tdefaultBranch = main\n\
                 [user]\n\tname = Tester\n\temail = tester@example.com\n{extra}"
            ),
        )
        .unwrap();
        let git = GitRunner::default()
            .with_env("GIT_CONFIG_NOSYSTEM", "1")
            .with_env("GIT_CONFIG_GLOBAL", global.to_string_lossy());
        Self { dir, git }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn git(&self, cwd: &Path, ctx: &AccountContext, args: &[&str]) -> String {
        self.git.run_text(cwd, ctx, args).unwrap()
    }

    /// Work repo with `main` (README.md) and `dev` (dev.txt), pushed to a bare `origin.git`.
    fn seed_bare(&self) -> PathBuf {
        let work = self.path("work");
        let bare = self.path("origin.git");
        std::fs::create_dir_all(&work).unwrap();
        let none = AccountContext::default();
        self.git(&work, &none, &["init", "-q", "-b", "main"]);
        std::fs::write(work.join("README.md"), "hello\n").unwrap();
        self.git(&work, &none, &["add", "."]);
        self.git(&work, &none, &["commit", "-q", "-m", "one"]);
        std::fs::write(work.join("README.md"), "hello again\n").unwrap();
        self.git(&work, &none, &["commit", "-q", "-am", "two"]);
        self.git(&work, &none, &["switch", "-q", "-c", "dev"]);
        std::fs::write(work.join("dev.txt"), "dev\n").unwrap();
        self.git(&work, &none, &["add", "."]);
        self.git(&work, &none, &["commit", "-q", "-m", "dev"]);
        self.git(&work, &none, &["switch", "-q", "main"]);
        self.git(
            self.dir.path(),
            &none,
            &["clone", "-q", "--bare", "work", "origin.git"],
        );
        bare
    }

    fn opts(&self, url: &str, dest: &str) -> CloneOptions {
        CloneOptions {
            url: url.to_owned(),
            dest: self.path(dest),
            ..CloneOptions::default()
        }
    }
}

fn file_url(path: &Path) -> String {
    let p = path.to_string_lossy().replace('\\', "/");
    format!("file:///{}", p.trim_start_matches('/'))
}

fn none() -> AccountContext {
    AccountContext::default()
}

#[test]
fn clones_a_branch_with_progress() {
    let env = Env::new();
    let bare = env.seed_bare();
    let mut opts = env.opts(&file_url(&bare), "clone");
    opts.branch = Some("dev".into());
    let mut events = Vec::new();
    let out = clone(&env.git, &none(), &opts, &CancelToken::new(), |p| {
        events.push(p)
    })
    .unwrap();
    assert_eq!(out.path, env.path("clone"));
    assert!(!out.uses_lfs && !out.lfs_pending && out.lfs_error.is_none());
    assert_eq!(
        std::fs::read_to_string(out.path.join("dev.txt")).unwrap(),
        "dev\n"
    );
    assert_eq!(
        env.git(&out.path, &none(), &["branch", "--show-current"]),
        "dev"
    );
    assert!(
        events.iter().any(|p| p.done),
        "no finished progress phase: {events:?}"
    );
}

#[test]
fn shallow_clone_into_an_existing_empty_folder() {
    let env = Env::new();
    let bare = env.seed_bare();
    std::fs::create_dir_all(env.path("empty")).unwrap();
    let mut opts = env.opts(&file_url(&bare), "empty");
    opts.depth = Some(1);
    let out = clone(&env.git, &none(), &opts, &CancelToken::new(), |_| {}).unwrap();
    assert_eq!(
        env.git(&out.path, &none(), &["rev-list", "--count", "HEAD"]),
        "1"
    );
    assert_eq!(
        env.git(
            &out.path,
            &none(),
            &["rev-parse", "--is-shallow-repository"]
        ),
        "true"
    );
}

#[test]
fn rejects_unsafe_urls_without_creating_anything() {
    for url in [
        "",
        " https://x/y.git",
        "-uhoh",
        "--upload-pack=touch /tmp/pwned",
        "ext::sh -c touch% /tmp/pwned",
        "fd::17",
        "https://host/a\nb",
    ] {
        assert!(validate_url(url).is_err(), "{url:?} accepted");
    }
    for url in [
        "https://github.com/acme/app.git",
        "git@github.com:acme/app.git",
        "ssh://git@[::1]:2222/acme/app.git",
        "file:///C:/repos/app.git",
        r"C:\repos\app.git",
        "../app.git",
    ] {
        assert!(validate_url(url).is_ok(), "{url:?} rejected");
    }
    let env = Env::new();
    let err = clone(
        &env.git,
        &none(),
        &env.opts("-uhoh", "x"),
        &CancelToken::new(),
        |_| {},
    )
    .unwrap_err();
    assert_eq!(err.kind(), Some(FailureKind::InvalidInput));
    assert!(!env.path("x").exists());
}

#[test]
fn destination_must_be_missing_or_empty() {
    let env = Env::new();
    assert_eq!(
        check_destination(&env.path("new/deep")).unwrap(),
        Destination::Missing
    );
    std::fs::create_dir_all(env.path("empty")).unwrap();
    assert_eq!(
        check_destination(&env.path("empty")).unwrap(),
        Destination::EmptyDir
    );
    std::fs::create_dir_all(env.path("full")).unwrap();
    std::fs::write(env.path("full/keep.txt"), "mine").unwrap();
    std::fs::write(env.path("file"), "x").unwrap();
    for dest in ["full", "file"] {
        let err = check_destination(&env.path(dest)).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::AlreadyExists), "{dest}");
    }
    assert!(check_destination(Path::new("relative/dir")).is_err());

    // A clone into a non-empty folder leaves it untouched.
    let bare = env.seed_bare();
    let err = clone(
        &env.git,
        &none(),
        &env.opts(&file_url(&bare), "full"),
        &CancelToken::new(),
        |_| {},
    )
    .unwrap_err();
    assert_eq!(err.kind(), Some(FailureKind::AlreadyExists));
    assert_eq!(
        std::fs::read_to_string(env.path("full/keep.txt")).unwrap(),
        "mine"
    );
}

#[test]
fn failed_clone_removes_only_the_folders_it_created() {
    let env = Env::new();
    let missing = env.path("nope.git");
    // Nested destination: `a` and `a/b` do not exist yet.
    let err = clone(
        &env.git,
        &none(),
        &env.opts(&file_url(&missing), "a/b/c"),
        &CancelToken::new(),
        |_| {},
    )
    .unwrap_err();
    assert!(!matches!(err, OpError::Cancelled), "{err}");
    assert!(!env.path("a").exists(), "created parents left behind");
    assert!(env.dir.path().exists());

    // An existing empty folder is kept (and stays empty).
    std::fs::create_dir_all(env.path("mine")).unwrap();
    clone(
        &env.git,
        &none(),
        &env.opts(&file_url(&missing), "mine"),
        &CancelToken::new(),
        |_| {},
    )
    .unwrap_err();
    assert!(env.path("mine").is_dir());
    assert_eq!(std::fs::read_dir(env.path("mine")).unwrap().count(), 0);

    // Unknown branch: git fails after creating the repository; everything is removed.
    let bare = env.seed_bare();
    let mut opts = env.opts(&file_url(&bare), "branchless");
    opts.branch = Some("no-such-branch".into());
    let err = clone(&env.git, &none(), &opts, &CancelToken::new(), |_| {}).unwrap_err();
    assert_eq!(err.kind(), Some(FailureKind::NotFound), "{err}");
    assert!(!env.path("branchless").exists());
}

#[test]
fn cancel_removes_the_half_cloned_folder() {
    // A post-checkout hook (run by `git clone`) that blocks until the clone is cancelled.
    let hooks = tempfile::tempdir().unwrap();
    let marker = hooks.path().join("started");
    // Short and unique: a failing test cannot hang for long or touch other sleeps.
    let unique = format!("30.{}3", std::process::id());
    std::fs::write(
        hooks.path().join("post-checkout"),
        format!(
            "#!/bin/sh\ntouch '{}'\nsleep {unique}\n",
            marker.to_string_lossy().replace('\\', "/")
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            hooks.path().join("post-checkout"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    let env = Env::new();
    let bare = env.seed_bare();
    // Only the clone sees the hook (not the seeding above).
    let git = env.git.clone().with_env(
        "GIT_CONFIG_PARAMETERS",
        format!(
            "'core.hookspath'='{}'",
            hooks.path().to_string_lossy().replace('\\', "/")
        ),
    );
    let token = CancelToken::new();
    let watcher = {
        let token = token.clone();
        let marker = marker.clone();
        std::thread::spawn(move || {
            let started = Instant::now();
            while !marker.exists() && started.elapsed() < Duration::from_secs(60) {
                std::thread::sleep(Duration::from_millis(50));
            }
            token.cancel();
            marker.exists()
        })
    };
    let started = Instant::now();
    let err = clone(
        &git,
        &none(),
        &env.opts(&file_url(&bare), "half/clone"),
        &token,
        |_| {},
    )
    .unwrap_err();
    assert!(watcher.join().unwrap(), "the post-checkout hook never ran");
    assert!(matches!(err, OpError::Cancelled), "{err}");
    assert!(started.elapsed() < Duration::from_secs(60));
    assert!(!env.path("half").exists(), "half-cloned folder left behind");
}

#[test]
fn recurses_into_submodules() {
    let env = Env::with_global("[protocol \"file\"]\n\tallow = always\n");
    let none = none();
    let sub = env.path("sub");
    std::fs::create_dir_all(&sub).unwrap();
    env.git(&sub, &none, &["init", "-q", "-b", "main"]);
    std::fs::write(sub.join("lib.txt"), "lib\n").unwrap();
    env.git(&sub, &none, &["add", "."]);
    env.git(&sub, &none, &["commit", "-q", "-m", "lib"]);
    let bare = env.seed_bare();
    let work = env.path("work");
    env.git(
        &work,
        &none,
        &["submodule", "add", "-q", &file_url(&sub), "vendor/lib"],
    );
    env.git(&work, &none, &["commit", "-q", "-m", "submodule"]);
    env.git(&work, &none, &["push", "-q", &file_url(&bare), "main"]);

    let mut opts = env.opts(&file_url(&bare), "with-subs");
    opts.recurse_submodules = true;
    let out = clone(&env.git, &none, &opts, &CancelToken::new(), |_| {}).unwrap();
    assert_eq!(
        std::fs::read_to_string(out.path.join("vendor/lib/lib.txt")).unwrap(),
        "lib\n"
    );
}

// ----- HTTPS with account credentials (local test server) -----------------------------------

/// Credential helper script answering with alice's token (stands in for the sidecar).
fn helper_ctx(dir: &Path, token: &str) -> AccountContext {
    let script = dir.join("helper.sh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\ntest \"$1\" = get || exit 0\ncat >/dev/null\n\
             echo username=alice\necho password={token}\n"
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    AccountContext {
        identity: Some(Identity {
            name: "Alice".into(),
            email: "alice@acme.dev".into(),
        }),
        https: Some(HttpsAuth {
            helper_path: script,
            env: Vec::new(),
        }),
        ssh: None,
        signing: None,
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

struct ServerEnv {
    env: Env,
    server: TestGitServer,
    ctx: AccountContext,
}

impl ServerEnv {
    fn new() -> Self {
        let env = Env::new();
        let server = TestGitServer::start(&env.path("server"), &[("alice", ALICE_TOKEN)]).unwrap();
        server.create_repo("alice", "app").unwrap();
        let ctx = helper_ctx(env.dir.path(), ALICE_TOKEN);
        Self { env, server, ctx }
    }

    /// Pushes a work repo to alice/app; `lfs` adds two tracked `.bin` files.
    fn push_seed(&self, lfs: bool) {
        let work = self.env.path("seed");
        std::fs::create_dir_all(&work).unwrap();
        let g = |args: &[&str]| self.env.git(&work, &self.ctx, args);
        g(&["init", "-q", "-b", "main"]);
        if lfs {
            g(&["lfs", "install", "--local"]);
            g(&["lfs", "track", "*.bin"]);
            std::fs::create_dir_all(work.join("a")).unwrap();
            std::fs::create_dir_all(work.join("b")).unwrap();
            std::fs::write(work.join("a/one.bin"), b"first large file\n").unwrap();
            std::fs::write(work.join("b/two.bin"), b"second large file\n").unwrap();
        }
        std::fs::write(work.join("README.md"), "app\n").unwrap();
        g(&["add", "."]);
        g(&["commit", "-q", "-m", "seed"]);
        g(&["remote", "add", "origin", &self.server.url("alice", "app")]);
        g(&["push", "-q", "-u", "origin", "main"]);
    }

    fn clone(&self, dest: &str, lfs: LfsMode) -> Result<CloneOutcome, OpError> {
        let mut opts = self.env.opts(&self.server.url("alice", "app"), dest);
        opts.lfs = lfs;
        clone(&self.env.git, &self.ctx, &opts, &CancelToken::new(), |_| {})
    }
}

fn is_pointer(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .unwrap()
        .starts_with("version https://git-lfs.github.com/spec/v1")
}

#[test]
fn https_clone_uses_the_account_credentials() {
    let s = ServerEnv::new();
    s.push_seed(false);
    let out = s.clone("ok", LfsMode::All).unwrap();
    assert_eq!(
        std::fs::read_to_string(out.path.join("README.md")).unwrap(),
        "app\n"
    );
    // The helper is injected through the environment only.
    let config = std::fs::read_to_string(out.path.join(".git/config")).unwrap();
    assert!(!config.contains("helper") && !config.contains(ALICE_TOKEN));

    // Without credentials (or with a wrong token) the server refuses; nothing is left behind.
    let mut opts = s.env.opts(&s.server.url("alice", "app"), "denied");
    let err = clone(&s.env.git, &none(), &opts, &CancelToken::new(), |_| {}).unwrap_err();
    assert_eq!(err.kind(), Some(FailureKind::AuthFailed), "{err}");
    assert!(!s.env.path("denied").exists());
    opts.dest = s.env.path("wrong");
    let other = s.env.path("other-helper");
    std::fs::create_dir_all(&other).unwrap();
    let wrong = helper_ctx(&other, "not-the-token");
    clone(&s.env.git, &wrong, &opts, &CancelToken::new(), |_| {}).unwrap_err();
    assert!(!s.env.path("wrong").exists());
}

#[test]
fn lfs_download_now_later_or_partially() {
    if !git_lfs_available() {
        return;
    }
    let s = ServerEnv::new();
    s.push_seed(true);

    let later = s.clone("later", LfsMode::Later).unwrap();
    assert!(later.uses_lfs && later.lfs_pending);
    assert!(is_pointer(&later.path.join("a/one.bin")));
    assert!(is_pointer(&later.path.join("b/two.bin")));
    // The filter was not configured globally: the clone installed it locally.
    assert_eq!(
        s.env.git(
            &later.path,
            &none(),
            &["config", "--local", "filter.lfs.process"]
        ),
        "git-lfs filter-process"
    );

    let partial = s
        .clone(
            "partial",
            LfsMode::Partial {
                include: "a/**".into(),
                exclude: Some(" ".into()),
            },
        )
        .unwrap();
    assert!(!partial.lfs_pending);
    assert_eq!(partial.lfs_error, None);
    assert_eq!(
        std::fs::read(partial.path.join("a/one.bin")).unwrap(),
        b"first large file\n"
    );
    assert!(is_pointer(&partial.path.join("b/two.bin")));
    assert_eq!(
        s.env
            .git(&partial.path, &none(), &["config", "lfs.fetchinclude"]),
        "a/**"
    );

    let all = s.clone("all", LfsMode::All).unwrap();
    assert!(!all.lfs_pending);
    assert_eq!(
        std::fs::read(all.path.join("b/two.bin")).unwrap(),
        b"second large file\n"
    );

    let err = s
        .clone(
            "bad-pattern",
            LfsMode::Partial {
                include: "--all".into(),
                exclude: None,
            },
        )
        .unwrap_err();
    assert_eq!(err.kind(), Some(FailureKind::InvalidInput));
    assert!(!s.env.path("bad-pattern").exists());
}
