//! Integration tests against real repositories. They need git-lfs (Git for Windows ships it);
//! when `git lfs version` fails they print a note and pass.

use std::path::{Path, PathBuf};

use nanomgg_git_engine::{AccountContext, GitRunner, Identity};
use nanomgg_lfs::guard::{self, GuardLevel, GuardThresholds, MIB};
use nanomgg_lfs::{LfsError, detect, setup};

struct Repo {
    _dir: tempfile::TempDir,
    path: PathBuf,
    git: GitRunner,
    ctx: AccountContext,
}

impl Repo {
    /// `None` (test skipped) when git-lfs is unavailable.
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
        let path = dir.path().join("work");
        std::fs::create_dir_all(&path).unwrap();
        let repo = Self {
            _dir: dir,
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
        Some(repo)
    }

    fn run(&self, args: &[&str]) -> String {
        self.git.run_text(&self.path, &self.ctx, args).unwrap()
    }

    fn write(&self, name: &str, content: &[u8]) {
        let p = self.path.join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    fn root(&self) -> &Path {
        &self.path
    }
}

fn binary(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

#[test]
fn detect_install_track_and_untrack() {
    let Some(r) = Repo::new() else { return };
    let git = &r.git;

    let s = detect::detect(git, r.root()).unwrap();
    assert!(s.git_lfs.version.is_some());
    assert!(!s.uses_lfs && s.patterns.is_empty());
    assert!(!s.hooks_installed);
    assert!(!s.needs_install, "repo does not use LFS yet");
    assert!(s.hooks_dir.ends_with(".git/hooks"), "{}", s.hooks_dir);

    // A freshly cloned LFS repo: attributes present (root and nested, untracked too), no hooks.
    r.write(
        ".gitattributes",
        b"*.psd filter=lfs diff=lfs merge=lfs -text
*.txt text
",
    );
    r.write(
        "assets/video/.gitattributes",
        b"*.mp4 filter=lfs diff=lfs merge=lfs -text
",
    );
    let s = detect::detect(git, r.root()).unwrap();
    assert!(s.uses_lfs);
    assert!(s.needs_install, "uses LFS but no hooks yet: {s:?}");
    assert!(s.hooks.iter().all(|h| !h.exists));

    setup::install_local(git, r.root()).unwrap();
    let s = detect::detect(git, r.root()).unwrap();
    assert!(s.hooks_installed, "{:?}", s.hooks);
    assert!(s.filter_configured);
    assert!(!s.needs_install);
    // --local: the filter is written to this repository's config.
    let local = r.run(&["config", "--local", "--get", "filter.lfs.process"]);
    assert!(local.contains("git-lfs"), "{local}");

    // Track: changes .gitattributes once.
    assert!(
        !setup::track(git, r.root(), "*.psd", false).unwrap(),
        "already tracked"
    );
    assert!(setup::track(git, r.root(), "*.fbx", true).unwrap());
    assert!(!setup::track(git, r.root(), "*.fbx", true).unwrap());
    assert!(setup::track(git, r.root(), "Art Files/*.tga", false).unwrap());
    // Option-like patterns never reach git.
    assert!(matches!(
        setup::track(git, r.root(), "--global", false),
        Err(LfsError::InvalidPattern(_))
    ));

    let s = detect::detect(git, r.root()).unwrap();
    let list: Vec<(&str, &str, bool)> = s
        .patterns
        .iter()
        .map(|p| (p.file.as_str(), p.pattern.as_str(), p.lockable))
        .collect();
    assert_eq!(
        list,
        [
            (".gitattributes", "*.psd", false),
            (".gitattributes", "*.fbx", true),
            (".gitattributes", "Art[[:space:]]Files/*.tga", false),
            ("assets/video/.gitattributes", "*.mp4", false),
        ]
    );

    assert!(setup::untrack(git, r.root(), "*.psd").unwrap());
    assert!(!setup::untrack(git, r.root(), "*.psd").unwrap());
    let s = detect::detect(git, r.root()).unwrap();
    assert!(!s.patterns.iter().any(|p| p.pattern == "*.psd"));
}

#[test]
fn hooks_path_is_respected() {
    let Some(r) = Repo::new() else { return };
    r.run(&["config", "core.hooksPath", "tools/hooks"]);
    std::fs::create_dir_all(r.path.join("tools/hooks")).unwrap();
    r.write("tools/hooks/pre-push", b"#!/bin/sh\nnpm test\n");
    let s = detect::detect(&r.git, r.root()).unwrap();
    assert!(s.hooks_dir.ends_with("tools/hooks"), "{}", s.hooks_dir);
    assert!(s.hooks[0].exists && !s.hooks[0].lfs, "foreign pre-push");
    assert!(!s.hooks_installed);
    // git-lfs refuses to overwrite a foreign hook: surfaced as an error, not hidden.
    assert!(setup::install_local(&r.git, r.root()).is_err());
}

#[test]
fn large_file_guard_on_a_real_repository() {
    let Some(r) = Repo::new() else { return };
    let git = &r.git;
    setup::install_local(git, r.root()).unwrap();

    // Committed before tracking: full blob in the index.
    r.write("old.psd", &binary(3 * MIB as usize));
    r.write("README.md", b"# hi\n");
    r.run(&["add", "."]);
    r.run(&["commit", "-q", "-m", "base"]);

    assert!(setup::track(git, r.root(), "*.bin", false).unwrap());
    assert!(setup::track(git, r.root(), "*.psd", false).unwrap());
    r.write("assets/big.bin", &binary(3 * MIB as usize)); // LFS-tracked, staged below
    r.write("raw.dat", &binary(4 * MIB as usize)); // not tracked, not staged
    r.write("Design/new.psd", &binary(2 * MIB as usize)); // tracked, not staged
    r.write("build/app.zip", &binary(10)); // build output
    r.write("notes.txt", b"plain text\n");
    r.write("medium.mp4", &binary(2 * MIB as usize)); // binary, below warn
    setup::stage_as_lfs(git, r.root(), &["assets/big.bin".into()]).unwrap();

    let thresholds = GuardThresholds {
        warn_bytes: MIB,
        block_bytes: Some(7 * MIB / 2),
    };
    let paths: Vec<String> = [
        "assets/big.bin",
        "raw.dat",
        "old.psd",
        "Design\\new.psd",
        "build/app.zip",
        "notes.txt",
        "gone.txt",
        "raw.dat",
    ]
    .map(String::from)
    .to_vec();
    let report = guard::check(git, r.root(), &paths, thresholds).unwrap();
    assert_eq!(report.findings.len(), 7, "duplicates removed");
    let f = |p: &str| report.findings.iter().find(|f| f.path == p).unwrap();

    let big = f("assets/big.bin");
    assert!(big.already_lfs && big.staged_pointer, "{big:?}");
    assert!(big.staged_size.unwrap() < 200);
    assert_eq!(big.disk_size, Some(3 * MIB));
    assert_eq!(big.level, GuardLevel::None);
    assert!(!big.needs_renormalize);

    let raw = f("raw.dat");
    assert_eq!(raw.level, GuardLevel::Block);
    assert!(!raw.already_lfs && raw.looks_binary, "NUL sniffed");
    assert_eq!(raw.staged_size, None);
    assert_eq!(raw.pattern_by_extension.as_deref(), Some("*.dat"));
    assert_eq!(raw.pattern_by_path, "/raw.dat");
    let raw_pattern = raw.pattern_by_path.clone();

    let old = f("old.psd");
    assert!(old.already_lfs && !old.staged_pointer && old.needs_renormalize);
    assert_eq!(old.level, GuardLevel::Warn);

    let new = f("Design/new.psd");
    assert!(new.already_lfs && new.staged_size.is_none());
    assert_eq!(new.size, Some(0), "becomes a pointer when added");
    assert_eq!(new.level, GuardLevel::None);

    let zip = f("build/app.zip");
    assert!(zip.looks_like_build_output && zip.looks_binary);
    assert_eq!(zip.level, GuardLevel::None, "tiny");

    let notes = f("notes.txt");
    assert!(!notes.looks_binary && notes.level == GuardLevel::None);
    let gone = f("gone.txt");
    assert_eq!((gone.size, gone.level), (None, GuardLevel::None));
    assert_eq!((report.warn_count, report.block_count), (1, 1));
    assert!(report.blocked());

    // Binary below the warn threshold of GitHub defaults: suggestion only.
    let report = guard::check(
        git,
        r.root(),
        &["medium.mp4".into()],
        GuardThresholds::GITHUB,
    )
    .unwrap();
    assert_eq!(report.findings[0].level, GuardLevel::Suggest);
    assert_eq!(report.suggest_count, 1);

    // Renormalize turns the staged full blob into a pointer.
    setup::renormalize(git, r.root(), &["old.psd".into()]).unwrap();
    let report = guard::check(git, r.root(), &["old.psd".into()], thresholds).unwrap();
    let old = &report.findings[0];
    assert!(old.staged_pointer && !old.needs_renormalize, "{old:?}");
    assert_eq!(old.level, GuardLevel::None);

    // Track-by-path suggestion is anchored to the root: a same-named nested file is not LFS.
    r.write("sub/raw.dat", b"x");
    assert!(setup::track(git, r.root(), &raw_pattern, false).unwrap());
    let report = guard::check(
        git,
        r.root(),
        &["raw.dat".into(), "sub/raw.dat".into()],
        thresholds,
    )
    .unwrap();
    assert!(report.findings[0].already_lfs);
    assert!(!report.findings[1].already_lfs);

    // Invalid paths are rejected before running git.
    assert!(matches!(
        guard::check(git, r.root(), &["../outside".into()], thresholds),
        Err(LfsError::InvalidPath(_))
    ));
}
