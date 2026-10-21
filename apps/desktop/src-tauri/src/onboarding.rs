//! First-run setup (PLAN Phase 3B.2): checks the tools the app relies on, installs a missing one
//! with the OS package manager on request, and remembers how far the user got.
//!
//! - Tools are probed with their own `--version` (no shell, timeout, no console window). A tool
//!   not on the app's `PATH` is also looked for in its default install folder: right after an
//!   install the app process still has the old `PATH`, so it must be restarted to use it.
//! - Installs only run commands from a fixed whitelist ([`install_plan`]); the frontend sends a
//!   tool id, never a command line.
//! - Progress lives in the setting `onboarding.setup`. The setup opens by itself only for a new
//!   user: never completed/skipped, no account and the import wizard never finished.

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use nanomgg_git_engine::version::{MIN_GIT, MIN_GIT_LFS, parse_git_version, parse_lfs_version};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::accounts::storage_err;
use crate::error::{AppError, AppResult};
use crate::import::KEY_IMPORT_DONE;
use crate::state::AppState;

mod lfs_release;

/// Setting: [`SetupProgress`].
pub const KEY_SETUP: &str = "onboarding.setup";

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// Lines of installer output kept for "Chi tiết kỹ thuật".
const OUTPUT_LINES: usize = 80;

// ----- DTOs ----------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SetupStep {
    Welcome,
    Tools,
    Accounts,
    Finish,
}

/// Stored under [`KEY_SETUP`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetupProgress {
    pub completed: bool,
    pub skipped: bool,
    pub steps_done: Vec<SetupStep>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    pub progress: SetupProgress,
    pub account_count: u32,
    pub import_done: bool,
    /// Brand-new user: open the setup on start.
    pub should_auto_open: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SetupToolId {
    Git,
    GitLfs,
    Ssh,
    Gh,
    Glab,
    Webview2,
}

/// How much the app needs the tool: git blocks, git-lfs / OpenSSH warn, gh / glab are info only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SetupImportance {
    Required,
    Recommended,
    Optional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SetupToolState {
    Ok,
    Outdated,
    Missing,
    /// Installed in its default folder but not on the app's `PATH` (restart the app).
    OutsidePath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SetupPackageManager {
    Winget,
    Brew,
    XcodeSelect,
    /// The official release archive from GitHub, unpacked by the app (git-lfs on macOS when
    /// Homebrew cannot install it: Intel Macs, or no Homebrew).
    GithubRelease,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetupTool {
    pub id: SetupToolId,
    pub importance: SetupImportance,
    pub state: SetupToolState,
    pub version: Option<String>,
    pub minimum: Option<String>,
    pub path: Option<String>,
    /// First line of the tool's own version output.
    pub raw: Option<String>,
    pub download_url: String,
    /// "Cài giúp tôi" is possible with this manager.
    pub installer: Option<SetupPackageManager>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetupToolsReport {
    pub tools: Vec<SetupTool>,
    /// git is usable: the next step may be opened.
    pub can_continue: bool,
    /// Something was found outside `PATH`: the app must restart to use it.
    pub restart_needed: bool,
    /// Package manager this OS installs with (`None` on Linux).
    pub package_manager: Option<SetupPackageManager>,
    pub package_manager_found: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SetupInstallOutcome {
    Installed,
    AlreadyInstalled,
    /// `xcode-select --install` opened the system installer; the user continues there.
    Started,
    /// Cancelled from the app.
    Cancelled,
    /// The user said no to the administrator (UAC) prompt or closed the installer.
    UserCancelled,
    NeedsAdmin,
    ManagerMissing,
    Network,
    PackageNotFound,
    TimedOut,
    Failed,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetupInstallResult {
    pub tool: SetupToolId,
    pub outcome: SetupInstallOutcome,
    pub manager: Option<SetupPackageManager>,
    /// The command that ran, for the technical details.
    pub command: Option<String>,
    pub exit_code: Option<i32>,
    /// Cleaned, shortened output (progress bars dropped).
    pub output: String,
}

/// What an install is doing now, read from the installer's output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SetupInstallPhase {
    Preparing,
    Downloading,
    Verifying,
    Installing,
    Finishing,
}

/// Live progress of `setup_install` (event `setup-install-progress`).
#[derive(Debug, Clone, PartialEq, Serialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct SetupInstallProgress {
    pub tool: SetupToolId,
    pub phase: SetupInstallPhase,
    /// Of the current phase (a download), when the installer prints one.
    pub percent: Option<u8>,
    /// Last few readable output lines, oldest first.
    pub recent: Vec<String>,
}

// ----- environment ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TargetOs {
    Windows,
    Macos,
    Linux,
}

impl TargetOs {
    pub(crate) fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else {
            Self::Linux
        }
    }
}

/// Where tools are looked for. [`ProbeEnv::current`] reads the real process; tests build one.
#[derive(Debug, Clone)]
pub(crate) struct ProbeEnv {
    pub os: TargetOs,
    pub path: Option<OsString>,
    /// Windows executable extensions, lower case with the dot (`PATHEXT`).
    pub exts: Vec<String>,
    /// `%ProgramFiles%`, `%ProgramFiles(x86)%`.
    pub program_files: Vec<PathBuf>,
    pub local_app_data: Option<PathBuf>,
    pub system_root: Option<PathBuf>,
    /// macOS / Linux folders outside a GUI app's minimal `PATH` (Homebrew).
    pub unix_dirs: Vec<PathBuf>,
}

impl ProbeEnv {
    pub(crate) fn current() -> Self {
        let var = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let os = TargetOs::current();
        let exts = if os == TargetOs::Windows {
            std::env::var("PATHEXT")
                .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
                .split(';')
                .filter(|e| !e.is_empty())
                .map(|e| e.to_ascii_lowercase())
                .collect()
        } else {
            Vec::new()
        };
        Self {
            os,
            path: std::env::var_os("PATH"),
            exts,
            program_files: ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
                .into_iter()
                .filter_map(var)
                .collect(),
            local_app_data: var("LOCALAPPDATA"),
            system_root: var("SystemRoot"),
            unix_dirs: [
                "/opt/homebrew/bin",
                "/usr/local/bin",
                "/opt/local/bin",
                "/usr/bin",
            ]
            .into_iter()
            .map(PathBuf::from)
            // Where the app puts git-lfs when `/usr/local/bin` is not writable.
            .chain(var("HOME").map(|h| h.join(".local").join("bin")))
            .collect(),
        }
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        self.path
            .as_ref()
            .map(|p| std::env::split_paths(p).collect())
            .unwrap_or_default()
    }
}

/// App execution aliases (`WindowsApps\winget.exe`) are reparse points `is_file` may reject.
fn exists_file(p: &Path) -> bool {
    p.is_file() || std::fs::symlink_metadata(p).is_ok_and(|m| !m.is_dir())
}

/// `name` in `dir`, trying the Windows executable extensions.
pub(crate) fn find_in(dir: &Path, name: &str, env: &ProbeEnv) -> Option<PathBuf> {
    if env.os == TargetOs::Windows {
        env.exts
            .iter()
            .map(|ext| dir.join(format!("{name}{ext}")))
            .find(|p| exists_file(p))
    } else {
        Some(dir.join(name)).filter(|p| exists_file(p))
    }
}

/// First `name` on `PATH`.
pub(crate) fn which(name: &str, env: &ProbeEnv) -> Option<PathBuf> {
    env.path_dirs().iter().find_map(|d| find_in(d, name, env))
}

/// Default install folders, searched when the tool is not on `PATH`.
pub(crate) fn fallback_dirs(tool: SetupToolId, env: &ProbeEnv) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    match env.os {
        TargetOs::Windows => {
            let pf = &env.program_files;
            let lad = env.local_app_data.as_deref();
            let both = |dirs: &mut Vec<PathBuf>, rel: &[&str]| {
                for base in pf
                    .iter()
                    .map(PathBuf::as_path)
                    .chain(lad.map(|l| l.join("Programs")).as_deref())
                {
                    let mut p = base.to_path_buf();
                    p.extend(rel);
                    if !dirs.contains(&p) {
                        dirs.push(p);
                    }
                }
            };
            match tool {
                SetupToolId::Git => both(&mut dirs, &["Git", "cmd"]),
                SetupToolId::GitLfs => {
                    both(&mut dirs, &["Git LFS"]);
                    both(&mut dirs, &["Git", "mingw64", "bin"]);
                }
                SetupToolId::Gh => both(&mut dirs, &["GitHub CLI"]),
                SetupToolId::Glab => both(&mut dirs, &["glab"]),
                SetupToolId::Ssh => {
                    both(&mut dirs, &["Git", "usr", "bin"]);
                    if let Some(root) = &env.system_root {
                        dirs.push(root.join("System32").join("OpenSSH"));
                    }
                }
                SetupToolId::Webview2 => {}
            }
            // winget "portable" packages are linked here.
            if let Some(l) = lad
                && tool != SetupToolId::Webview2
            {
                dirs.push(l.join("Microsoft").join("WinGet").join("Links"));
            }
        }
        TargetOs::Macos | TargetOs::Linux => {
            if tool != SetupToolId::Webview2 {
                dirs.extend(env.unix_dirs.iter().cloned());
            }
        }
    }
    dirs
}

