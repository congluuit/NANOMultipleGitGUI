//! Restricting private key files to the current user.
//!
//! OpenSSH refuses private keys other users can read ("UNPROTECTED PRIVATE KEY FILE!"):
//! - Unix: the file must not be accessible by group/others → mode `0600` (directory `0700`).
//! - Windows (Win32-OpenSSH, shipped with Windows 10+ and used by many Git setups): the ACL
//!   may only grant access to the owner, `SYSTEM` and `Administrators`. Files created under
//!   `%APPDATA%` inherit ACEs from the profile folder (and sometimes extra groups), so we
//!   remove inheritance and grant the current user Full Control only.
//!
//! The Windows part shells out to `icacls.exe` (resolved under `%SystemRoot%\System32` to avoid
//! PATH hijacking) instead of calling the Win32 security API, because the workspace forbids
//! `unsafe` code. The user is identified by SID (`whoami /user`), which avoids problems with
//! non-ASCII user names and console code pages; the result is verified by reading the ACL back.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{Result, SshKeyError};

/// Outcome of a permission check on a key file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PermissionReport {
    /// Only the current user (plus `SYSTEM`/`Administrators` on Windows) can access the file.
    pub restricted: bool,
    /// Human-readable entries: the ACEs printed by `icacls` on Windows, `mode 0600` on Unix.
    pub entries: Vec<String>,
    /// Why the file is not considered restricted.
    pub problem: Option<String>,
}

/// Restricts a file (or, with `is_dir`, a directory and what is created inside it later) to
/// the current user.
pub fn restrict_to_current_user(path: &Path, is_dir: bool) -> Result<()> {
    imp::restrict(path, is_dir).map_err(|message| SshKeyError::Permissions {
        path: path.to_path_buf(),
        message,
    })
}

/// Checks that a key file is private to the current user.
pub fn check_permissions(path: &Path) -> Result<PermissionReport> {
    if !path.exists() {
        return Err(SshKeyError::NotFound(path.to_path_buf()));
    }
    imp::check(path).map_err(|message| SshKeyError::Permissions {
        path: path.to_path_buf(),
        message,
    })
}

/// Restricts and then verifies; fails if the verification does not pass.
///
/// On Windows the first pass keeps explicit ACEs the file may already carry (e.g. someone ran
/// `icacls key /grant Users:R`); if the check finds such entries the ACL is reset to the
/// inherited defaults and restricted again. Within the key store the parent directory is
/// itself user-only, so the reset never exposes the key to anyone else.
pub(crate) fn restrict_and_verify(path: &Path) -> Result<()> {
    restrict_to_current_user(path, false)?;
    let mut report = check_permissions(path)?;
    if !report.restricted {
        imp::reset(path).map_err(|message| SshKeyError::Permissions {
            path: path.to_path_buf(),
            message,
        })?;
        restrict_to_current_user(path, false)?;
        report = check_permissions(path)?;
    }
    if report.restricted {
        Ok(())
    } else {
        Err(SshKeyError::Permissions {
            path: path.to_path_buf(),
            message: report
                .problem
                .unwrap_or_else(|| "permissions are still too open".into()),
        })
    }
}

#[allow(dead_code)] // Only used by the Windows implementation and its tests.
fn system_tool(name: &str) -> PathBuf {
    if let Some(root) = std::env::var_os("SystemRoot") {
        let p = Path::new(&root).join("System32").join(name);
        if p.is_file() {
            return p;
        }
    }
    PathBuf::from(name)
}

#[cfg(unix)]
mod imp {
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use super::PermissionReport;

    pub fn reset(_path: &Path) -> Result<(), String> {
        Ok(())
    }

    pub fn restrict(path: &Path, is_dir: bool) -> Result<(), String> {
        let mode = if is_dir { 0o700 } else { 0o600 };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .map_err(|e| e.to_string())
    }

    pub fn check(path: &Path) -> Result<PermissionReport, String> {
        let mode = std::fs::metadata(path)
            .map_err(|e| e.to_string())?
            .permissions()
            .mode()
            & 0o777;
        let restricted = mode & 0o077 == 0;
        Ok(PermissionReport {
            restricted,
            entries: vec![format!("mode {mode:04o}")],
            problem: (!restricted).then(|| format!("mode {mode:04o} gives access to group/others")),
        })
    }
}

#[cfg(windows)]
mod imp {
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::Command;

    use super::{PermissionReport, system_tool};

    /// Do not flash a console window when called from the GUI app.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    #[derive(Clone)]
    pub struct CurrentUser {
        /// `domain\user` as printed by `whoami` (lower-case on most systems).
        pub name: String,
        pub sid: String,
    }

    static CURRENT_USER: std::sync::OnceLock<CurrentUser> = std::sync::OnceLock::new();

