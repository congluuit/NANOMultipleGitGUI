//! Copies a token out of the `gh` / `glab` CLI — only when the user asked for it (PLAN 3B.1).
//!
//! The CLI is run directly (no shell), with stdin closed, a timeout, no console window on
//! Windows, and the token environment variables removed (`GH_TOKEN` & co. would make the CLI
//! print *that* token instead of the stored one). Nothing is written to the CLI's config.
//! Tokens come back as [`Zeroizing`] strings and never appear in errors.
//!
//! - gh: `gh auth token --hostname <host> --user <user>` (gh ≥ 2.40). Older gh has no
//!   `--user`; the call is retried without it only when the host has a single account in
//!   `hosts.yml`, so the token of another account can never be picked up. Callers still
//!   validate the token against the provider and compare the user name.
//! - glab: `glab config get token --host <host>` (one account per host).

use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use zeroize::{Zeroize, Zeroizing};

use crate::cli_hosts::CliTool;

/// Default time limit of one CLI call (the CLI may read the OS keyring).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

/// Variables that override the stored token in gh / glab.
const TOKEN_ENV: [&str; 8] = [
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GH_ENTERPRISE_TOKEN",
    "GITHUB_ENTERPRISE_TOKEN",
    "GITLAB_TOKEN",
    "GITLAB_ACCESS_TOKEN",
    "GLAB_TOKEN",
    "OAUTH_TOKEN",
];