/// `PATH` first, then the default folders (`on_path == false`).
fn locate(tool: SetupToolId, name: &str, env: &ProbeEnv) -> Option<(PathBuf, bool)> {
    if let Some(p) = which(name, env) {
        return Some((p, true));
    }
    fallback_dirs(tool, env)
        .iter()
        .find_map(|d| find_in(d, name, env))
        .map(|p| (p, false))
}

// ----- running tools -------------------------------------------------------------------------

pub(crate) fn command(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

#[derive(Debug)]
pub(crate) struct RunOutput {
    pub exit_code: Option<i32>,
    /// stdout then stderr, lossily decoded.
    pub text: String,
    pub cancelled: bool,
    pub timed_out: bool,
}

/// Kills `child` and, on Unix, its whole process group: `brew` is a shell script whose `ruby` /
/// `curl` children would otherwise outlive it, still holding Homebrew's lock so that every
/// retry fails with "already locked".
fn kill_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("/bin/kill")
            .args(["-s", "KILL", "--", &format!("-{}", child.id())])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

/// Runs `cmd` until it exits, `cancel` is set or `timeout` passes (then it and its children are
/// killed). Pipes are drained on threads; a grandchild still holding them does not block the
/// result.
pub(crate) fn run_cancellable(
    cmd: Command,
    cancel: &AtomicBool,
    timeout: Duration,
) -> std::io::Result<RunOutput> {
    run_streaming(cmd, cancel, timeout, &mut |_| {})
}

/// Splits a pipe's bytes into lines as they arrive; `\r` ends a line too, so a progress bar
/// redrawn in place reports each state.
#[derive(Default)]
struct LineSplitter(Vec<u8>);

impl LineSplitter {
    fn push(&mut self, bytes: &[u8], on_line: &mut dyn FnMut(&str)) {
        for &b in bytes {
            if b == b'\n' || b == b'\r' {
                self.flush(on_line);
            } else {
                self.0.push(b);
            }
        }
    }

    fn flush(&mut self, on_line: &mut dyn FnMut(&str)) {
        if !self.0.is_empty() {
            on_line(&String::from_utf8_lossy(&self.0));
            self.0.clear();
        }
    }
}

/// [`run_cancellable`] that also hands every output line (stdout or stderr) to `on_line` while
/// the process runs.
pub(crate) fn run_streaming(
    mut cmd: Command,
    cancel: &AtomicBool,
    timeout: Duration,
    on_line: &mut dyn FnMut(&str),
) -> std::io::Result<RunOutput> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn()?;
    // `(is_err, Some(chunk))` while reading, `(is_err, None)` at the end of the pipe.
    let (tx, rx) = mpsc::channel::<(bool, Option<Vec<u8>>)>();
    let mut open = 0;
    for (is_err, pipe) in [
        (
            false,
            child
                .stdout
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
        ),
        (
            true,
            child
                .stderr
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
        ),
    ] {
        if let Some(mut pipe) = pipe {
            open += 1;
            let tx = tx.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match pipe.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if tx.send((is_err, Some(buf[..n].to_vec()))).is_err() {
                                return;
                            }
                        }
                    }
                }
                let _ = tx.send((is_err, None));
            });
        }
    }
    drop(tx);
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let mut split = [LineSplitter::default(), LineSplitter::default()];
    let mut take = |msg: (bool, Option<Vec<u8>>), open: &mut usize| match msg {
        (is_err, Some(chunk)) => {
            split[usize::from(is_err)].push(&chunk, on_line);
            (if is_err { &mut err } else { &mut out }).extend_from_slice(&chunk);
        }
        (is_err, None) => {
            split[usize::from(is_err)].flush(on_line);
            *open -= 1;
        }
    };
    let start = Instant::now();
    let (mut cancelled, mut timed_out) = (false, false);
    let status = loop {
        while let Ok(msg) = rx.try_recv() {
            take(msg, &mut open);
        }
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
        } else if start.elapsed() > timeout {
            timed_out = true;
        }
        if cancelled || timed_out {
            kill_tree(&mut child);
            break child.wait().ok();
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    while open > 0 {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(msg) => take(msg, &mut open),
            Err(_) => break,
        }
    }
    let mut text = String::from_utf8_lossy(&out).into_owned();
    let err = String::from_utf8_lossy(&err);
    if !err.trim().is_empty() {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&err);
    }
    Ok(RunOutput {
        exit_code: if cancelled || timed_out {
            None
        } else {
            status.and_then(|s| s.code())
        },
        text,
        cancelled,
        timed_out,
    })
}

/// Output of `<program> <args>`, or `None` if it could not run / timed out.
fn probe_output(program: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = command(program);
    cmd.args(args);
    let out = run_cancellable(cmd, &AtomicBool::new(false), PROBE_TIMEOUT).ok()?;
    (!out.timed_out).then_some(out.text)
}

/// First line `parse` accepts (warnings may come before the version).
fn parse_lines<T>(s: &str, parse: impl Fn(&str) -> Option<T>) -> Option<T> {
    s.lines().find_map(|l| parse(l.trim()))
}

fn first_line(s: &str) -> Option<String> {
    s.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_owned)
}

/// First `X.Y[.Z]` in `s` (`v1.29.380`, `gh version 2.40.1 (2023-12-13)`, `glab 1.46.1 (abc)`).
pub(crate) fn parse_cli_version(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit()
            && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'v')
        {
            let end = s[i..]
                .find(|c: char| !(c.is_ascii_digit() || c == '.'))
                .map_or(s.len(), |e| i + e);
            let run = s[i..end].trim_end_matches('.');
            if run.split('.').filter(|p| !p.is_empty()).count() >= 2 {
                return Some(run.to_owned());
            }
            i = end;
        }
        i += 1;
    }
    None
}

/// `OpenSSH_for_Windows_9.5p1, LibreSSL 3.8.2` / `OpenSSH_9.6p1, OpenSSL 3.0.13 30 Jan 2024`.
pub(crate) fn parse_ssh_version(s: &str) -> Option<String> {
    let at = s.find("OpenSSH_")? + "OpenSSH_".len();
    let rest = s[at..].strip_prefix("for_Windows_").unwrap_or(&s[at..]);
    let v: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '.')
        .collect();
    v.starts_with(|c: char| c.is_ascii_digit()).then_some(v)
}

// ----- probing -------------------------------------------------------------------------------

pub(crate) fn download_url(tool: SetupToolId, os: TargetOs) -> &'static str {
    match (tool, os) {
        (SetupToolId::Git, TargetOs::Windows) => "https://git-scm.com/downloads/win",
        (SetupToolId::Git, TargetOs::Macos) => "https://git-scm.com/downloads/mac",
        (SetupToolId::Git, TargetOs::Linux) => "https://git-scm.com/downloads/linux",
        (SetupToolId::GitLfs, _) => "https://git-lfs.com/",
        (SetupToolId::Ssh, TargetOs::Windows) => {
            "https://learn.microsoft.com/windows-server/administration/openssh/openssh_install_firstuse"
        }
        (SetupToolId::Ssh, _) => "https://www.openssh.com/",
        (SetupToolId::Gh, _) => "https://cli.github.com/",
        (SetupToolId::Glab, _) => "https://gitlab.com/gitlab-org/cli",
        (SetupToolId::Webview2, _) => "https://developer.microsoft.com/microsoft-edge/webview2/",
    }
}

/// Package managers found on this machine.
#[derive(Debug, Clone, Default)]
pub(crate) struct Managers {
    pub winget: Option<PathBuf>,
    pub brew: Option<PathBuf>,
    /// An Intel Mac: Homebrew no longer has git-lfs bottles for it and building from source
    /// fails, so git-lfs comes from the official release instead.
    pub mac_intel: bool,
}

/// Apple silicon answers `hw.optional.arm64: 1`, also when this process runs under Rosetta.
fn is_mac_intel() -> bool {
    !command(Path::new("/usr/sbin/sysctl"))
        .args(["-n", "hw.optional.arm64"])
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "1")
}