    fn run(tool: &str, args: &[&std::ffi::OsStr]) -> Result<String, String> {
        let out = Command::new(system_tool(tool))
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("cannot run {tool}: {e}"))?;
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(format!(
                "{tool} failed ({}): {} {}",
                out.status,
                stdout.trim(),
                stderr.trim()
            ));
        }
        Ok(stdout)
    }

    pub fn current_user() -> Result<CurrentUser, String> {
        if let Some(user) = CURRENT_USER.get() {
            return Ok(user.clone());
        }
        let user = query_current_user()?;
        Ok(CURRENT_USER.get_or_init(|| user).clone())
    }

    fn query_current_user() -> Result<CurrentUser, String> {
        // `"desktop-x\user","S-1-5-21-…"`
        let out = run(
            "whoami.exe",
            &[
                "/user".as_ref(),
                "/fo".as_ref(),
                "csv".as_ref(),
                "/nh".as_ref(),
            ],
        )?;
        let line = out.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        let fields: Vec<&str> = line.split("\",\"").map(|f| f.trim_matches('"')).collect();
        match fields.as_slice() {
            [name, sid] if sid.starts_with("S-1-") => Ok(CurrentUser {
                name: name.trim().to_string(),
                sid: sid.trim().to_string(),
            }),
            _ => Err(format!("unexpected `whoami /user` output: {line}")),
        }
    }

    /// Drops every explicit ACE (back to inherited defaults).
    pub fn reset(path: &Path) -> Result<(), String> {
        run(
            "icacls.exe",
            &[path.as_os_str(), "/reset".as_ref(), "/q".as_ref()],
        )?;
        Ok(())
    }

    pub fn restrict(path: &Path, is_dir: bool) -> Result<(), String> {
        let user = current_user()?;
        let grant = if is_dir {
            format!("*{}:(OI)(CI)F", user.sid)
        } else {
            format!("*{}:F", user.sid)
        };
        run(
            "icacls.exe",
            &[
                path.as_os_str(),
                "/inheritance:r".as_ref(),
                "/grant:r".as_ref(),
                grant.as_ref(),
                "/q".as_ref(),
            ],
        )?;
        Ok(())
    }

    pub fn check(path: &Path) -> Result<PermissionReport, String> {
        let user = current_user()?;
        let out = run("icacls.exe", &[path.as_os_str()])?;
        let entries = parse_icacls(&out, &path.to_string_lossy());
        Ok(evaluate(&entries, &user.name))
    }

    /// ACE lines of `icacls <path>` output: the first line starts with the path, the others
    /// are indented; a blank line precedes the localised summary.
    pub fn parse_icacls(out: &str, path: &str) -> Vec<String> {
        let mut entries = Vec::new();
        for (i, line) in out.lines().enumerate() {
            let line = line.trim_end();
            if line.trim().is_empty() {
                if i > 0 {
                    break;
                }
                continue;
            }
            let ace = if i == 0 {
                line.strip_prefix(path).unwrap_or(line)
            } else {
                line
            };
            let ace = ace.trim();
            if !ace.is_empty() {
                entries.push(ace.to_string());
            }
        }
        entries
    }

    pub fn evaluate(entries: &[String], user: &str) -> PermissionReport {
        let mut problem = None;
        let mut user_has_access = false;
        for ace in entries {
            let Some(idx) = ace.rfind(":(") else {
                problem.get_or_insert_with(|| format!("unrecognised ACE `{ace}`"));
                continue;
            };
            let (who, rights) = (&ace[..idx], &ace[idx + 1..]);
            if rights.contains("(I)") {
                problem.get_or_insert_with(|| format!("inherited access for `{who}`"));
            }
            if who.eq_ignore_ascii_case(user) {
                user_has_access = true;
                continue;
            }
            // Accepted by Win32-OpenSSH (English names; localised systems simply get a
            // warning in the report, which is still correct to show).
            let allowed = who.eq_ignore_ascii_case("NT AUTHORITY\\SYSTEM")
                || who.eq_ignore_ascii_case("BUILTIN\\Administrators");
            if !allowed {
                problem.get_or_insert_with(|| format!("`{who}` also has access"));
            }
        }
        if !user_has_access {
            problem.get_or_insert_with(|| format!("`{user}` has no access entry"));
        }
        PermissionReport {
            restricted: problem.is_none(),
            entries: entries.to_vec(),
            problem,
        }
    }
}

#[cfg(not(any(unix, windows)))]
mod imp {
    use std::path::Path;

    use super::PermissionReport;

    pub fn reset(_path: &Path) -> Result<(), String> {
        Ok(())
    }

    pub fn restrict(_path: &Path, _is_dir: bool) -> Result<(), String> {
        Err("restricting permissions is not supported on this platform".into())
    }

    pub fn check(_path: &Path) -> Result<PermissionReport, String> {
        Err("checking permissions is not supported on this platform".into())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::imp::{evaluate, parse_icacls};

    #[test]
    fn parses_icacls_output() {
        let out = "C:\\x\\key DESKTOP-1\\Bob:(F)\r\n       NT AUTHORITY\\SYSTEM:(I)(F)\r\n\r\nSuccessfully processed 1 files; Failed processing 0 files\r\n";
        let entries = parse_icacls(out, "C:\\x\\key");
        assert_eq!(
            entries,
            ["DESKTOP-1\\Bob:(F)", "NT AUTHORITY\\SYSTEM:(I)(F)"]
        );
        let report = evaluate(&entries, "desktop-1\\bob");
        assert!(!report.restricted);
        assert!(report.problem.unwrap().contains("inherited"));

        let ok = evaluate(&["DESKTOP-1\\Bob:(F)".to_string()], "desktop-1\\bob");
        assert!(ok.restricted, "{ok:?}");

        let other = evaluate(
            &[
                "DESKTOP-1\\Bob:(F)".to_string(),
                "BUILTIN\\Users:(RX)".to_string(),
            ],
            "desktop-1\\bob",
        );
        assert!(!other.restricted);

        let none = evaluate(
            &["BUILTIN\\Administrators:(F)".to_string()],
            "desktop-1\\bob",
        );
        assert!(!none.restricted);
    }
}
