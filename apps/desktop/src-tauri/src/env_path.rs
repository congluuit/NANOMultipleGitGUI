//! Makes the process `PATH` match the user's terminal on macOS and Linux.
//!
//! A GUI app started from Finder, the Dock, Spotlight or a desktop launcher does not go through
//! the login shell, so it gets a minimal `PATH` (`/usr/bin:/bin:/usr/sbin:/sbin` on macOS) that
//! never contains Homebrew (`/usr/local/bin` on Intel, `/opt/homebrew/bin` on Apple silicon),
//! MacPorts or `~/.local/bin`. Everything the app runs by name (`git`, `git-lfs`, `gh`, `glab`,
//! `ssh`, `brew`) would then be invisible, and restarting the app never helps.
//!
//! [`fix`] runs once, before any thread exists: it asks the login shell for its `PATH`, appends
//! whatever the process is missing, then appends the well-known tool folders that exist on disk.
//! On Windows installers update the registry `PATH` and a restart picks it up, so nothing is done.

use std::path::{Path, PathBuf};

/// What [`fix`] changed, for the log (tracing is not up yet when it runs).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct PathFix {
    /// Folders appended to the process `PATH`.
    pub added: Vec<PathBuf>,
    /// Whether the login shell answered (otherwise only the well-known folders were tried).
    pub from_login_shell: bool,
}

/// Folders tools get installed into that a GUI app's `PATH` lacks (searched in this order).
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) fn well_known_dirs(home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = [
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        "/usr/local/sbin",
        "/opt/local/bin",
        "/home/linuxbrew/.linuxbrew/bin",
        "/snap/bin",
        "/var/lib/flatpak/exports/bin",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();
    if let Some(home) = home {
        dirs.push(home.join(".local").join("bin"));
    }
    dirs
}

/// `current` first, then the login shell's folders it lacks, then the well-known folders that
/// exist (`exists`). Returns the merged list and what was appended. Pure, so it is testable on
/// every OS.
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) fn merge(
    current: &[PathBuf],
    login_shell: &[PathBuf],
    well_known: &[PathBuf],
    exists: impl Fn(&Path) -> bool,
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut merged: Vec<PathBuf> = current
        .iter()
        .filter(|d| !d.as_os_str().is_empty())
        .cloned()
        .collect();
    let mut added = Vec::new();
    let mut push = |merged: &mut Vec<PathBuf>, d: &Path| {
        if !d.as_os_str().is_empty() && !merged.iter().any(|m| m == d) {
            merged.push(d.to_path_buf());
            added.push(d.to_path_buf());
        }
    };
    for d in login_shell {
        push(&mut merged, d);
    }
    for d in well_known.iter().filter(|d| exists(d)) {
        push(&mut merged, d);
    }
    (merged, added)
}

/// `PATH` as the user's login shell sees it (`$SHELL -l -c 'printf %s "$PATH"'`), or `None`
/// when the shell is missing, fails or takes longer than a few seconds (a profile waiting for
/// input).
#[cfg(unix)]
fn login_shell_path() -> Option<Vec<PathBuf>> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let shell = std::env::var_os("SHELL")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .filter(|s| s.is_file())
        .unwrap_or_else(|| PathBuf::from("/bin/sh"));
    let mut child = Command::new(&shell)
        .args(["-l", "-c", "printf '%s' \"$PATH\""])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut stdout = child.stdout.take()?;
    // Read on a thread so a profile that prints a lot cannot fill the pipe and block `wait`.
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut stdout, &mut buf).ok()?;
        Some(buf)
    });
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let out = reader.join().ok().flatten()?;
    if !status?.success() {
        return None;
    }
    // Profiles may echo banners before the value: keep the last line, which is `printf`'s.
    let text = String::from_utf8_lossy(&out);
    let last = text.lines().last()?.trim();
    if last.is_empty() {
        return None;
    }
    Some(std::env::split_paths(last).collect())
}

/// Extends the process `PATH`. Call once at startup, before spawning threads.
#[cfg(unix)]
pub(crate) fn fix() -> PathFix {
    let current: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    let login = login_shell_path();
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from);
    let (merged, added) = merge(
        &current,
        login.as_deref().unwrap_or(&[]),
        &well_known_dirs(home.as_deref()),
        |d| d.is_dir(),
    );
    if !added.is_empty()
        && let Ok(joined) = std::env::join_paths(&merged)
    {
        // SAFETY: called from `run()` before Tauri or anything else starts a thread (the shell
        // reader thread above has been joined), so nothing else reads the environment.
        unsafe { std::env::set_var("PATH", joined) };
    }
    PathFix {
        added,
        from_login_shell: login.is_some(),
    }
}

/// Windows: installers write the registry `PATH`; a restart picks it up (see `onboarding`).
#[cfg(not(unix))]
pub(crate) fn fix() -> PathFix {
    PathFix::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn appends_login_shell_and_existing_well_known_dirs_without_duplicates() {
        let current = [p("/usr/bin"), p("/bin"), p("")];
        let login = [
            p("/opt/homebrew/bin"),
            p("/usr/bin"),
            p("/Users/me/.cargo/bin"),
        ];
        let known = [
            p("/opt/homebrew/bin"),
            p("/usr/local/bin"),
            p("/opt/local/bin"),
        ];
        let (merged, added) = merge(&current, &login, &known, |d| {
            d != Path::new("/opt/local/bin")
        });
        assert_eq!(
            merged,
            [
                p("/usr/bin"),
                p("/bin"),
                p("/opt/homebrew/bin"),
                p("/Users/me/.cargo/bin"),
                p("/usr/local/bin"),
            ]
        );
        assert_eq!(
            added,
            [
                p("/opt/homebrew/bin"),
                p("/Users/me/.cargo/bin"),
                p("/usr/local/bin")
            ]
        );
    }

    #[test]
    fn nothing_to_add_when_the_shell_is_silent_and_no_folder_exists() {
        let current = [p("/usr/bin")];
        let (merged, added) = merge(&current, &[], &well_known_dirs(None), |_| false);
        assert_eq!(merged, current);
        assert!(added.is_empty());
    }

    #[test]
    fn well_known_dirs_cover_homebrew_on_both_mac_architectures_and_the_user_bin() {
        let dirs = well_known_dirs(Some(Path::new("/Users/me")));
        assert!(dirs.contains(&p("/opt/homebrew/bin")));
        assert!(dirs.contains(&p("/usr/local/bin")));
        assert_eq!(dirs.last(), Some(&p("/Users/me/.local/bin")));
    }
}
