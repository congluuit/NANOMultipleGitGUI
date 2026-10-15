use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // Must run before tauri-build, which fails if a configured `externalBin` is missing.
    prepare_credential_sidecar();
    load_dotenv();

    let mut attrs = tauri_build::Attributes::new();
    if std::env::var("TARGET").is_ok_and(|t| t.contains("windows-msvc")) {
        // Embed the manifest ourselves for every target instead of letting tauri-build add it
        // to the main binary only: test binaries link the dialog plugin, which imports
        // comctl32 v6 symbols and otherwise fails with STATUS_ENTRYPOINT_NOT_FOUND.
        attrs =
            attrs.windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    tauri_build::try_build(attrs).expect("failed to run tauri-build");
}

/// Build-time settings that may come from `<workspace>/.env` (git-ignored) when they are not
/// set in the environment, so local `pnpm dev` / `pnpm tauri build` embed the maintainer's
/// OAuth client ids without exporting anything. A real environment variable (CI) wins.
/// `NANOMGG_GITHUB_REPO` is the project's `owner/repo` (About links, update checks);
/// `NANOMGG_UPDATER_PUBKEY` is the minisign public key that update packages must be signed with.
const DOTENV_KEYS: [&str; 4] = [
    "NANOMGG_GITHUB_OAUTH_CLIENT_ID",
    "NANOMGG_GITLAB_OAUTH_CLIENT_ID",
    "NANOMGG_GITHUB_REPO",
    "NANOMGG_UPDATER_PUBKEY",
];

fn load_dotenv() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace = manifest_dir.join("../../..");
    let file = workspace.join(".env");
    for key in DOTENV_KEYS {
        println!("cargo:rerun-if-env-changed={key}");
    }
    let Ok(text) = std::fs::read_to_string(&file) else {
        // Watching a missing file would rerun this script on every build; the directory's
        // mtime changes when `.env` is created.
        println!("cargo:rerun-if-changed={}", workspace.display());
        return;
    };
    println!("cargo:rerun-if-changed={}", file.display());
    for line in text.lines() {
        let line = line.trim();
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if !DOTENV_KEYS.contains(&key) || std::env::var(key).is_ok_and(|v| !v.trim().is_empty()) {
            continue;
        }
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        if !value.is_empty() {
            println!("cargo:rustc-env={key}={value}");
        }
    }
}

/// Packages the `nanomgg-credential` sidecar (credential helper + SSH askpass).
///
/// `tauri.conf.json` lists it under `bundle.externalBin` as `binaries/nanomgg-credential`, so
/// Tauri expects `binaries/nanomgg-credential-<target-triple>[.exe]` next to this file. It then
/// copies it next to the app executable in `target/<profile>/` (used by `cargo run`, `cargo test`
/// and `tauri dev`) and into every bundle (`pnpm tauri build`), where the app finds it at runtime
/// beside its own exe (`accounts::sidecar_path`).
///
/// To keep plain `cargo build` / `cargo test` / `tauri build` working without an extra step, the
/// helper is built here with a nested `cargo build` (same profile and target) into a separate
/// target directory (`<target-dir>/sidecar-build`, so the outer build's lock is not contended).
/// Set `NANOMGG_SIDECAR=<path>` to use an already-built helper instead (CI caching, cross
/// builds). `binaries/` is generated and git-ignored.
fn prepare_credential_sidecar() {
    println!("cargo:rerun-if-env-changed=NANOMGG_SIDECAR");
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace = manifest_dir.join("../../..");
    for watched in [
        "crates/credential-helper/src",
        "crates/credential-helper/Cargo.toml",
        "crates/credentials/src",
        "crates/credentials/Cargo.toml",
    ] {
        println!(
            "cargo:rerun-if-changed={}",
            workspace.join(watched).display()
        );
    }

    let target = std::env::var("TARGET").unwrap();
    let exe = if target.contains("windows") {
        ".exe"
    } else {
        ""
    };
    let dest = manifest_dir
        .join("binaries")
        .join(format!("nanomgg-credential-{target}{exe}"));

    let built = match std::env::var_os("NANOMGG_SIDECAR") {
        Some(prebuilt) => PathBuf::from(prebuilt),
        None => build_helper(&workspace, &target, exe),
    };
    copy_if_changed(&built, &dest);
    if target.ends_with("-apple-darwin") {
        make_universal_sidecar(&manifest_dir.join("binaries"));
    }
}