impl Managers {
    pub(crate) fn detect(env: &ProbeEnv) -> Self {
        match env.os {
            TargetOs::Windows => Self {
                winget: which("winget", env).or_else(|| {
                    let dir = env
                        .local_app_data
                        .as_ref()?
                        .join("Microsoft")
                        .join("WindowsApps");
                    find_in(&dir, "winget", env)
                }),
                brew: None,
                mac_intel: false,
            },
            TargetOs::Macos => Self {
                winget: None,
                brew: locate(SetupToolId::Git, "brew", env).map(|(p, _)| p),
                mac_intel: is_mac_intel(),
            },
            TargetOs::Linux => Self::default(),
        }
    }
}

/// The whitelisted install command of `tool`, if this OS / machine can install it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InstallPlan {
    pub manager: SetupPackageManager,
    pub program: PathBuf,
    pub args: Vec<&'static str>,
}

/// Homebrew side jobs that make `brew install` look frozen from a GUI (no output reaches the
/// user until the process exits): `brew update` of every tap before installing (minutes with
/// old git-cloned taps), upgrading the dependents of anything it touched, and `brew cleanup`.
/// Formula metadata is still downloaded when it is missing, so a first install works.
const BREW_ENV: [(&str, &str); 4] = [
    ("HOMEBREW_NO_AUTO_UPDATE", "1"),
    ("HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK", "1"),
    ("HOMEBREW_NO_INSTALL_CLEANUP", "1"),
    ("HOMEBREW_NO_ENV_HINTS", "1"),
];

impl InstallPlan {
    /// The process to spawn for this plan.
    pub(crate) fn command(&self) -> Command {
        let mut cmd = command(&self.program);
        cmd.args(&self.args);
        if self.manager == SetupPackageManager::Brew {
            cmd.envs(BREW_ENV);
        }
        cmd
    }