impl CliTool {
    /// Executable name without extension.
    pub fn program_name(self) -> &'static str {
        match self {
            CliTool::Gh => "gh",
            CliTool::Glab => "glab",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CliTokenError {
    /// The CLI answered but has no token for this host / user (signed out, token removed, or
    /// kept somewhere it cannot read). `detail` is the CLI's message (never contains a token).
    #[error("the CLI has no token for this account: {detail}")]
    NoToken { detail: String },
    /// gh older than 2.40 (no `--user`) and the host has several accounts.
    #[error("this gh version cannot pick an account by name; update gh (2.40 or newer)")]
    UserFlagUnsupported,
    #[error("the CLI did not answer within {0:?}")]
    Timeout(Duration),
    /// The executable could not be started.
    #[error("could not run the CLI: {detail}")]
    Spawn { detail: String },
}

/// Which account's token to read.
#[derive(Debug, Clone, Copy)]
pub struct TokenRequest<'a> {
    pub host: &'a str,
    /// `None` when the config does not record the user (the CLI's active account is used).
    pub username: Option<&'a str>,
    /// `hosts.yml` lists exactly one account for this host (allows the old-gh fallback).
    pub single_account_on_host: bool,
}

/// File names to look for in a directory.
fn candidate_names(tool: CliTool) -> Vec<String> {
    let name = tool.program_name();
    if cfg!(windows) {
        vec![
            format!("{name}.exe"),
            format!("{name}.cmd"),
            format!("{name}.bat"),
        ]
    } else {
        vec![name.to_owned()]
    }
}

/// Install folders that are often missing from `PATH` (GUI apps on macOS do not get the shell
/// `PATH`; winget/MSI installs may need a new session).
pub fn default_cli_dirs(tool: CliTool, home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if cfg!(windows) {
        let var = |n: &str| {
            std::env::var_os(n)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let sub = match tool {
            CliTool::Gh => "GitHub CLI",
            CliTool::Glab => "glab",
        };
        for base in [var("ProgramFiles"), var("ProgramFiles(x86)")]
            .into_iter()
            .flatten()
        {
            dirs.push(base.join(sub));
        }
        if let Some(local) = var("LOCALAPPDATA") {
            dirs.push(local.join("Programs").join(sub));
            dirs.push(local.join("Microsoft").join("WinGet").join("Links"));
        }
        if let Some(h) = home {
            dirs.push(h.join("scoop").join("shims"));
        }
    } else {
        for d in [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/usr/bin",
            "/home/linuxbrew/.linuxbrew/bin",
            "/snap/bin",
        ] {
            dirs.push(PathBuf::from(d));
        }
        if let Some(h) = home {
            dirs.push(h.join(".local").join("bin"));
        }
    }
    dirs
}

/// Finds the CLI in `path_var` (a `PATH` value), then in `extra_dirs`.
pub fn find_cli(
    tool: CliTool,
    path_var: Option<&OsStr>,
    extra_dirs: &[PathBuf],
) -> Option<PathBuf> {
    let names = candidate_names(tool);
    path_var
        .map(|p| std::env::split_paths(p).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .filter(|d| !d.as_os_str().is_empty())
        .chain(extra_dirs.iter().cloned())
        .find_map(|dir| names.iter().map(|n| dir.join(n)).find(|p| p.is_file()))
}

/// [`find_cli`] with the process `PATH` and [`default_cli_dirs`].
pub fn locate_cli(tool: CliTool, home: Option<&Path>) -> Option<PathBuf> {
    find_cli(
        tool,
        std::env::var_os("PATH").as_deref(),
        &default_cli_dirs(tool, home),
    )
}

/// Arguments of the token call.
fn token_args(tool: CliTool, host: &str, user: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = match tool {
        CliTool::Gh => vec![
            "auth".into(),
            "token".into(),
            "--hostname".into(),
            host.into(),
        ],
        CliTool::Glab => vec![
            "config".into(),
            "get".into(),
            "token".into(),
            "--host".into(),
            host.into(),
        ],
    };
    if let (CliTool::Gh, Some(u)) = (tool, user) {
        args.push("--user".into());
        args.push(u.into());
    }
    args
}

/// The command for one CLI call (exposed for tests that inspect the environment).
pub fn token_command(program: &Path, args: &[String]) -> Command {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("GLAB_NO_PROMPT", "1")
        .env("NO_PROMPT", "1")
        .env("NO_COLOR", "1");
    for var in TOKEN_ENV {
        cmd.env_remove(var);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

struct CliOutput {
    success: bool,
    stdout: Zeroizing<Vec<u8>>,
    stderr: String,
}

fn run(program: &Path, args: &[String], timeout: Duration) -> Result<CliOutput, CliTokenError> {
    let mut child = token_command(program, args)
        .spawn()
        .map_err(|e| CliTokenError::Spawn {
            detail: e.to_string(),
        })?;
    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let out_reader = std::thread::spawn(move || {
        let mut buf = Zeroizing::new(Vec::new());
        if let Some(p) = out_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = p.take(64 * 1024).read_to_end(&mut buf);
        }
        buf
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                // The readers are left to finish on their own: a grandchild (shell scripts)
                // may still hold the pipes open.
                return Err(CliTokenError::Timeout(timeout));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                return Err(CliTokenError::Spawn {
                    detail: e.to_string(),
                });
            }
        }
    };
    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    let mut stderr = String::from_utf8_lossy(&stderr).trim().to_owned();
    if stderr.chars().count() > 300 {
        stderr = stderr.chars().take(300).collect::<String>() + "…";
    }
    Ok(CliOutput {
        success: status.success(),
        stdout,
        stderr,
    })
}

/// The token printed on stdout: one line, no inner whitespace.
fn parse_token(stdout: &[u8]) -> Option<Zeroizing<String>> {
    let text = std::str::from_utf8(stdout).ok()?;
    let token = text.trim();
    if token.is_empty() || token.chars().any(char::is_whitespace) {
        return None;
    }
    Some(Zeroizing::new(token.to_owned()))
}

fn no_token(out: &CliOutput) -> CliTokenError {
    let detail = if out.stderr.is_empty() {
        if out.success && !out.stdout.is_empty() {
            "unexpected output".to_owned()
        } else {
            "no token".to_owned()
        }
    } else {
        out.stderr.clone()
    };
    CliTokenError::NoToken { detail }
}

fn is_unknown_user_flag(stderr: &str) -> bool {
    let s = stderr.to_ascii_lowercase();
    s.contains("unknown flag") && s.contains("--user")
}

/// Asks the CLI at `program` for the token of `req`.
pub fn fetch_token(
    tool: CliTool,
    program: &Path,
    req: TokenRequest<'_>,
    timeout: Duration,
) -> Result<Zeroizing<String>, CliTokenError> {
    let attempt = |user: Option<&str>| -> Result<CliOutput, CliTokenError> {
        run(program, &token_args(tool, req.host, user), timeout)
    };
    let mut out = attempt(req.username)?;
    if tool == CliTool::Gh
        && req.username.is_some()
        && !out.success
        && is_unknown_user_flag(&out.stderr)
    {
        if !req.single_account_on_host {
            return Err(CliTokenError::UserFlagUnsupported);
        }
        out = attempt(None)?;
    }
    let token = if out.success {
        parse_token(&out.stdout)
    } else {
        None
    };
    let result = token.ok_or_else(|| no_token(&out));
    out.stdout.zeroize();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes a fake CLI into `dir` and returns its path. `body` is a Windows batch script or a
    /// POSIX shell script depending on the platform.
    fn fake_cli(dir: &Path, name: &str, windows: &str, unix: &str) -> PathBuf {
        if cfg!(windows) {
            let p = dir.join(format!("{name}.cmd"));
            std::fs::write(
                &p,
                format!("@echo off\r\n{}\r\n", windows.replace('\n', "\r\n")),
            )
            .unwrap();
            p
        } else {
            let p = dir.join(name);
            std::fs::write(&p, format!("#!/bin/sh\n{unix}\n")).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            p
        }
    }

    /// gh ≥ 2.40: alice and bob on github.com, active = bob.
    fn modern_gh(dir: &Path) -> PathBuf {
        fake_cli(
            dir,
            "gh",
            "if not \"%1 %2 %3\"==\"auth token --hostname\" exit /b 2\n\
             if not \"%4\"==\"github.com\" (echo no oauth token found for %4 1>&2& exit /b 1)\n\
             if not \"%5\"==\"--user\" (echo gho_bob_active& exit /b 0)\n\
             if \"%6\"==\"alice\" (echo gho_alice_token& exit /b 0)\n\
             if \"%6\"==\"bob\" (echo gho_bob_active& exit /b 0)\n\
             echo no oauth token found for %4 account %6 1>&2\n\
             exit /b 1",
            "[ \"$1 $2 $3\" = \"auth token --hostname\" ] || exit 2\n\
             [ \"$4\" = \"github.com\" ] || { echo \"no oauth token found for $4\" >&2; exit 1; }\n\
             [ \"$5\" = \"--user\" ] || { echo gho_bob_active; exit 0; }\n\
             [ \"$6\" = alice ] && { echo gho_alice_token; exit 0; }\n\
             [ \"$6\" = bob ] && { echo gho_bob_active; exit 0; }\n\
             echo \"no oauth token found for $4 account $6\" >&2; exit 1",
        )
    }

    /// gh < 2.40: no `--user` flag.
    fn old_gh(dir: &Path) -> PathBuf {
        fake_cli(
            dir,
            "gh",
            "if \"%5\"==\"--user\" (echo unknown flag: --user 1>&2& exit /b 1)\n\
             echo gho_only_user",
            "[ \"$5\" = \"--user\" ] && { echo 'unknown flag: --user' >&2; exit 1; }\n\
             echo gho_only_user",
        )
    }

    fn req<'a>(host: &'a str, user: Option<&'a str>, single: bool) -> TokenRequest<'a> {
        TokenRequest {
            host,
            username: user,
            single_account_on_host: single,
        }
    }

    const T: Duration = Duration::from_secs(20);

    #[test]
    fn gh_returns_the_token_of_the_requested_user() {
        let tmp = tempfile::tempdir().unwrap();
        let gh = modern_gh(tmp.path());
        let t = fetch_token(CliTool::Gh, &gh, req("github.com", Some("alice"), false), T).unwrap();
        assert_eq!(t.as_str(), "gho_alice_token");
        let t = fetch_token(CliTool::Gh, &gh, req("github.com", Some("bob"), false), T).unwrap();
        assert_eq!(t.as_str(), "gho_bob_active");
        let err =
            fetch_token(CliTool::Gh, &gh, req("github.com", Some("carol"), false), T).unwrap_err();
        assert!(
            matches!(&err, CliTokenError::NoToken { detail } if detail.contains("carol")),
            "{err:?}"
        );
        let err =
            fetch_token(CliTool::Gh, &gh, req("ghe.acme.vn", Some("alice"), true), T).unwrap_err();
        assert!(matches!(err, CliTokenError::NoToken { .. }));
    }

    #[test]
    fn old_gh_falls_back_only_for_a_single_account_host() {
        let tmp = tempfile::tempdir().unwrap();
        let gh = old_gh(tmp.path());
        let t = fetch_token(CliTool::Gh, &gh, req("github.com", Some("an"), true), T).unwrap();
        assert_eq!(t.as_str(), "gho_only_user");
        // Several accounts: never guess which one the CLI would answer for.
        let err =
            fetch_token(CliTool::Gh, &gh, req("github.com", Some("an"), false), T).unwrap_err();
        assert_eq!(err, CliTokenError::UserFlagUnsupported);
    }

    #[test]
    fn glab_reads_the_host_token_and_reports_empty_output() {
        let tmp = tempfile::tempdir().unwrap();
        let glab = fake_cli(
            tmp.path(),
            "glab",
            "if not \"%1 %2 %3 %4\"==\"config get token --host\" exit /b 2\n\
             if \"%5\"==\"gitlab.acme.vn\" (echo glpat-acme_token& exit /b 0)\n\
             exit /b 0",
            "[ \"$1 $2 $3 $4\" = \"config get token --host\" ] || exit 2\n\
             [ \"$5\" = gitlab.acme.vn ] && { echo glpat-acme_token; exit 0; }\n\
             exit 0",
        );
        let t = fetch_token(CliTool::Glab, &glab, req("gitlab.acme.vn", None, true), T).unwrap();
        assert_eq!(t.as_str(), "glpat-acme_token");
        // glab prints nothing (exit 0) for a host without a token.
        let err =
            fetch_token(CliTool::Glab, &glab, req("gitlab.com", Some("an"), true), T).unwrap_err();
        assert_eq!(
            err,
            CliTokenError::NoToken {
                detail: "no token".into()
            }
        );
    }

    #[test]
    fn output_that_is_not_a_single_token_is_refused_without_echoing_it() {
        let tmp = tempfile::tempdir().unwrap();
        let gh = fake_cli(
            tmp.path(),
            "gh",
            "echo gho_part1 secret_part2",
            "echo gho_part1 secret_part2",
        );
        let err = fetch_token(CliTool::Gh, &gh, req("github.com", None, true), T).unwrap_err();
        assert!(!err.to_string().contains("secret_part2"), "{err}");
        assert!(matches!(err, CliTokenError::NoToken { .. }));
    }

    #[test]
    fn slow_cli_times_out() {
        let tmp = tempfile::tempdir().unwrap();
        let gh = fake_cli(
            tmp.path(),
            "gh",
            "ping -n 3 127.0.0.1 >nul\necho gho_late",
            "sleep 2\necho gho_late",
        );
        let started = Instant::now();
        let err = fetch_token(
            CliTool::Gh,
            &gh,
            req("github.com", None, true),
            Duration::from_millis(300),
        )
        .unwrap_err();
        assert!(matches!(err, CliTokenError::Timeout(_)), "{err:?}");
        assert!(started.elapsed() < Duration::from_millis(1800));
    }

    #[test]
    fn missing_program_is_a_spawn_error() {
        let tmp = tempfile::tempdir().unwrap();
        let err = fetch_token(
            CliTool::Gh,
            &tmp.path().join("nope.exe"),
            req("github.com", None, true),
            T,
        )
        .unwrap_err();
        assert!(matches!(err, CliTokenError::Spawn { .. }));
    }

    #[test]
    fn token_variables_are_removed_from_the_cli_environment() {
        let cmd = token_command(
            Path::new("gh"),
            &token_args(CliTool::Gh, "github.com", Some("an")),
        );
        let envs: Vec<_> = cmd.get_envs().collect();
        for var in TOKEN_ENV {
            assert!(
                envs.iter()
                    .any(|(k, v)| *k == OsStr::new(var) && v.is_none()),
                "{var} must be removed"
            );
        }
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            ["auth", "token", "--hostname", "github.com", "--user", "an"]
        );
        let glab = token_args(CliTool::Glab, "gitlab.com", Some("ignored"));
        assert_eq!(glab, ["config", "get", "token", "--host", "gitlab.com"]);
    }

    #[test]
    fn finds_the_cli_in_path_then_in_extra_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b, c) = (
            tmp.path().join("a"),
            tmp.path().join("b"),
            tmp.path().join("c"),
        );
        for d in [&a, &b, &c] {
            std::fs::create_dir_all(d).unwrap();
        }
        let name = if cfg!(windows) { "gh.exe" } else { "gh" };
        std::fs::write(b.join(name), b"").unwrap();
        std::fs::write(c.join(name), b"").unwrap();
        let path = std::env::join_paths([&a, &b]).unwrap();
        assert_eq!(
            find_cli(CliTool::Gh, Some(&path), std::slice::from_ref(&c)),
            Some(b.join(name))
        );
        assert_eq!(
            find_cli(CliTool::Gh, None, std::slice::from_ref(&c)),
            Some(c.join(name))
        );
        assert_eq!(find_cli(CliTool::Glab, Some(&path), &[c]), None);
    }
}
