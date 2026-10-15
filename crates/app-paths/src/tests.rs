use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tempfile::TempDir;

use super::*;

struct Env {
    tmp: TempDir,
    defaults: Locations,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let local = tmp.path().join("local");
        let defaults = Locations {
            log: local.join("logs"),
            cache: local.join("cache"),
            data: tmp.path().join("roaming"),
        };
        Self { tmp, defaults }
    }

    fn pointer_path(&self) -> PathBuf {
        self.tmp.path().join("local").join(POINTER_FILE)
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.tmp.path().join(rel)
    }
}

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(30)
}

fn no_progress(_: &MoveProgress) {}

// ----- resolve -------------------------------------------------------------------------------

#[test]
fn resolve_without_pointer_uses_defaults() {
    let env = Env::new();
    let res = resolve(&env.defaults, &env.pointer_path());
    assert_eq!(res.locations, env.defaults);
    assert_eq!(res.pointer, Pointer::default());
    assert!(res.warnings.is_empty());
}

#[test]
fn resolve_valid_pointer() {
    let env = Env::new();
    let (log, data) = (env.path("D/logs"), env.path("D/data"));
    fs::create_dir_all(&log).unwrap();
    fs::create_dir_all(&data).unwrap();
    let pointer = Pointer {
        log: Some(log.clone()),
        data: Some(data.clone()),
        ..Pointer::default()
    };
    write_pointer(&env.pointer_path(), &pointer).unwrap();
    let res = resolve(&env.defaults, &env.pointer_path());
    assert_eq!(res.locations.log, log);
    assert_eq!(res.locations.cache, env.defaults.cache);
    assert_eq!(res.locations.data, data);
    assert_eq!(res.pointer, pointer);
    assert!(res.warnings.is_empty(), "{:?}", res.warnings);
}

#[test]
fn resolve_corrupt_pointer_falls_back_with_warning() {
    let env = Env::new();
    write(&env.pointer_path(), "{ not json");
    let res = resolve(&env.defaults, &env.pointer_path());
    assert_eq!(res.locations, env.defaults);
    assert!(matches!(
        res.warnings.as_slice(),
        [StorageWarning::PointerUnreadable { .. }]
    ));
}

#[test]
fn resolve_missing_folders() {
    let env = Env::new();
    let pointer = Pointer {
        // Parent exists → recreated.
        log: Some(env.path("logs-here")),
        // Drive gone → default.
        cache: Some(env.path("gone/drive/cache")),
        // Data is never recreated.
        data: Some(env.path("data-gone")),
        ..Pointer::default()
    };
    write_pointer(&env.pointer_path(), &pointer).unwrap();
    let res = resolve(&env.defaults, &env.pointer_path());
    assert_eq!(res.locations.log, env.path("logs-here"));
    assert!(env.path("logs-here").is_dir());
    assert_eq!(res.locations.cache, env.defaults.cache);
    assert_eq!(res.locations.data, env.defaults.data);
    assert!(!env.path("data-gone").exists());
    let missing: Vec<StorageKind> = res
        .warnings
        .iter()
        .filter_map(|w| match w {
            StorageWarning::MissingFolder { storage, .. } => Some(*storage),
            _ => None,
        })
        .collect();
    assert_eq!(missing, vec![StorageKind::Cache, StorageKind::Data]);
    // The pointer keeps the custom paths for when the drive is back.
    assert_eq!(res.pointer, pointer);
}

#[test]
fn relative_custom_path_is_ignored() {
    let env = Env::new();
    write(
        &env.pointer_path(),
        r#"{"version":1,"log":"relative/logs"}"#,
    );
    let res = resolve(&env.defaults, &env.pointer_path());
    assert_eq!(res.locations.log, env.defaults.log);
    assert_eq!(res.warnings.len(), 1);
}

#[test]
fn default_pointer_removes_the_file() {
    let env = Env::new();
    let mut pointer = Pointer {
        cache: Some(env.path("c")),
        ..Pointer::default()
    };
    write_pointer(&env.pointer_path(), &pointer).unwrap();
    assert!(env.pointer_path().is_file());
    pointer.set(StorageKind::Cache, &env.defaults.cache, &env.defaults);
    assert_eq!(pointer, Pointer::default());
    write_pointer(&env.pointer_path(), &pointer).unwrap();
    assert!(!env.pointer_path().exists());
}

// ----- usage, probes, cleanup ----------------------------------------------------------------