    pub(crate) fn display(&self) -> String {
        if self.manager == SetupPackageManager::GithubRelease {
            return format!("download {}", lfs_release::RELEASES_PAGE);
        }
        let name = self
            .program
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        std::iter::once(name.as_str())
            .chain(self.args.iter().copied())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

pub(crate) fn install_plan(tool: SetupToolId, os: TargetOs, m: &Managers) -> Option<InstallPlan> {
    match os {
        TargetOs::Windows => {
            let id = match tool {
                SetupToolId::Git => "Git.Git",
                SetupToolId::GitLfs => "GitHub.GitLFS",
                SetupToolId::Gh => "GitHub.cli",
                SetupToolId::Glab => "GLab.GLab",
                SetupToolId::Ssh | SetupToolId::Webview2 => return None,
            };
            Some(InstallPlan {
                manager: SetupPackageManager::Winget,
                program: m.winget.clone()?,
                args: vec![
                    "install",
                    "--id",
                    id,
                    "-e",
                    "--source",
                    "winget",
                    "--accept-package-agreements",
                    "--accept-source-agreements",
                ],
            })
        }
        TargetOs::Macos => {
            let formula = match tool {
                SetupToolId::Git => {
                    return Some(InstallPlan {
                        manager: SetupPackageManager::XcodeSelect,
                        program: PathBuf::from("/usr/bin/xcode-select"),
                        args: vec!["--install"],
                    });
                }
                SetupToolId::GitLfs if m.mac_intel || m.brew.is_none() => {
                    return Some(InstallPlan {
                        manager: SetupPackageManager::GithubRelease,
                        program: PathBuf::new(),
                        args: vec![],
                    });
                }
                SetupToolId::GitLfs => "git-lfs",
                SetupToolId::Gh => "gh",
                SetupToolId::Glab => "glab",
                SetupToolId::Ssh | SetupToolId::Webview2 => return None,
            };
            Some(InstallPlan {
                manager: SetupPackageManager::Brew,
                program: m.brew.clone()?,
                args: vec!["install", formula],
            })
        }
        TargetOs::Linux => None,
    }
}

fn package_manager(os: TargetOs) -> Option<SetupPackageManager> {
    match os {
        TargetOs::Windows => Some(SetupPackageManager::Winget),
        TargetOs::Macos => Some(SetupPackageManager::Brew),
        TargetOs::Linux => None,
    }
}

struct Found {
    path: Option<PathBuf>,
    on_path: bool,
    raw: Option<String>,
}

fn state_of(found: &Found, version_ok: Option<bool>) -> SetupToolState {
    match (
        found.path.is_some() || found.raw.is_some(),
        found.on_path,
        version_ok,
    ) {
        (false, _, _) => SetupToolState::Missing,
        (true, false, _) => SetupToolState::OutsidePath,
        (true, true, Some(true)) => SetupToolState::Ok,
        (true, true, Some(false)) => SetupToolState::Outdated,
        // On PATH but its output is not a version (broken install, shim): treat as missing.
        (true, true, None) => SetupToolState::Missing,
    }
}

fn found_with(tool: SetupToolId, name: &str, args: &[&str], env: &ProbeEnv) -> Found {
    match locate(tool, name, env) {
        Some((p, on_path)) => Found {
            raw: probe_output(&p, args),
            path: Some(p),
            on_path,
        },
        None => Found {
            path: None,
            on_path: false,
            raw: None,
        },
    }
}

/// macOS ships `/usr/bin/git` as a stub that pops up the Command Line Tools installer when run;
/// only run it once `xcode-select -p` says the tools are there.
fn mac_git_stub_without_tools(p: &Path, env: &ProbeEnv) -> bool {
    env.os == TargetOs::Macos
        && p == Path::new("/usr/bin/git")
        && !command(Path::new("/usr/bin/xcode-select"))
            .arg("-p")
            .output()
            .is_ok_and(|o| o.status.success())
}

/// Checks every tool. `webview` is the runtime version (Windows only; `None` elsewhere).
pub(crate) fn probe_tools(env: &ProbeEnv, webview: Option<String>) -> SetupToolsReport {
    let managers = Managers::detect(env);
    let os = env.os;
    let mut tools = Vec::new();
    let mut push = |id: SetupToolId,
                    importance: SetupImportance,
                    found: &Found,
                    version: Option<String>,
                    version_ok: Option<bool>,
                    minimum: Option<String>| {
        tools.push(SetupTool {
            id,
            importance,
            state: state_of(found, version_ok),
            version,
            minimum,
            path: found.path.as_ref().map(|p| p.display().to_string()),
            raw: found.raw.as_deref().and_then(first_line),
            download_url: download_url(id, os).to_owned(),
            installer: install_plan(id, os, &managers).map(|p| p.manager),
        });
    };

    // git
    let git = match locate(SetupToolId::Git, "git", env) {
        Some((p, _)) if mac_git_stub_without_tools(&p, env) => Found {
            path: None,
            on_path: false,
            raw: None,
        },
        Some((p, on_path)) => Found {
            raw: probe_output(&p, &["--version"]),
            path: Some(p),
            on_path,
        },
        None => Found {
            path: None,
            on_path: false,
            raw: None,
        },
    };
    let git_version = git
        .raw
        .as_deref()
        .and_then(|r| parse_lines(r, parse_git_version));
    push(
        SetupToolId::Git,
        SetupImportance::Required,
        &git,
        git_version.map(|v| v.to_string()),
        git_version.map(|v| v >= MIN_GIT),
        Some(MIN_GIT.to_string()),
    );
    let git_usable = git.on_path && git_version.is_some();

    // git-lfs: standalone on PATH, else bundled with git (Git for Windows), else default folders.
    let mut lfs = found_with(SetupToolId::GitLfs, "git-lfs", &["version"], env);
    if !lfs.on_path
        && git_usable
        && let Some(git_path) = &git.path
        && let Some(raw) = probe_output(git_path, &["lfs", "version"])
        && parse_lines(&raw, parse_lfs_version).is_some()
    {
        lfs = Found {
            path: None,
            on_path: true,
            raw: Some(raw),
        };
    }
    let lfs_version = lfs
        .raw
        .as_deref()
        .and_then(|r| parse_lines(r, parse_lfs_version));
    push(
        SetupToolId::GitLfs,
        SetupImportance::Recommended,
        &lfs,
        lfs_version.map(|v| v.to_string()),
        lfs_version.map(|v| v >= MIN_GIT_LFS),
        Some(MIN_GIT_LFS.to_string()),
    );

    // OpenSSH: on Windows git uses its own `<root>/usr/bin/ssh.exe`, as the app does
    // (git is `<root>/cmd/git.exe` or `<root>/mingw64/bin/git.exe`).
    let bundled_ssh = match (&git.path, os) {
        (Some(p), TargetOs::Windows) => p
            .ancestors()
            .skip(2)
            .take(2)
            .find_map(|root| find_in(&root.join("usr").join("bin"), "ssh", env)),
        _ => None,
    };
    let ssh = match bundled_ssh {
        Some(p) => Found {
            raw: probe_output(&p, &["-V"]),
            on_path: git.on_path,
            path: Some(p),
        },
        None => found_with(SetupToolId::Ssh, "ssh", &["-V"], env),
    };
    let ssh_version = ssh
        .raw
        .as_deref()
        .and_then(|r| parse_lines(r, parse_ssh_version));
    let ok = ssh_version.as_ref().map(|_| true);
    push(
        SetupToolId::Ssh,
        SetupImportance::Recommended,
        &ssh,
        ssh_version,
        ok,
        None,
    );

    for (id, name) in [(SetupToolId::Gh, "gh"), (SetupToolId::Glab, "glab")] {
        let found = found_with(id, name, &["--version"], env);
        let version = found.raw.as_deref().and_then(parse_cli_version);
        let ok = version.as_ref().map(|_| true);
        push(id, SetupImportance::Optional, &found, version, ok, None);
    }

    if os == TargetOs::Windows {
        let found = Found {
            path: None,
            on_path: webview.is_some(),
            raw: webview.clone(),
        };
        let ok = webview.as_ref().map(|_| true);
        push(
            SetupToolId::Webview2,
            SetupImportance::Required,
            &found,
            webview,
            ok,
            None,
        );
    }

    let can_continue = tools
        .iter()
        .any(|t| t.id == SetupToolId::Git && t.state == SetupToolState::Ok);
    let restart_needed = tools.iter().any(|t| t.state == SetupToolState::OutsidePath);
    let package_manager = package_manager(os);
    SetupToolsReport {
        tools,
        can_continue,
        restart_needed,
        package_manager,
        package_manager_found: managers.winget.is_some() || managers.brew.is_some(),
    }
}

// ----- installing ----------------------------------------------------------------------------

/// winget HRESULTs (`APPINSTALLER_CLI_ERROR_*`) as process exit codes.
const WINGET_NO_PACKAGE: u32 = 0x8A15_0014;
const WINGET_UPDATE_NOT_APPLICABLE: u32 = 0x8A15_002B;
const WINGET_ALREADY_INSTALLED: u32 = 0x8A15_0061;

/// Turns an installer run into something a beginner can act on.
pub(crate) fn classify(
    manager: SetupPackageManager,
    exit_code: Option<i32>,
    output: &str,
) -> SetupInstallOutcome {
    let lower = output.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| lower.contains(n));
    let code = exit_code.map(|c| c as u32);
    let already = has(&[
        "already installed",
        "no available upgrade found",
        "no newer package versions",
    ]) || matches!(
        code,
        Some(WINGET_ALREADY_INSTALLED | WINGET_UPDATE_NOT_APPLICABLE)
    );
    match (manager, exit_code) {
        (SetupPackageManager::XcodeSelect, Some(0)) => return SetupInstallOutcome::Started,
        (_, Some(0)) => {
            return if already {
                SetupInstallOutcome::AlreadyInstalled
            } else {
                SetupInstallOutcome::Installed
            };
        }
        _ => {}
    }
    if already {
        SetupInstallOutcome::AlreadyInstalled
    } else if has(&[
        "exit code: 1223",
        "exit code: 1602",
        "cancelled by the user",
        "canceled by the user",
    ]) {
        SetupInstallOutcome::UserCancelled
    } else if has(&[
        "administrator",
        "elevat",
        "0x80070005",
        "access is denied",
        "permission denied",
        "exit code: 740",
    ]) {
        SetupInstallOutcome::NeedsAdmin
    } else if has(&[
        "0x80072ee7",
        "0x80072efd",
        "0x80072f8f",
        "internetopenurl",
        "failed when searching source",
        "could not resolve host",
        "failed to connect",
        "network",
    ]) {
        SetupInstallOutcome::Network
    } else if code == Some(WINGET_NO_PACKAGE)
        || has(&[
            "no package found matching",
            "no available formula",
            "no formulae found",
        ])
    {
        SetupInstallOutcome::PackageNotFound
    } else {
        SetupInstallOutcome::Failed
    }
}

/// Keeps the last lines a person can read: progress bars and spinners redrawn with `\r` are
/// reduced to their final state, lines made only of spinner/bar glyphs dropped.
pub(crate) fn clean_output(raw: &str) -> String {
    let lines: Vec<String> = raw
        .split('\n')
        .map(|l| {
            l.rsplit('\r')
                .find(|s| !s.trim().is_empty())
                .unwrap_or("")
                .trim_end()
        })
        .filter(|l| {
            let t = l.trim();
            !t.is_empty()
                && !t
                    .chars()
                    .all(|c| matches!(c, '-' | '\\' | '|' | '/' | '█' | '▒' | ' '))
        })
        .map(str::to_owned)
        .collect();
    let start = lines.len().saturating_sub(OUTPUT_LINES);
    nanomgg_logging::redact(&lines[start..].join("\n")).into_owned()
}

/// Readable output lines kept in [`SetupInstallProgress::recent`].
const RECENT_LINES: usize = 5;
/// Progress-bar redraws are reported at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(150);

/// The number right before a `%` (`######## 45.0%`, `██▒▒  45%`).
pub(crate) fn percent_in(line: &str) -> Option<u8> {
    let at = line.rfind('%')?;
    let head = &line[..at];
    let start = head
        .rfind(|c: char| !(c.is_ascii_digit() || c == '.'))
        .map_or(0, |i| i + 1);
    let v: f32 = head[start..].parse().ok()?;
    (0.0..=100.0).contains(&v).then(|| v as u8)
}

fn bytes_of(num: &str, unit: &str) -> Option<f64> {
    let n: f64 = num.parse().ok()?;
    let k = match unit.to_ascii_uppercase().as_str() {
        "B" => 1.0,
        "KB" | "KIB" => 1024.0,
        "MB" | "MIB" => 1024.0 * 1024.0,
        "GB" | "GIB" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some(n * k)
}

/// `done / total` sizes as a percent (winget: `██████▒▒▒▒  10.0 MB / 60.0 MB`).
pub(crate) fn sizes_percent(line: &str) -> Option<u8> {
    let (left, right) = line.split_once(" / ")?;
    let mut l = left.split_whitespace().rev();
    let (lu, ln) = (l.next()?, l.next()?);
    let mut r = right.split_whitespace();
    let (rn, ru) = (r.next()?, r.next()?);
    let (done, total) = (bytes_of(ln, lu)?, bytes_of(rn, ru)?);
    (total > 0.0 && done <= total).then(|| (done * 100.0 / total) as u8)
}

/// The phase a winget / Homebrew output line announces.
pub(crate) fn phase_of(line: &str) -> Option<SetupInstallPhase> {
    use SetupInstallPhase as P;
    let l = line.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| l.contains(n));
    if has(&[
        "successfully installed",
        "🍺",
        "==> summary",
        "==> caveats",
        "linking ",
    ]) {
        Some(P::Finishing)
    } else if has(&["verif", "checksum"]) {
        Some(P::Verifying)
    } else if has(&[
        "pouring",
        "installing",
        "starting package install",
        "extracting",
    ]) {
        Some(P::Installing)
    } else if has(&["downloading", "fetching", "download"]) {
        Some(P::Downloading)
    } else {
        None
    }
}

/// Follows an installer's output and turns it into [`SetupInstallProgress`] events.
pub(crate) struct ProgressTracker {
    tool: SetupToolId,
    phase: SetupInstallPhase,
    percent: Option<u8>,
    recent: std::collections::VecDeque<String>,
    last_emit: Option<Instant>,
}

impl ProgressTracker {
    pub(crate) fn new(tool: SetupToolId) -> Self {
        Self {
            tool,
            phase: SetupInstallPhase::Preparing,
            percent: None,
            recent: Default::default(),
            last_emit: None,
        }
    }

    pub(crate) fn snapshot(&self) -> SetupInstallProgress {
        SetupInstallProgress {
            tool: self.tool,
            phase: self.phase,
            percent: self.percent,
            recent: self.recent.iter().cloned().collect(),
        }
    }

    /// Moves to `phase` (a new phase starts without a percent).
    pub(crate) fn set_phase(&mut self, phase: SetupInstallPhase) {
        if phase != self.phase {
            self.phase = phase;
            self.percent = None;
        }
    }

