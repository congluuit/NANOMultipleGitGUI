//! Read-only import of existing configuration for the first-run wizard (PLAN.md 4.2).
//!
//! Scanners read `~/.gitconfig` (+ XDG config, `include.path`, `includeIf`), `~/.ssh/config`,
//! the GitHub CLI `hosts.yml` and the glab `config.yml`, and turn them into **suggestions**
//! (identities, path/remote rules, SSH host ↔ key mappings, provider accounts). Nothing is ever
//! written; tokens found in gh/glab files are never returned (only the fact that one exists).
//! [`cli_token`] is the one exception, run only after the user agreed: it asks the gh / glab
//! executable for a token (PLAN 3B.1).
//!
//! Every suggestion carries a [`SourceRef`] (file + 1-based line) so the UI can show where it
//! came from; unreadable or malformed files become [`ImportWarning`]s instead of errors.
//!
//! All entry points take explicit paths (home, AppData, [`ScanEnv`]) so they can be tested
//! with temporary directories; [`ScanEnv::from_process`] reads the real environment.

pub mod cli_hosts;
pub mod cli_token;
pub mod gitconfig;
pub mod paths;
pub mod ssh_config;

use std::path::{Path, PathBuf};

use serde::Serialize;

pub use cli_hosts::{gh_hosts_candidates, glab_config_candidates, scan_gh_hosts, scan_glab_config};
pub use gitconfig::{GitConfigScan, global_config_files, scan_gitconfig, scan_gitconfig_files};
pub use ssh_config::{SshConfigScan, scan_ssh_config};

/// Where a suggestion or warning comes from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    /// Path as found (OS separators).
    pub file: String,
    /// 1-based line number, when known.
    pub line: Option<u32>,
}

impl SourceRef {
    pub(crate) fn new(file: &Path, line: Option<u32>) -> Self {
        Self {
            file: file.to_string_lossy().into_owned(),
            line,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum WarningKind {
    /// The file exists but cannot be read (permissions, not UTF-8, …).
    Unreadable,
    /// Syntax error; the rest of the file was still used when possible.
    Parse,
    /// An `include.path` / `includeIf` / ssh `Include` target does not exist.
    MissingInclude,
    /// Include nesting too deep or cyclic.
    IncludeLoop,
    /// A construct the importer does not translate (`includeIf "onbranch:…"`, ssh `Match`).
    Unsupported,
}

/// Non-fatal problem found while scanning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ImportWarning {
    pub kind: WarningKind,
    pub source: SourceRef,
    /// English technical detail (the UI localises by `kind`).
    pub message: String,
}

/// Provider family, as in PLAN.md 4.1 `Provider.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    Github,
    GithubEnterprise,
    Gitlab,
    GitlabSelfHosted,
    /// Plain SSH host or unrecognised server.
    Unknown,
}

impl ProviderKind {
    /// Best guess from a host name.
    pub fn guess(host: &str) -> Self {
        let h = host.trim().trim_end_matches('.').to_ascii_lowercase();
        match h.as_str() {
            "github.com" | "ssh.github.com" | "www.github.com" | "api.github.com" => Self::Github,
            "gitlab.com" | "altssh.gitlab.com" | "www.gitlab.com" => Self::Gitlab,
            _ if h.contains("gitlab") => Self::GitlabSelfHosted,
            _ if h.contains("github") || h.ends_with(".ghe.com") => Self::GithubEnterprise,
            _ => Self::Unknown,
        }
    }
}

/// Machine-readable remarks the UI turns into localised hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum SuggestionNote {
    /// SSH host alias (`github.com-work`) only existed to pick a key; the app injects the key
    /// per account, so remotes can use the real host again.
    AliasUnnecessary,
    /// The pattern contains wildcards; it is shown as a default, not mapped to an account.
    WildcardHost,
    /// An `IdentityFile` does not exist.
    KeyFileMissing,
    /// The CLI keeps a token for this account (keyring or file); the app cannot reuse it, the
    /// user signs in again.
    SignInAgain,
    /// The CLI stores this token in plain text in its config file (worth telling the user).
    TokenInPlainText,
    /// Values came from a conditional include nested inside another one; the rule is an
    /// approximation (both conditions must hold in git).
    NestedCondition,
}

/// Everything [`scan`] found.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub identities: Vec<gitconfig::SuggestedIdentity>,
    pub rules: Vec<gitconfig::SuggestedRule>,
    pub ssh_hosts: Vec<ssh_config::SuggestedSshHost>,
    pub accounts: Vec<cli_hosts::SuggestedAccount>,
    pub warnings: Vec<ImportWarning>,
    /// Files that were read (in order), for the wizard's "Sources" list.
    pub scanned_files: Vec<String>,
}

/// Environment that changes where configs live. `Default` = nothing set, no git executable.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanEnv {
    /// `XDG_CONFIG_HOME`.
    pub xdg_config_home: Option<PathBuf>,
    /// `GIT_CONFIG_GLOBAL` (replaces `~/.gitconfig` and the XDG file).
    pub git_config_global: Option<PathBuf>,
    /// `GH_CONFIG_DIR`.
    pub gh_config_dir: Option<PathBuf>,
    /// `GLAB_CONFIG_DIR`.
    pub glab_config_dir: Option<PathBuf>,
    /// git executable used to cross-check config parsing (`git config --file … --list -z`).
    /// `None` → built-in parser only.
    pub git: Option<PathBuf>,
}

impl ScanEnv {
    /// Reads the variables from the current process and uses `git` from PATH.
    pub fn from_process() -> Self {
        let var = |name: &str| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        Self {
            xdg_config_home: var("XDG_CONFIG_HOME"),
            git_config_global: var("GIT_CONFIG_GLOBAL"),
            gh_config_dir: var("GH_CONFIG_DIR"),
            glab_config_dir: var("GLAB_CONFIG_DIR"),
            git: Some(PathBuf::from("git")),
        }
    }
}

/// Scans every supported source. `home` is the user's home directory, `appdata` the Windows
/// `%APPDATA%` (Roaming) folder (`None` on Unix).
pub fn scan(home: &Path, appdata: Option<&Path>, env: &ScanEnv) -> ImportReport {
    let mut report = ImportReport::default();

    let git = scan_gitconfig(home, env);
    report.identities = git.identities;
    report.rules = git.rules;
    report.warnings.extend(git.warnings);
    report.scanned_files.extend(git.files);

    let ssh = scan_ssh_config(home, None);
    report.ssh_hosts = ssh.hosts;
    report.warnings.extend(ssh.warnings);
    report.scanned_files.extend(ssh.files);

    for (candidates, scanner) in [
        (
            gh_hosts_candidates(home, appdata, env),
            scan_gh_hosts as fn(&Path) -> cli_hosts::CliHostsScan,
        ),
        (glab_config_candidates(home, appdata, env), scan_glab_config),
    ] {
        if let Some(path) = candidates.into_iter().find(|p| p.is_file()) {
            let found = scanner(&path);
            report.accounts.extend(found.accounts);
            report.warnings.extend(found.warnings);
            report
                .scanned_files
                .push(path.to_string_lossy().into_owned());
        }
    }
    report
}