#[test]
fn usage_counts_files_and_honours_exclusions() {
    let env = Env::new();
    let root = env.path("data");
    write(&root.join("a.db"), "12345");
    write(&root.join("ssh/key"), "abc");
    write(&root.join("logs/x.log"), "zzzzzzzzzz");
    let all = dir_usage(&root, &[], soon());
    assert_eq!((all.bytes, all.files, all.complete), (18, 3, true));
    let some = dir_usage(&root, &[root.join("logs")], soon());
    assert_eq!((some.bytes, some.files), (8, 2));
    let missing = dir_usage(&env.path("nope"), &[], soon());
    assert_eq!((missing.bytes, missing.complete), (0, true));
}

#[test]
fn usage_stops_at_the_deadline() {
    let env = Env::new();
    let root = env.path("big");
    for i in 0..50 {
        write(&root.join(format!("d{i}/f")), "x");
    }
    let usage = dir_usage(&root, &[], Instant::now());
    assert!(!usage.complete);
    assert!(usage.files < 50);
}

#[test]
fn write_probe() {
    let env = Env::new();
    check_writable(env.tmp.path()).unwrap();
    assert_eq!(
        fs::read_dir(env.tmp.path()).unwrap().count(),
        0,
        "probe left behind"
    );
    assert!(check_writable(&env.path("missing")).is_err());
}

#[test]
fn free_space_of_missing_folder_uses_parent() {
    let env = Env::new();
    assert!(free_space(&env.path("a/b/c")).is_some_and(|f| f > 0));
}

#[test]
fn prune_logs_keeps_the_newest_file() {
    let env = Env::new();
    let dir = env.path("logs");
    write(&dir.join("nanomgg.2026-10-13.log"), "old");
    write(&dir.join("nanomgg.2026-10-14.log"), "older!");
    std::thread::sleep(Duration::from_millis(50));
    write(&dir.join("nanomgg.2026-10-15.log"), "today");
    write(&dir.join("notes.txt"), "not a log");
    let report = prune_logs(&dir, "nanomgg");
    assert_eq!(report.removed_files, 2);
    assert_eq!(report.removed_bytes, 9);
    assert_eq!(report.failed, 0);
    assert!(dir.join("nanomgg.2026-10-15.log").is_file());
    assert!(dir.join("notes.txt").is_file());
    assert!(!dir.join("nanomgg.2026-10-13.log").exists());
}

#[test]
fn clear_cache_contents() {
    let env = Env::new();
    let dir = env.path("cache");
    write(&dir.join("a"), "1234");
    write(&dir.join("sub/b"), "56");
    let report = clear_dir_contents(&dir);
    assert_eq!((report.removed_files, report.removed_bytes), (2, 6));
    assert!(dir.is_dir());
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
    assert_eq!(
        clear_dir_contents(&env.path("none")),
        CleanReport::default()
    );
}

// ----- copy / move ---------------------------------------------------------------------------

#[test]
fn copy_then_remove_source() {
    let env = Env::new();
    let (from, to) = (env.path("from"), env.path("dest/new"));
    write(&from.join("nanomgg.db"), "database");
    write(&from.join("ssh/k1"), "key");
    fs::create_dir_all(from.join("empty")).unwrap();
    write(&from.join("cache/skip"), "excluded");
    let mut phases = Vec::new();
    let tree = copy_tree(&from, &to, &[from.join("cache")], &mut |p| {
        phases.push(p.phase);
    })
    .unwrap();
    assert_eq!(tree.bytes, 11);
    assert_eq!(
        fs::read_to_string(to.join("nanomgg.db")).unwrap(),
        "database"
    );
    assert_eq!(fs::read_to_string(to.join("ssh/k1")).unwrap(), "key");
    assert!(to.join("empty").is_dir());
    assert!(!to.join("cache").exists());
    assert!(phases.contains(&MovePhase::Copying) && phases.contains(&MovePhase::Verifying));
    assert!(tree.remove_source().is_empty());
    // Only the excluded folder stays behind.
    assert!(!from.join("nanomgg.db").exists());
    assert!(!from.join("ssh").exists());
    assert!(from.join("cache/skip").is_file());
}

#[test]
fn failed_copy_keeps_the_source_and_cleans_the_target() {
    let env = Env::new();
    let (from, to) = (env.path("from"), env.path("to"));
    write(&from.join("a.txt"), "aaa");
    write(&from.join("b.txt"), "bbb");
    write(&from.join("sub/c.txt"), "ccc");
    // A folder where a file must go makes the copy fail midway.
    fs::create_dir_all(to.join("b.txt")).unwrap();
    let err = copy_tree(&from, &to, &[], &mut no_progress).unwrap_err();
    assert!(err.path.ends_with("b.txt"), "{err}");
    for f in ["a.txt", "b.txt", "sub/c.txt"] {
        assert!(from.join(f).is_file(), "source {f} must stay");
    }
    assert!(!to.join("a.txt").exists());
    assert!(!to.join("sub").exists());
    // What was there before is left alone.
    assert!(to.join("b.txt").is_dir());
}