    pub(crate) fn set_percent(&mut self, percent: Option<u8>) {
        self.percent = percent;
    }

    pub(crate) fn note(&mut self, line: &str) {
        let line = nanomgg_logging::redact(line.trim()).into_owned();
        if self.recent.back() != Some(&line) {
            if self.recent.len() == RECENT_LINES {
                self.recent.pop_front();
            }
            self.recent.push_back(line);
        }
    }

    /// Reads one output line; returns whether it is worth an event now.
    pub(crate) fn feed(&mut self, line: &str) -> bool {
        let t = line.trim();
        if t.is_empty() {
            return false;
        }
        let pct = percent_in(t).or_else(|| sizes_percent(t));
        // A redrawn bar (`#####  45.0%`) has no words; it only moves the percent.
        let bar_only = pct.is_some()
            && !t
                .split_whitespace()
                .any(|w| w.chars().filter(|c| c.is_alphabetic()).count() >= 4);
        if bar_only {
            if self.phase == SetupInstallPhase::Preparing {
                self.set_phase(SetupInstallPhase::Downloading);
            }
            self.percent = pct;
            return self.due();
        }
        if t.chars()
            .all(|c| matches!(c, '-' | '\\' | '|' | '/' | '█' | '▒' | '#' | '=' | ' '))
        {
            return false;
        }
        if let Some(phase) = phase_of(t) {
            self.set_phase(phase);
        }
        self.note(t);
        true
    }

    pub(crate) fn percent(&self) -> Option<u8> {
        self.percent
    }

    /// A progress-only change may be reported (not too soon after the last event).
    pub(crate) fn due(&self) -> bool {
        self.last_emit
            .is_none_or(|at| at.elapsed() >= PROGRESS_EVERY)
    }

    pub(crate) fn emitted(&mut self) {
        self.last_emit = Some(Instant::now());
    }
}

pub(crate) fn run_install(
    tool: SetupToolId,
    plan: &InstallPlan,
    cancel: &AtomicBool,
    timeout: Duration,
    emit: &mut dyn FnMut(SetupInstallProgress),
) -> SetupInstallResult {
    let cmd = plan.command();
    let result = |outcome, exit_code, output: String| SetupInstallResult {
        tool,
        outcome,
        manager: Some(plan.manager),
        command: Some(plan.display()),
        exit_code,
        output,
    };
    let mut tracker = ProgressTracker::new(tool);
    tracker.note(&plan.display());
    emit(tracker.snapshot());
    tracker.emitted();
    let mut on_line = |line: &str| {
        if tracker.feed(line) {
            emit(tracker.snapshot());
            tracker.emitted();
        }
    };
    match run_streaming(cmd, cancel, timeout, &mut on_line) {
        Err(e) => result(SetupInstallOutcome::ManagerMissing, None, e.to_string()),
        Ok(out) => {
            let output = clean_output(&out.text);
            let outcome = if out.cancelled {
                SetupInstallOutcome::Cancelled
            } else if out.timed_out {
                SetupInstallOutcome::TimedOut
            } else {
                classify(plan.manager, out.exit_code, &out.text)
            };
            tracing::info!(?tool, ?outcome, code = ?out.exit_code, "setup install finished");
            result(outcome, out.exit_code, output)
        }
    }
}

/// Cancel flag of the install in progress (one at a time).
static INSTALLING: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

fn installing() -> std::sync::MutexGuard<'static, Option<Arc<AtomicBool>>> {
    INSTALLING.lock().unwrap_or_else(|e| e.into_inner())
}

struct InstallSlot;

impl InstallSlot {
    fn take() -> AppResult<(Self, Arc<AtomicBool>)> {
        let mut slot = installing();
        if slot.is_some() {
            return Err(AppError::InvalidInput(
                "another installation is already running".into(),
            ));
        }
        let flag = Arc::new(AtomicBool::new(false));
        *slot = Some(flag.clone());
        Ok((Self, flag))
    }
}

impl Drop for InstallSlot {
    fn drop(&mut self) {
        *installing() = None;
    }
}

fn manager_missing(tool: SetupToolId, os: TargetOs) -> SetupInstallResult {
    SetupInstallResult {
        tool,
        outcome: SetupInstallOutcome::ManagerMissing,
        manager: package_manager(os),
        command: None,
        exit_code: None,
        output: String::new(),
    }
}

/// Installs `tool` with a package-manager command (the GitHub release is installed by
/// [`setup_install`] itself).
pub(crate) fn install_impl(
    tool: SetupToolId,
    env: &ProbeEnv,
    timeout: Duration,
    emit: &mut dyn FnMut(SetupInstallProgress),
) -> AppResult<SetupInstallResult> {
    let managers = Managers::detect(env);
    let Some(plan) = install_plan(tool, env.os, &managers) else {
        return Ok(manager_missing(tool, env.os));
    };
    let (_slot, cancel) = InstallSlot::take()?;
    tracing::info!(?tool, command = %plan.display(), "setup install started");
    Ok(run_install(tool, &plan, &cancel, timeout, emit))
}

