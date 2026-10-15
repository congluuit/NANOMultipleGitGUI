//! Installs the app by running its own NSIS installer silently (`/S /D=<dir>`), so the result is
//! exactly what the classic installer and the in-app updater produce: same files, registry
//! entries, Start menu shortcut and uninstaller (Settings → Apps).
//!
//! Errors are short codes (`notWritable`, `exit:2`…) that the UI translates.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const PRODUCT: &str = "NANOMultipleGitGUI";
const MAIN_BINARY: &str = "nanomgg.exe";
#[cfg(windows)]
const UNINSTALL_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Uninstall\NANOMultipleGitGUI";
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x0000_0008;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Existing {
    pub dir: String,
    pub version: Option<String>,
    /// The installed version is newer than this setup (NSIS refuses silent downgrades).
    pub newer: bool,
    /// Same version: installing again repairs it.
    pub same: bool,
}

/// Same default as the NSIS installer in per-user mode: `%LOCALAPPDATA%\NANOMultipleGitGUI`.
pub fn default_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(PRODUCT)
}

/// The per-user installation recorded by NSIS, if any.
#[cfg(windows)]
pub fn existing(this_version: &str) -> Option<Existing> {
    let key = windows_registry::CURRENT_USER.open(UNINSTALL_KEY).ok()?;
    let dir = key.get_string("InstallLocation").ok()?;
    let dir = dir.trim().trim_matches('"').to_owned();
    if dir.is_empty() {
        return None;
    }
    let version = key.get_string("DisplayVersion").ok();
    let order = version
        .as_deref()
        .and_then(|v| semver::Version::parse(v).ok())
        .zip(semver::Version::parse(this_version).ok())
        .map(|(installed, this)| installed.cmp(&this));
    Some(Existing {
        dir,
        version,
        newer: order == Some(std::cmp::Ordering::Greater),
        same: order == Some(std::cmp::Ordering::Equal),
    })
}

#[cfg(not(windows))]
pub fn existing(_this_version: &str) -> Option<Existing> {
    None
}

/// Whether the app is running for this user (NSIS closes it during the install).
#[cfg(windows)]
pub fn app_running() -> bool {
    use std::os::windows::process::CommandExt;
    Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {MAIN_BINARY}"), "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .is_ok_and(|o| {
            String::from_utf8_lossy(&o.stdout)
                .to_ascii_lowercase()
                .contains(MAIN_BINARY)
        })
}

#[cfg(not(windows))]
pub fn app_running() -> bool {
    false
}

/// A folder the per-user installer can use: absolute, writable without elevation, and either
/// new, empty or an earlier installation (the uninstaller later removes the folder).
pub fn check_folder(dir: &Path) -> Result<(), String> {
    if !dir.is_absolute() || dir.parent().is_none() {
        return Err("notAbsolute".into());
    }
    if dir.is_file() {
        return Err("notEmpty".into());
    }
    if dir.is_dir() {
        let empty = fs::read_dir(dir)
            .map_err(|_| "notWritable")?
            .next()
            .is_none();
        if !empty && !dir.join(MAIN_BINARY).is_file() {
            return Err("notEmpty".into());
        }
    }
    let existing_ancestor = dir.ancestors().find(|p| p.is_dir()).ok_or("notAbsolute")?;
    let probe = existing_ancestor.join(format!(".nanomgg-setup-{}", std::process::id()));
    fs::write(&probe, b"").map_err(|_| "notWritable")?;
    let _ = fs::remove_file(&probe);
    Ok(())
}

/// Folder picked in the browse dialog → install folder: a product subfolder unless the user
/// picked one already named like the product.
pub fn install_dir_for_pick(picked: &Path) -> PathBuf {
    let named = picked
        .file_name()
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(PRODUCT));
    if named {
        picked.to_path_buf()
    } else {
        picked.join(PRODUCT)
    }
}

/// Writes the embedded installer to a private temp folder; removed again on drop.
struct Extracted {
    dir: PathBuf,
    exe: PathBuf,
}

impl Extracted {
    fn new(payload: &[u8]) -> Result<Self, String> {
        let dir = std::env::temp_dir().join(format!("nanomgg-setup-{}", std::process::id()));
        fs::create_dir_all(&dir).map_err(|_| "extract")?;
        let exe = dir.join(format!("{PRODUCT}-setup.exe"));
        fs::write(&exe, payload).map_err(|_| "extract")?;
        Ok(Self { dir, exe })
    }
}

impl Drop for Extracted {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

pub fn install(
    payload: &[u8],
    dir: &Path,
    desktop: Option<&Path>,
    desktop_shortcut: bool,
    stage: impl Fn(&'static str),
) -> Result<(), String> {
    if payload.is_empty() {
        return Err("noPayload".into());
    }
    check_folder(dir)?;
    stage("extracting");
    let extracted = Extracted::new(payload)?;
    let shortcut = desktop.map(|d| d.join(format!("{PRODUCT}.lnk")));
    let had_shortcut = shortcut.as_deref().is_some_and(Path::exists);

    stage("installing");
    let status = run_silent(&extracted.exe, dir)?;
    drop(extracted);
    if !status.success() {
        return Err(format!("exit:{}", status.code().unwrap_or(-1)));
    }
    if !dir.join(MAIN_BINARY).is_file() {
        return Err("missing".into());
    }

    stage("finishing");
    // Silent NSIS always creates the desktop shortcut; keep one the user already had.
    if !desktop_shortcut
        && !had_shortcut
        && let Some(path) = shortcut
    {
        let _ = fs::remove_file(path);
    }
    Ok(())
}

#[cfg(windows)]
fn run_silent(exe: &Path, dir: &Path) -> Result<std::process::ExitStatus, String> {
    use std::os::windows::process::CommandExt;
    // NSIS reads `/D=` raw: it must be the last argument and must not be quoted.
    Command::new(exe)
        .arg("/S")
        .raw_arg(format!("/D={}", dir.display()))
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|_| "launch".into())
}

#[cfg(not(windows))]
fn run_silent(_exe: &Path, _dir: &Path) -> Result<std::process::ExitStatus, String> {
    Err("windowsOnly".into())
}

/// Starts the installed app on its own (it outlives the setup).
pub fn launch(dir: &Path) -> Result<(), String> {
    let mut cmd = Command::new(dir.join(MAIN_BINARY));
    cmd.current_dir(dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(DETACHED_PROCESS);
    }
    cmd.spawn().map(drop).map_err(|_| "launch".into())
}

/// Runs the embedded installer with its own classic wizard (used when this window cannot be
/// shown because the WebView2 runtime is missing; that wizard installs it).
pub fn run_classic(payload: &[u8]) -> i32 {
    let Ok(extracted) = Extracted::new(payload) else {
        return 1;
    };
    Command::new(&extracted.exe)
        .status()
        .map_or(1, |s| s.code().unwrap_or(1))
}