#[test]
fn rollback_removes_created_folders() {
    let env = Env::new();
    let (from, to) = (env.path("from"), env.path("x/y/to"));
    write(&from.join("d/e/f"), "1");
    let tree = copy_tree(&from, &to, &[], &mut no_progress).unwrap();
    tree.rollback();
    assert!(!env.path("x").exists());
    assert!(from.join("d/e/f").is_file());
}

// ----- target rules --------------------------------------------------------------------------

#[test]
fn target_rules() {
    let env = Env::new();
    let d = &env.defaults;
    let protected = [env.path("local")];
    let plan = |kind, p: &Path| plan_target(kind, p, d, d, &protected);

    // Still empty: an ancestor of the (not yet created) app folders.
    assert_eq!(
        plan(StorageKind::Log, env.tmp.path()),
        Err(TargetProblem::ContainsAppFolder)
    );

    let empty = env.path("picked-empty");
    fs::create_dir_all(&empty).unwrap();
    let ok = plan(StorageKind::Log, &empty).unwrap();
    assert_eq!(ok.target, empty);
    assert!(!ok.creates_subfolder && !ok.is_default);
    assert_eq!(
        plan(StorageKind::Log, &env.path("new-folder"))
            .unwrap()
            .target,
        env.path("new-folder")
    );

    // Non-empty → dedicated subfolder.
    let busy = env.path("busy");
    write(&busy.join("my-file.txt"), "user data");
    let sub = plan(StorageKind::Cache, &busy).unwrap();
    assert_eq!(sub.target, busy.join("nanomgg-cache"));
    assert!(sub.creates_subfolder);
    write(&busy.join("nanomgg-cache/x"), "x");
    assert!(matches!(
        plan(StorageKind::Cache, &busy),
        Err(TargetProblem::NotEmpty { .. })
    ));

    assert_eq!(
        plan(StorageKind::Log, Path::new("relative")),
        Err(TargetProblem::InvalidPath)
    );
    assert_eq!(
        plan(StorageKind::Log, &env.path("a/../b")),
        Err(TargetProblem::InvalidPath)
    );
    assert_eq!(
        plan(StorageKind::Log, &d.log),
        Err(TargetProblem::SameAsCurrent)
    );
    // Inside itself / another app folder / the pointer folder.
    assert_eq!(
        plan(StorageKind::Log, &d.log.join("deeper")),
        Err(TargetProblem::InsideAppFolder)
    );
    assert_eq!(
        plan(StorageKind::Cache, &d.data.join("cache")),
        Err(TargetProblem::InsideAppFolder)
    );
    assert_eq!(
        plan(StorageKind::Cache, &env.path("local/other")),
        Err(TargetProblem::InsideAppFolder)
    );
    let file = env.path("file.txt");
    write(&file, "x");
    assert_eq!(
        plan(StorageKind::Log, &file),
        Err(TargetProblem::NotAFolder)
    );
}

#[test]
fn back_to_default_is_allowed_even_if_not_empty() {
    let env = Env::new();
    let custom = env.path("custom-data");
    fs::create_dir_all(&custom).unwrap();
    write(&env.defaults.data.join("stale.txt"), "old");
    let mut current = env.defaults.clone();
    current.data = custom;
    let plan = plan_target(
        StorageKind::Data,
        &env.defaults.data,
        &current,
        &env.defaults,
        &[],
    )
    .unwrap();
    assert!(plan.is_default);
    assert_eq!(plan.target, env.defaults.data);
}

#[test]
fn verify_target_checks_writable_and_space() {
    let env = Env::new();
    let target = env.path("t/new");
    assert!(verify_target(&target, 1).unwrap() > 0);
    assert!(!target.exists(), "folder created for the check is removed");
    assert!(matches!(
        verify_target(&target, u64::MAX - rules::SPACE_MARGIN - 1),
        Err(TargetProblem::NotEnoughSpace { .. })
    ));
    let file = env.path("file");
    write(&file, "x");
    assert!(matches!(
        verify_target(&file.join("sub"), 1),
        Err(TargetProblem::NotWritable { .. })
    ));
}