fn cancel_install() -> bool {
    match installing().as_ref() {
        Some(flag) => {
            flag.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

// ----- progress ------------------------------------------------------------------------------

pub(crate) fn status_impl(st: &AppState) -> AppResult<SetupStatus> {
    let progress: SetupProgress = st.storage.get_setting(KEY_SETUP)?.unwrap_or_default();
    let import_done = st
        .storage
        .get_setting::<bool>(KEY_IMPORT_DONE)?
        .unwrap_or(false);
    let account_count = st.storage.accounts().map_err(storage_err)?.len() as u32;
    Ok(SetupStatus {
        should_auto_open: !progress.completed
            && !progress.skipped
            && !import_done
            && account_count == 0,
        progress,
        account_count,
        import_done,
    })
}

pub(crate) fn save_impl(st: &AppState, mut progress: SetupProgress) -> AppResult<SetupStatus> {
    let mut seen = Vec::new();
    progress.steps_done.retain(|s| {
        let new = !seen.contains(s);
        seen.push(*s);
        new
    });
    st.storage.set_setting(KEY_SETUP, &progress)?;
    status_impl(st)
}

// ----- commands ------------------------------------------------------------------------------

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppHandle, &AppState) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || f(&app, &app.state::<AppState>()))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn setup_status(app: AppHandle) -> AppResult<SetupStatus> {
    blocking(app, |_, st| status_impl(st)).await
}

/// Saves the whole progress object (finish, skip, steps done).
#[tauri::command]
#[specta::specta]
pub async fn setup_save(app: AppHandle, progress: SetupProgress) -> AppResult<SetupStatus> {
    blocking(app, move |_, st| save_impl(st, progress)).await
}

#[tauri::command]
#[specta::specta]
pub async fn setup_tools() -> AppResult<SetupToolsReport> {
    tauri::async_runtime::spawn_blocking(|| {
        let env = ProbeEnv::current();
        let webview = (env.os == TargetOs::Windows)
            .then(|| tauri::webview_version().ok())
            .flatten();
        probe_tools(&env, webview)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))
}

/// Installs `tool` with the whitelisted package-manager command of this OS (or, for git-lfs on
/// an Intel Mac / a Mac without Homebrew, the official release). Progress arrives as
/// [`SetupInstallProgress`].
#[tauri::command]
#[specta::specta]
pub async fn setup_install(app: AppHandle, tool: SetupToolId) -> AppResult<SetupInstallResult> {
    let handle = app.clone();
    let emit = move |p: SetupInstallProgress| {
        if let Err(e) = p.emit(&handle) {
            tracing::warn!(error = %e, "failed to emit setup install progress");
        }
    };
    let (env, managers) = tauri::async_runtime::spawn_blocking(|| {
        let env = ProbeEnv::current();
        let managers = Managers::detect(&env);
        (env, managers)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;
    match install_plan(tool, env.os, &managers) {
        Some(plan) if plan.manager == SetupPackageManager::GithubRelease => {
            let (_slot, cancel) = InstallSlot::take()?;
            tracing::info!(?tool, "setup install started (GitHub release)");
            let http = app.state::<crate::auth::AuthRuntime>().http.clone();
            let mut emit = emit;
            Ok(lfs_release::install(&http, managers.mac_intel, &cancel, &mut emit).await)
        }
        Some(_) => tauri::async_runtime::spawn_blocking(move || {
            let mut emit = emit;
            install_impl(tool, &env, INSTALL_TIMEOUT, &mut emit)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?,
        None => Ok(manager_missing(tool, env.os)),
    }
}

/// Stops the running install, if any.
#[tauri::command]
#[specta::specta]
pub async fn setup_install_cancel() -> AppResult<bool> {
    Ok(cancel_install())
}

/// Restarts the app so it picks up a `PATH` changed by an installer.
#[tauri::command]
#[specta::specta]
pub async fn setup_restart(app: AppHandle) -> AppResult<()> {
    app.restart()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::commands::{AccountInput, account_save_impl};
    use nanomgg_accounts::AuthMethod;
    use nanomgg_credentials::MemorySecretStore;
    use nanomgg_storage::Storage;
    use std::fs;

    fn state(tmp: &Path) -> AppState {
        let storage = Arc::new(Storage::open(&tmp.join("app.db")).unwrap());
        let rt = crate::accounts::AccountsRuntime::new(
            None,
            None,
            Arc::new(MemorySecretStore::default()),
        );
        AppState::with_accounts(storage, rt)
    }

    fn env(path: &[&Path], os: TargetOs) -> ProbeEnv {
        ProbeEnv {
            os,
            path: Some(std::env::join_paths(path).unwrap()),
            exts: if os == TargetOs::Windows {
                vec![".exe".into(), ".cmd".into()]
            } else {
                Vec::new()
            },
            program_files: Vec::new(),
            local_app_data: None,
            system_root: None,
            unix_dirs: Vec::new(),
        }
    }

    /// A fake program printing `out` (to stderr if `to_stderr`) and exiting with `code`.
    fn fake(dir: &Path, name: &str, out: &str, to_stderr: bool, code: i32) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        if cfg!(windows) {
            let p = dir.join(format!("{name}.cmd"));
            let redirect = if to_stderr { " 1>&2" } else { "" };
            let body: String = out
                .lines()
                .map(|l| format!("echo {l}{redirect}\r\n"))
                .collect();
            fs::write(&p, format!("@echo off\r\n{body}exit /b {code}\r\n")).unwrap();
            p
        } else {
            let p = dir.join(name);
            let redirect = if to_stderr { " 1>&2" } else { "" };
            let body: String = out
                .lines()
                .map(|l| format!("echo '{l}'{redirect}\n"))
                .collect();
            fs::write(&p, format!("#!/bin/sh\n{body}exit {code}\n")).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
            }
            p
        }
    }

    fn os() -> TargetOs {
        TargetOs::current()
    }

    fn tool(r: &SetupToolsReport, id: SetupToolId) -> &SetupTool {
        r.tools.iter().find(|t| t.id == id).unwrap()
    }

    /// Read-only look at this machine (`--version` only): `cargo test -- --ignored real_machine`.
    #[test]
    #[ignore]
    fn real_machine_probe() {
        let r = probe_tools(&ProbeEnv::current(), None);
        for t in &r.tools {
            eprintln!(
                "{:?} {:?} {:?} {:?} {:?}",
                t.id, t.state, t.version, t.path, t.installer
            );
        }
        eprintln!(
            "continue={} manager_found={}",
            r.can_continue, r.package_manager_found
        );
    }

    #[test]
    fn parses_cli_versions() {
        assert_eq!(parse_cli_version("v1.29.380").as_deref(), Some("1.29.380"));
        assert_eq!(
            parse_cli_version(
                "gh version 2.40.1 (2023-12-13)\nhttps://github.com/cli/cli/releases/tag/v2.40.1"
            )
            .as_deref(),
            Some("2.40.1")
        );
        assert_eq!(
            parse_cli_version("glab 1.46.1 (8a2bd6f3)").as_deref(),
            Some("1.46.1")
        );
        assert_eq!(
            parse_cli_version("glab version 1.36.0 (2024-01-18)").as_deref(),
            Some("1.36.0")
        );
        assert_eq!(
            parse_cli_version("Homebrew 4.2.5").as_deref(),
            Some("4.2.5")
        );
        assert_eq!(parse_cli_version("not installed"), None);
        assert_eq!(parse_cli_version("x64 build 7"), None);
    }

    #[test]
    fn parses_ssh_versions() {
        assert_eq!(
            parse_ssh_version("OpenSSH_for_Windows_9.5p1, LibreSSL 3.8.2").as_deref(),
            Some("9.5p1")
        );
        assert_eq!(
            parse_ssh_version("OpenSSH_9.6p1, OpenSSL 3.0.13 30 Jan 2024").as_deref(),
            Some("9.6p1")
        );
        assert_eq!(parse_ssh_version("ssh: command not found"), None);
    }

    #[test]
    fn install_commands_are_a_fixed_whitelist() {
        let winget = Managers {
            winget: Some(PathBuf::from("C:/wa/winget.exe")),
            ..Default::default()
        };
        let plan = install_plan(SetupToolId::Git, TargetOs::Windows, &winget).unwrap();
        assert_eq!(plan.manager, SetupPackageManager::Winget);
        assert_eq!(plan.program, PathBuf::from("C:/wa/winget.exe"));
        assert_eq!(
            plan.args,
            [
                "install",
                "--id",
                "Git.Git",
                "-e",
                "--source",
                "winget",
                "--accept-package-agreements",
                "--accept-source-agreements"
            ]
        );
        assert_eq!(
            plan.display(),
            "winget install --id Git.Git -e --source winget --accept-package-agreements --accept-source-agreements"
        );
        for (tool, id) in [
            (SetupToolId::GitLfs, "GitHub.GitLFS"),
            (SetupToolId::Gh, "GitHub.cli"),
            (SetupToolId::Glab, "GLab.GLab"),
        ] {
            assert_eq!(
                install_plan(tool, TargetOs::Windows, &winget).unwrap().args[2],
                id
            );
        }
        assert!(install_plan(SetupToolId::Ssh, TargetOs::Windows, &winget).is_none());
        assert!(install_plan(SetupToolId::Webview2, TargetOs::Windows, &winget).is_none());
        // No winget → no button.
        assert!(install_plan(SetupToolId::Git, TargetOs::Windows, &Managers::default()).is_none());

        let brew = Managers {
            brew: Some(PathBuf::from("/opt/homebrew/bin/brew")),
            ..Default::default()
        };
        let git = install_plan(SetupToolId::Git, TargetOs::Macos, &Managers::default()).unwrap();
        assert_eq!(git.manager, SetupPackageManager::XcodeSelect);
        assert_eq!(git.args, ["--install"]);
        let lfs = install_plan(SetupToolId::GitLfs, TargetOs::Macos, &brew).unwrap();
        assert_eq!(lfs.manager, SetupPackageManager::Brew);
        assert_eq!(lfs.args, ["install", "git-lfs"]);
        let cmd = lfs.command();
        let env: Vec<_> = cmd.get_envs().collect();
        assert!(env.contains(&("HOMEBREW_NO_AUTO_UPDATE".as_ref(), Some("1".as_ref()))));
        assert!(env.contains(&("HOMEBREW_NO_INSTALL_CLEANUP".as_ref(), Some("1".as_ref()))));
        // Intel Mac, or no Homebrew: git-lfs comes from the official release instead.
        for m in [
            Managers {
                mac_intel: true,
                ..brew.clone()
            },
            Managers::default(),
        ] {
            let plan = install_plan(SetupToolId::GitLfs, TargetOs::Macos, &m).unwrap();
            assert_eq!(plan.manager, SetupPackageManager::GithubRelease);
            assert!(
                plan.display()
                    .contains("github.com/git-lfs/git-lfs/releases")
            );
        }
        // Other tools still need Homebrew.
        assert!(install_plan(SetupToolId::Gh, TargetOs::Macos, &Managers::default()).is_none());
        for tool in [SetupToolId::Git, SetupToolId::GitLfs, SetupToolId::Gh] {
            assert!(install_plan(tool, TargetOs::Linux, &brew).is_none());
        }
    }

    #[test]
    fn download_pages_are_https() {
        for os in [TargetOs::Windows, TargetOs::Macos, TargetOs::Linux] {
            for t in [
                SetupToolId::Git,
                SetupToolId::GitLfs,
                SetupToolId::Ssh,
                SetupToolId::Gh,
                SetupToolId::Glab,
                SetupToolId::Webview2,
            ] {
                assert!(download_url(t, os).starts_with("https://"));
            }
        }
    }

    #[test]
    fn probes_fake_tools_on_a_fake_path() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        fake(&bin, "git", "git version 2.47.0.windows.1", false, 0);
        fake(
            &bin,
            "git-lfs",
            "git-lfs/3.5.1 (GitHub; windows amd64; go 1.21.7)",
            false,
            0,
        );
        fake(
            &bin,
            "ssh",
            "OpenSSH_for_Windows_9.5p1, LibreSSL 3.8.2",
            true,
            0,
        );
        fake(&bin, "gh", "gh version 2.40.1 (2023-12-13)", false, 0);
        let r = probe_tools(&env(&[&bin], os()), Some("130.0.2849.80".into()));
        let git = tool(&r, SetupToolId::Git);
        assert_eq!(git.state, SetupToolState::Ok);
        assert_eq!(git.version.as_deref(), Some("2.47.0"));
        assert_eq!(git.importance, SetupImportance::Required);
        assert!(git.path.as_deref().unwrap().contains("git"));
        assert_eq!(
            tool(&r, SetupToolId::GitLfs).version.as_deref(),
            Some("3.5.1")
        );
        let ssh = tool(&r, SetupToolId::Ssh);
        assert_eq!(
            (ssh.state, ssh.version.as_deref()),
            (SetupToolState::Ok, Some("9.5p1"))
        );
        assert_eq!(tool(&r, SetupToolId::Gh).version.as_deref(), Some("2.40.1"));
        assert_eq!(tool(&r, SetupToolId::Glab).state, SetupToolState::Missing);
        assert!(r.can_continue && !r.restart_needed);
        if os() == TargetOs::Windows {
            let wv = tool(&r, SetupToolId::Webview2);
            assert_eq!(
                (wv.state, wv.version.as_deref()),
                (SetupToolState::Ok, Some("130.0.2849.80"))
            );
        }
    }

    #[test]
    fn old_or_missing_git_blocks_and_lfs_only_warns() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        fake(&bin, "git", "git version 2.30.1", false, 0);
        fake(
            &bin,
            "git-lfs",
            "git-lfs/2.13.3 (GitHub; linux amd64)",
            false,
            0,
        );
        let r = probe_tools(&env(&[&bin], os()), None);
        assert_eq!(tool(&r, SetupToolId::Git).state, SetupToolState::Outdated);
        let lfs = tool(&r, SetupToolId::GitLfs);
        assert_eq!(
            (lfs.state, lfs.importance),
            (SetupToolState::Outdated, SetupImportance::Recommended)
        );
        assert!(!r.can_continue);

        let empty = tmp.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        let r = probe_tools(&env(&[&empty], os()), None);
        assert_eq!(tool(&r, SetupToolId::Git).state, SetupToolState::Missing);
        assert!(!r.can_continue);
        assert_eq!(
            tool(&r, SetupToolId::Git).download_url,
            download_url(SetupToolId::Git, os())
        );
    }

    #[test]
    fn lfs_bundled_with_git_is_found_through_git() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        // One fake answers both `git --version` and `git lfs version`; each parser finds its line.
        fake(
            &bin,
            "git",
            "git version 2.47.0\ngit-lfs/3.6.0 (GitHub; windows amd64)",
            false,
            0,
        );
        let r = probe_tools(&env(&[&bin], os()), None);
        let lfs = tool(&r, SetupToolId::GitLfs);
        assert_eq!(lfs.state, SetupToolState::Ok);
        assert_eq!(lfs.path, None);
    }

    #[test]
    fn git_installed_outside_path_needs_a_restart() {
        let tmp = tempfile::tempdir().unwrap();
        let empty = tmp.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        let mut e = env(&[&empty], os());
        match os() {
            TargetOs::Windows => {
                let pf = tmp.path().join("Program Files");
                fake(
                    &pf.join("Git").join("cmd"),
                    "git",
                    "git version 2.47.0.windows.1",
                    false,
                    0,
                );
                e.program_files = vec![pf];
            }
            // /opt/homebrew/bin etc. are real folders here; only check the Windows layout.
            _ => return,
        }
        let r = probe_tools(&e, None);
        let git = tool(&r, SetupToolId::Git);
        assert_eq!(git.state, SetupToolState::OutsidePath);
        assert_eq!(git.version.as_deref(), Some("2.47.0"));
        assert!(git.path.as_deref().unwrap().contains("Program Files"));
        assert!(r.restart_needed && !r.can_continue);
    }

    #[test]
    fn windows_fallback_dirs() {
        let e = ProbeEnv {
            os: TargetOs::Windows,
            path: None,
            exts: vec![".exe".into()],
            program_files: vec![PathBuf::from("C:/PF")],
            local_app_data: Some(PathBuf::from("C:/U/AppData/Local")),
            system_root: Some(PathBuf::from("C:/Windows")),
            unix_dirs: Vec::new(),
        };
        let git = fallback_dirs(SetupToolId::Git, &e);
        assert_eq!(git[0], Path::new("C:/PF/Git/cmd"));
        assert!(git.contains(&PathBuf::from("C:/U/AppData/Local/Programs/Git/cmd")));
        assert!(git.contains(&PathBuf::from("C:/U/AppData/Local/Microsoft/WinGet/Links")));
        assert!(fallback_dirs(SetupToolId::GitLfs, &e).contains(&PathBuf::from("C:/PF/Git LFS")));
        assert!(fallback_dirs(SetupToolId::Gh, &e).contains(&PathBuf::from("C:/PF/GitHub CLI")));
        assert!(
            fallback_dirs(SetupToolId::Ssh, &e)
                .contains(&PathBuf::from("C:/Windows/System32/OpenSSH"))
        );
        assert!(fallback_dirs(SetupToolId::Webview2, &e).is_empty());
    }

    #[test]
    fn detects_winget_on_fake_path() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        let p = fake(&bin, "winget", "v1.29.380", false, 0);
        let mut e = env(&[&bin], os());
        e.os = TargetOs::Windows;
        e.exts = vec![".exe".into(), ".cmd".into()];
        if os() != TargetOs::Windows {
            // The fake has no extension outside Windows.
            e.exts.push(String::new());
        }
        let m = Managers::detect(&e);
        assert_eq!(m.winget.as_deref(), Some(p.as_path()));
        let out = probe_output(&p, &["--version"]).unwrap();
        assert_eq!(parse_cli_version(&out).as_deref(), Some("1.29.380"));
    }

    #[test]
    fn classifies_installer_results() {
        use SetupInstallOutcome as O;
        use SetupPackageManager as M;
        assert_eq!(
            classify(M::Winget, Some(0), "Successfully installed"),
            O::Installed
        );
        assert_eq!(
            classify(
                M::Winget,
                Some(WINGET_ALREADY_INSTALLED as i32),
                "Found an existing package already installed."
            ),
            O::AlreadyInstalled
        );
        assert_eq!(
            classify(
                M::Winget,
                Some(-1978335226),
                "Installer failed with exit code: 1223"
            ),
            O::UserCancelled
        );
        assert_eq!(
            classify(
                M::Winget,
                Some(1),
                "This package requires administrator privileges"
            ),
            O::NeedsAdmin
        );
        assert_eq!(
            classify(
                M::Winget,
                Some(1),
                "Failed when searching source: winget\n0x80072ee7"
            ),
            O::Network
        );
        assert_eq!(
            classify(
                M::Winget,
                Some(WINGET_NO_PACKAGE as i32),
                "No package found matching input criteria."
            ),
            O::PackageNotFound
        );
        assert_eq!(
            classify(M::Brew, Some(1), "Error: something odd"),
            O::Failed
        );
        assert_eq!(
            classify(M::XcodeSelect, Some(0), "install requested"),
            O::Started
        );
        assert_eq!(
            classify(
                M::XcodeSelect,
                Some(1),
                "xcode-select: error: command line tools are already installed"
            ),
            O::AlreadyInstalled
        );
    }

    #[test]
    fn cleans_progress_bars() {
        let raw = "Found Git [Git.Git]\r\n  -\r  \\\r  |\r\n  ██████▒▒▒▒  10 MB / 60 MB\r  ██████████  60 MB / 60 MB\r\nSuccessfully installed\r\n";
        assert_eq!(
            clean_output(raw),
            "Found Git [Git.Git]\n  ██████████  60 MB / 60 MB\nSuccessfully installed"
        );
        let many: String = (0..200).map(|i| format!("line {i}\n")).collect();
        let cleaned = clean_output(&many);
        assert_eq!(cleaned.lines().count(), OUTPUT_LINES);
        assert!(cleaned.ends_with("line 199"));
    }

    #[test]
    fn runs_a_fake_installer_and_reports_its_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let p = fake(
            tmp.path(),
            "winget",
            "Installer failed with exit code: 1223",
            false,
            7,
        );
        let plan = InstallPlan {
            manager: SetupPackageManager::Winget,
            program: p,
            args: vec!["install", "--id", "Git.Git"],
        };
        let r = run_install(
            SetupToolId::Git,
            &plan,
            &AtomicBool::new(false),
            Duration::from_secs(30),
            &mut |_| {},
        );
        assert_eq!(r.outcome, SetupInstallOutcome::UserCancelled);
        assert_eq!(r.exit_code, Some(7));
        assert!(r.output.contains("1223"));
        assert_eq!(r.command.as_deref(), Some("winget install --id Git.Git"));
    }

    #[test]
    fn cancelling_stops_a_running_installer() {
        let tmp = tempfile::tempdir().unwrap();
        let p = if cfg!(windows) {
            let p = tmp.path().join("slow.cmd");
            fs::write(
                &p,
                "@echo off\r\necho working\r\nping -n 6 127.0.0.1 >nul\r\nexit /b 0\r\n",
            )
            .unwrap();
            p
        } else {
            let p = tmp.path().join("slow");
            fs::write(&p, "#!/bin/sh\necho working\nsleep 5\n").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
            }
            p
        };
        let plan = InstallPlan {
            manager: SetupPackageManager::Brew,
            program: p,
            args: vec![],
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let c2 = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            c2.store(true, Ordering::Relaxed);
        });
        let start = Instant::now();
        let r = run_install(
            SetupToolId::GitLfs,
            &plan,
            &cancel,
            Duration::from_secs(60),
            &mut |_| {},
        );
        assert_eq!(r.outcome, SetupInstallOutcome::Cancelled);
        assert_eq!(r.exit_code, None);
        assert!(
            start.elapsed() < Duration::from_secs(4),
            "{:?}",
            start.elapsed()
        );

        let r = run_install(
            SetupToolId::GitLfs,
            &plan,
            &AtomicBool::new(false),
            Duration::from_millis(300),
            &mut |_| {},
        );
        assert_eq!(r.outcome, SetupInstallOutcome::TimedOut);
    }

    #[test]
    fn reads_percent_and_phase_from_installer_output() {
        use SetupInstallPhase as P;
        assert_eq!(
            percent_in("######################           45.0%"),
            Some(45)
        );
        assert_eq!(percent_in("  ██████▒▒▒▒  100%"), Some(100));
        assert_eq!(percent_in("no percent here"), None);
        assert_eq!(percent_in("150%"), None);
        assert_eq!(sizes_percent("  ██████▒▒▒▒  15.0 MB / 60.0 MB"), Some(25));
        assert_eq!(sizes_percent("  ███  512 KB / 1.00 MB"), Some(50));
        assert_eq!(sizes_percent("a / b"), None);
        // winget
        assert_eq!(
            phase_of("Downloading https://github.com/git.exe"),
            Some(P::Downloading)
        );
        assert_eq!(
            phase_of("Successfully verified installer hash"),
            Some(P::Verifying)
        );
        assert_eq!(phase_of("Starting package install..."), Some(P::Installing));
        assert_eq!(phase_of("Successfully installed"), Some(P::Finishing));
        // Homebrew
        assert_eq!(
            phase_of("==> Fetching downloads for: git-lfs"),
            Some(P::Downloading)
        );
        assert_eq!(
            phase_of("==> Pouring git-lfs--3.7.1.bottle.tar.gz"),
            Some(P::Installing)
        );
        assert_eq!(
            phase_of("🍺  /usr/local/Cellar/git-lfs/3.7.1: 82 files"),
            Some(P::Finishing)
        );
        assert_eq!(phase_of("Found Git [Git.Git] Version 2.47.0"), None);
    }

    #[test]
    fn tracker_follows_the_phases_and_keeps_recent_lines() {
        let mut t = ProgressTracker::new(SetupToolId::Git);
        assert!(!t.feed("   "));
        assert!(!t.feed("  -  "));
        assert!(t.feed("Found Git [Git.Git] Version 2.47.0"));
        assert!(t.feed("Downloading https://github.com/git.exe"));
        assert_eq!(t.snapshot().phase, SetupInstallPhase::Downloading);
        // A redrawn bar only moves the percent and is not kept as a line.
        assert!(t.feed("  ██████▒▒▒▒  30.0 MB / 60.0 MB"));
        t.emitted();
        assert!(!t.feed("  ███████▒▒▒  35.0 MB / 60.0 MB"), "throttled");
        assert_eq!(t.snapshot().percent, Some(58));
        assert!(t.feed("Successfully verified installer hash"));
        let s = t.snapshot();
        assert_eq!((s.phase, s.percent), (SetupInstallPhase::Verifying, None));
        for i in 0..10 {
            t.feed(&format!("line {i}"));
        }
        let s = t.snapshot();
        assert_eq!(s.recent.len(), RECENT_LINES);
        assert_eq!(s.recent.last().map(String::as_str), Some("line 9"));
    }

    #[test]
    fn streams_output_lines_while_the_installer_runs() {
        let tmp = tempfile::tempdir().unwrap();
        let p = fake(
            tmp.path(),
            "winget",
            "Downloading https://example.test/x.exe\nSuccessfully verified installer hash\nSuccessfully installed",
            false,
            0,
        );
        let plan = InstallPlan {
            manager: SetupPackageManager::Winget,
            program: p,
            args: vec![],
        };
        let mut seen = Vec::new();
        let r = run_install(
            SetupToolId::Git,
            &plan,
            &AtomicBool::new(false),
            Duration::from_secs(30),
            &mut |e| seen.push(e),
        );
        assert_eq!(r.outcome, SetupInstallOutcome::Installed);
        let phases: Vec<_> = seen.iter().map(|e| e.phase).collect();
        assert_eq!(phases[0], SetupInstallPhase::Preparing);
        assert!(phases.contains(&SetupInstallPhase::Downloading));
        assert!(phases.contains(&SetupInstallPhase::Verifying));
        assert_eq!(phases.last(), Some(&SetupInstallPhase::Finishing));
        assert!(
            seen.last()
                .unwrap()
                .recent
                .iter()
                .any(|l| l == "Successfully installed")
        );
    }

    #[test]
    fn missing_manager_and_single_install_slot() {
        let tmp = tempfile::tempdir().unwrap();
        let e = ProbeEnv {
            os: TargetOs::Linux,
            ..env(&[tmp.path()], os())
        };
        let r = install_impl(SetupToolId::Git, &e, Duration::from_secs(1), &mut |_| {}).unwrap();
        assert_eq!(r.outcome, SetupInstallOutcome::ManagerMissing);
        assert_eq!(r.command, None);

        let (slot, _flag) = InstallSlot::take().unwrap();
        assert!(InstallSlot::take().is_err());
        assert!(cancel_install());
        drop(slot);
        assert!(!cancel_install());
        assert!(InstallSlot::take().is_ok());
    }

    #[test]
    fn progress_and_first_run_detection() {
        let tmp = tempfile::tempdir().unwrap();
        let st = state(tmp.path());
        let s = status_impl(&st).unwrap();
        assert!(s.should_auto_open);
        assert_eq!(s.progress, SetupProgress::default());

        let s = save_impl(
            &st,
            SetupProgress {
                completed: false,
                skipped: true,
                steps_done: vec![SetupStep::Welcome, SetupStep::Welcome, SetupStep::Tools],
            },
        )
        .unwrap();
        assert!(!s.should_auto_open);
        assert_eq!(
            s.progress.steps_done,
            [SetupStep::Welcome, SetupStep::Tools]
        );
        let raw = st.storage.settings().unwrap();
        assert_eq!(raw[KEY_SETUP]["skipped"], serde_json::json!(true));
        assert_eq!(
            raw[KEY_SETUP]["stepsDone"],
            serde_json::json!(["welcome", "tools"])
        );

        // Existing user: the import wizard was done before the setup existed.
        let tmp2 = tempfile::tempdir().unwrap();
        let st2 = state(tmp2.path());
        st2.storage.set_setting(KEY_IMPORT_DONE, &true).unwrap();
        assert!(!status_impl(&st2).unwrap().should_auto_open);

        // Existing user with an account.
        let tmp3 = tempfile::tempdir().unwrap();
        let st3 = state(tmp3.path());
        account_save_impl(
            &st3,
            AccountInput {
                id: None,
                provider_id: "github".into(),
                username: "alice".into(),
                display_name: None,
                avatar_url: None,
                auth_method: AuthMethod::Pat,
                scopes: vec![],
                token_expires_at: None,
                color: None,
                label: None,
                default_clone_dir: None,
            },
        )
        .unwrap();
        let s = status_impl(&st3).unwrap();
        assert!(!s.should_auto_open);
        assert_eq!(s.account_count, 1);
    }
}
