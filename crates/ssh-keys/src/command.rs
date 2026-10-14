//! `GIT_SSH_COMMAND` for an account: its key only (`IdentitiesOnly`, no agent) and optionally
//! the app's own `known_hosts` file.
//!
//! Git runs `GIT_SSH_COMMAND` through `sh -c` (Git for Windows ships `sh`), so arguments are
//! single-quoted for POSIX sh. Paths are passed as `-o Option="<path>"` (the double quotes
//! protect spaces from ssh's own option tokenizer), use forward slashes (accepted by MSYS ssh
//! and Win32-OpenSSH) and have `%` doubled because ssh percent-expands `IdentityFile` and
//! `UserKnownHostsFile` values.

use std::path::{Path, PathBuf};

/// What ssh does with a host key it has never seen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HostKeyPolicy {
    /// ssh's default (`ask`) — fails in non-interactive git with "Host key verification
    /// failed" unless the key is already known.
    #[default]
    SshDefault,
    /// Trust on first use, refuse changed keys (`StrictHostKeyChecking=accept-new`).
    AcceptNew,
    /// Only connect to hosts already in the known_hosts file(s).
    Strict,
}

/// Options for [`git_ssh_command`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshCommandOptions {
    /// `ssh` executable; `None` → `ssh` from PATH (Git for Windows' bundled one under git).
    pub program: Option<PathBuf>,
    /// Private key (`-o IdentityFile="<key>" -o IdentitiesOnly=yes`).
    pub key_path: Option<PathBuf>,
    /// Ignore keys offered by a running ssh-agent / Pageant (`IdentityAgent=none`), so another
    /// account's key can never be picked.
    pub disable_agent: bool,
    /// App-managed known_hosts file (`UserKnownHostsFile`, written unhashed).
    pub known_hosts_file: Option<PathBuf>,
    pub host_key_policy: HostKeyPolicy,
    /// `ConnectTimeout` in seconds.
    pub connect_timeout_secs: Option<u32>,
}

impl Default for SshCommandOptions {
    fn default() -> Self {
        Self {
            program: None,
            key_path: None,
            disable_agent: true,
            known_hosts_file: None,
            host_key_policy: HostKeyPolicy::SshDefault,
            connect_timeout_secs: None,
        }
    }
}

impl SshCommandOptions {
    pub fn with_key(key_path: impl Into<PathBuf>) -> Self {
        Self {
            key_path: Some(key_path.into()),
            ..Self::default()
        }
    }
}

/// Raw argument vector (no shell quoting) — for running ssh directly with `Command`.
pub fn ssh_args(opts: &SshCommandOptions) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(key) = &opts.key_path {
        // `-o IdentityFile=` rather than `-i`: ssh stats a `-i` path *before* percent
        // expansion and silently drops it when missing, so a path containing `%` could never
        // be expressed with `-i`; config values are only percent-expanded (`%%` → `%`).
        args.push("-o".into());
        args.push(format!(
            "IdentityFile=\"{}\"",
            escape_percent(&portable_path(key))
        ));
        args.push("-o".into());
        args.push("IdentitiesOnly=yes".into());
    }
    if opts.disable_agent {
        args.push("-o".into());
        args.push("IdentityAgent=none".into());
    }
    args.extend(known_hosts_args(
        opts.known_hosts_file.as_deref(),
        opts.host_key_policy,
    ));
    if let Some(secs) = opts.connect_timeout_secs {
        args.push("-o".into());
        args.push(format!("ConnectTimeout={secs}"));
    }
    args
}

/// `-o` arguments selecting the known_hosts file and host key policy (raw, unquoted).
pub fn known_hosts_args(file: Option<&Path>, policy: HostKeyPolicy) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(file) = file {
        args.push("-o".into());
        // Double quotes protect spaces from ssh's own option tokenizer.
        args.push(format!(
            "UserKnownHostsFile=\"{}\"",
            escape_percent(&portable_path(file))
        ));
        // Keep the app's file readable so it can be listed and verified.
        args.push("-o".into());
        args.push("HashKnownHosts=no".into());
    }
    match policy {
        HostKeyPolicy::SshDefault => {}
        HostKeyPolicy::AcceptNew => {
            args.push("-o".into());
            args.push("StrictHostKeyChecking=accept-new".into());
        }
        HostKeyPolicy::Strict => {
            args.push("-o".into());
            args.push("StrictHostKeyChecking=yes".into());
        }
    }
    args
}

/// Complete `GIT_SSH_COMMAND` value (also valid for `core.sshCommand`).
pub fn git_ssh_command(opts: &SshCommandOptions) -> String {
    let program = opts
        .program
        .as_deref()
        .map(|p| sh_quote(&portable_path(p)))
        .unwrap_or_else(|| "ssh".into());
    let mut out = program;
    for arg in ssh_args(opts) {
        out.push(' ');
        if arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_=.,:/@+".contains(c))
        {
            out.push_str(&arg);
        } else {
            out.push_str(&sh_quote(&arg));
        }
    }
    out
}

/// Forward slashes work for Git for Windows' `sh`, MSYS ssh and Win32-OpenSSH.
pub fn portable_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn escape_percent(s: &str) -> String {
    s.replace('%', "%%")
}

/// Single-quotes for POSIX `sh`.
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_command_with_key_and_known_hosts() {
        let opts = SshCommandOptions {
            key_path: Some(PathBuf::from(r"C:\Users\An Nguyen\AppData\ssh\acc1")),
            known_hosts_file: Some(PathBuf::from(r"C:\Users\An Nguyen\AppData\known_hosts")),
            host_key_policy: HostKeyPolicy::AcceptNew,
            ..SshCommandOptions::default()
        };
        assert_eq!(
            git_ssh_command(&opts),
            "ssh -o 'IdentityFile=\"C:/Users/An Nguyen/AppData/ssh/acc1\"' -o IdentitiesOnly=yes \
             -o IdentityAgent=none -o 'UserKnownHostsFile=\"C:/Users/An Nguyen/AppData/known_hosts\"' \
             -o HashKnownHosts=no -o StrictHostKeyChecking=accept-new"
        );
    }

    #[test]
    fn quotes_program_percent_and_apostrophe() {
        let opts = SshCommandOptions {
            program: Some(PathBuf::from("/opt/my ssh/ssh")),
            key_path: Some(PathBuf::from("/home/o'brien/100%/key")),
            disable_agent: false,
            connect_timeout_secs: Some(10),
            ..SshCommandOptions::default()
        };
        assert_eq!(
            git_ssh_command(&opts),
            r#"'/opt/my ssh/ssh' -o 'IdentityFile="/home/o'\''brien/100%%/key"' -o IdentitiesOnly=yes -o ConnectTimeout=10"#
        );
    }

    #[test]
    fn strict_policy_without_file() {
        assert_eq!(
            known_hosts_args(None, HostKeyPolicy::Strict),
            ["-o", "StrictHostKeyChecking=yes"]
        );
        assert!(known_hosts_args(None, HostKeyPolicy::SshDefault).is_empty());
    }
}