// ----- pending data move ---------------------------------------------------------------------

fn seed_data(dir: &Path) {
    write(&dir.join("nanomgg.db"), "sqlite");
    write(&dir.join("ssh/acc_1"), "private key");
    write(&dir.join("known_hosts"), "github.com ...");
}

#[test]
fn pending_data_move_on_restart() {
    let env = Env::new();
    seed_data(&env.defaults.data);
    // macOS/Linux layout: the log folder lives inside the data folder and must stay.
    let mut defaults = env.defaults.clone();
    defaults.log = defaults.data.join("logs");
    write(&defaults.log.join("nanomgg.log"), "log");
    let to = env.path("D/nanomgg-data");
    let pointer = Pointer {
        pending_data: Some(PendingDataMove {
            from: defaults.data.clone(),
            to: to.clone(),
        }),
        ..Pointer::default()
    };
    write_pointer(&env.pointer_path(), &pointer).unwrap();

    let mut res = resolve(&defaults, &env.pointer_path());
    let mut fixed = Vec::new();
    let moved = apply_pending_data(
        &mut res,
        &defaults,
        &env.pointer_path(),
        &mut no_progress,
        &mut |p| fixed.push(p.to_owned()),
    )
    .unwrap();
    assert_eq!(moved.to, to);
    assert_eq!(fixed, vec![to.clone()]);
    assert_eq!(res.locations.data, to);
    assert_eq!(
        fs::read_to_string(to.join("ssh/acc_1")).unwrap(),
        "private key"
    );
    assert!(!to.join("logs").exists());
    assert!(!defaults.data.join("nanomgg.db").exists());
    assert!(defaults.log.join("nanomgg.log").is_file());
    assert!(
        res.warnings
            .iter()
            .any(|w| matches!(w, StorageWarning::DataMoved { .. }))
    );

    // Next start finds the new folder through the pointer, nothing pending.
    let again = resolve(&defaults, &env.pointer_path());
    assert_eq!(again.locations.data, to);
    assert_eq!(again.pointer.pending_data, None);
    assert!(again.warnings.is_empty());
}

#[test]
fn failed_pending_move_keeps_old_data_and_is_not_retried() {
    let env = Env::new();
    seed_data(&env.defaults.data);
    let to = env.path("to");
    fs::create_dir_all(to.join("known_hosts")).unwrap();
    let pointer = Pointer {
        pending_data: Some(PendingDataMove {
            from: env.defaults.data.clone(),
            to: to.clone(),
        }),
        ..Pointer::default()
    };
    write_pointer(&env.pointer_path(), &pointer).unwrap();
    let mut res = resolve(&env.defaults, &env.pointer_path());
    let moved = apply_pending_data(
        &mut res,
        &env.defaults,
        &env.pointer_path(),
        &mut no_progress,
        &mut |_| panic!("not called on failure"),
    );
    assert_eq!(moved, None);
    assert_eq!(res.locations.data, env.defaults.data);
    assert_eq!(
        fs::read_to_string(env.defaults.data.join("nanomgg.db")).unwrap(),
        "sqlite"
    );
    assert!(!to.join("nanomgg.db").exists());
    assert!(matches!(
        res.warnings.as_slice(),
        [StorageWarning::DataMoveFailed { .. }]
    ));
    assert_eq!(read_pointer(&env.pointer_path()).unwrap(), None);
}

#[test]
fn pending_move_from_another_folder_is_dropped() {
    let env = Env::new();
    seed_data(&env.defaults.data);
    let pointer = Pointer {
        pending_data: Some(PendingDataMove {
            from: env.path("somewhere-else"),
            to: env.path("to"),
        }),
        ..Pointer::default()
    };
    write_pointer(&env.pointer_path(), &pointer).unwrap();
    let mut res = resolve(&env.defaults, &env.pointer_path());
    assert_eq!(
        apply_pending_data(
            &mut res,
            &env.defaults,
            &env.pointer_path(),
            &mut no_progress,
            &mut |_| {}
        ),
        None
    );
    assert!(!env.path("to").exists());
    assert!(env.defaults.data.join("nanomgg.db").is_file());
}

#[test]
fn rebase_paths_below_the_old_folder() {
    let (from, to) = (Path::new("/a/data"), Path::new("/b/new"));
    assert_eq!(
        rebase_path(Path::new("/a/data/ssh/k"), from, to),
        Some(PathBuf::from("/b/new/ssh/k"))
    );
    assert_eq!(rebase_path(Path::new("/a/other/k"), from, to), None);
}