/// `tauri build --target universal-apple-darwin` builds both architectures, then bundles
/// `binaries/nanomgg-credential-universal-apple-darwin`: glue the two together with `lipo`
/// once both exist.
fn make_universal_sidecar(dir: &Path) {
    let arm = dir.join("nanomgg-credential-aarch64-apple-darwin");
    let intel = dir.join("nanomgg-credential-x86_64-apple-darwin");
    if !(arm.exists() && intel.exists()) {
        return;
    }
    let out = dir.join("nanomgg-credential-universal-apple-darwin");
    let status = std::process::Command::new("lipo")
        .arg("-create")
        .arg("-output")
        .arg(&out)
        .arg(&arm)
        .arg(&intel)
        .status();
    if !status.as_ref().is_ok_and(|s| s.success()) {
        println!("cargo:warning=could not create the universal sidecar with lipo: {status:?}");
    }
}

fn build_helper(workspace: &Path, target: &str, exe: &str) -> PathBuf {
    let release = std::env::var("PROFILE").is_ok_and(|p| p == "release");
    // OUT_DIR = <target-dir>[/<triple>]/<profile>/build/nanomgg-<hash>/out
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let sidecar_dir = out_dir
        .ancestors()
        .nth(4)
        .expect("unexpected OUT_DIR layout")
        .join("sidecar-build");

    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.current_dir(workspace)
        .args([
            "build",
            "--package",
            "nanomgg-credential-helper",
            "--bin",
            "nanomgg-credential",
            "--target",
            target,
            "--target-dir",
        ])
        .arg(&sidecar_dir);
    if release {
        cmd.arg("--release");
    }
    // Do not inherit settings meant for the outer build (clippy driver, coverage flags, an
    // explicit target dir).
    for var in [
        "CARGO_TARGET_DIR",
        "CARGO_BUILD_TARGET_DIR",
        "CARGO_ENCODED_RUSTFLAGS",
        "RUSTC_WORKSPACE_WRAPPER",
        "CLIPPY_ARGS",
        "CARGO_PRIMARY_PACKAGE",
    ] {
        cmd.env_remove(var);
    }
    let status = cmd
        .status()
        .expect("failed to run cargo to build the nanomgg-credential sidecar");
    assert!(
        status.success(),
        "building the nanomgg-credential sidecar failed ({status}); \
         set NANOMGG_SIDECAR=<path to nanomgg-credential> to use a prebuilt one"
    );
    sidecar_dir
        .join(target)
        .join(if release { "release" } else { "debug" })
        .join(format!("nanomgg-credential{exe}"))
}

/// Copies only when the content differs, so unchanged builds do not retrigger tauri-build's
/// `rerun-if-changed` on the sidecar. Writes to a temporary file first: several builds (other
/// target dirs) may update `binaries/` at the same time.
fn copy_if_changed(from: &Path, to: &Path) {
    let data = std::fs::read(from)
        .unwrap_or_else(|e| panic!("cannot read sidecar {}: {e}", from.display()));
    if std::fs::read(to).is_ok_and(|old| old == data) {
        return;
    }
    std::fs::create_dir_all(to.parent().unwrap()).expect("create binaries/");
    let tmp = to.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, &data).expect("write sidecar");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
    }
    if let Err(e) = std::fs::rename(&tmp, to) {
        let _ = std::fs::remove_file(&tmp);
        // Another build won the race with identical content: fine.
        if !std::fs::read(to).is_ok_and(|old| old == data) {
            panic!("cannot install sidecar at {}: {e}", to.display());
        }
    }
}
