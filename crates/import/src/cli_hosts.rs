//! GitHub CLI (`gh`) `hosts.yml` and GitLab CLI (`glab`) `config.yml` → suggested accounts.
//!
//! Tokens are never returned: before the YAML parser sees the text, the value of every key
//! containing `token` is replaced by a placeholder, and the raw text is zeroized. The scan
//! only reports *whether* a token is stored so the wizard can ask the user to sign in again.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use yaml_rust2::{Yaml, YamlLoader};
use zeroize::Zeroize;

use crate::{ImportWarning, ProviderKind, ScanEnv, SourceRef, SuggestionNote, WarningKind};

const REDACTED: &str = "\"<redacted>\"";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum CliTool {
    Gh,
    Glab,
}

/// Where the CLI keeps the account's token (the value itself is never read).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum TokenStorage {
    /// Plain text in the CLI config file.
    ConfigFile,
    /// Not in the file: gh keeps it in the OS keyring (its default since 2.40).
    Keyring,
    /// Not in the file and the CLI may not have one at all.
    Unknown,
}

/// A signed-in CLI account (PLAN.md 4.1 `Account`, to be confirmed by the user).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SuggestedAccount {
    pub tool: CliTool,
    pub provider: ProviderKind,
    /// Host as configured (`github.com`, `gitlab.acme.vn`).
    pub host: String,
    /// glab `api_host` when it differs from `host`.
    pub api_host: Option<String>,
    /// `None` when the file does not record the user name.
    pub username: Option<String>,
    /// The CLI's active account for this host.
    pub active: bool,
    /// `https` / `ssh`.
    pub git_protocol: Option<String>,
    pub token_storage: TokenStorage,
    pub notes: Vec<SuggestionNote>,
    pub source: SourceRef,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CliHostsScan {
    pub accounts: Vec<SuggestedAccount>,
    pub warnings: Vec<ImportWarning>,
}

/// gh `hosts.yml` locations in gh's order: `GH_CONFIG_DIR`, `XDG_CONFIG_HOME/gh`,
/// `%APPDATA%\GitHub CLI` (Windows), `~/.config/gh`.
pub fn gh_hosts_candidates(home: &Path, appdata: Option<&Path>, env: &ScanEnv) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    dirs.extend(env.gh_config_dir.clone());
    dirs.extend(env.xdg_config_home.as_ref().map(|x| x.join("gh")));
    dirs.extend(appdata.map(|a| a.join("GitHub CLI")));
    dirs.push(home.join(".config").join("gh"));
    dirs.into_iter().map(|d| d.join("hosts.yml")).collect()
}

/// glab `config.yml` locations: `GLAB_CONFIG_DIR`, `XDG_CONFIG_HOME/glab-cli`,
/// `%APPDATA%\glab-cli` (Windows, glab ≥ 1.4x), `~/.config/glab-cli`.
pub fn glab_config_candidates(home: &Path, appdata: Option<&Path>, env: &ScanEnv) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    dirs.extend(env.glab_config_dir.clone());
    dirs.extend(env.xdg_config_home.as_ref().map(|x| x.join("glab-cli")));
    dirs.extend(appdata.map(|a| a.join("glab-cli")));
    dirs.push(home.join(".config").join("glab-cli"));
    dirs.into_iter().map(|d| d.join("config.yml")).collect()
}

/// Replaces the value of every `*token*: value` line; returns the redacted text.
fn redact_tokens(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']);
        let ending = &line[body.len()..];
        let trimmed = body.trim_start();
        let redacted = trimmed.split_once(':').and_then(|(key, value)| {
            let key = key.trim().trim_matches(['"', '\'']).to_ascii_lowercase();
            if !key.contains("token") || key.contains(' ') || trimmed.starts_with('#') {
                return None;
            }
            let v = value.trim();
            let empty = v.is_empty()
                || matches!(v, "~" | "null" | "Null" | "NULL" | "\"\"" | "''" | "!!null")
                || v.starts_with('#');
            let indent = &body[..body.len() - trimmed.len()];
            let raw_key = trimmed.split_once(':').map(|(k, _)| k).unwrap_or_default();
            Some(if empty {
                format!("{indent}{raw_key}: null")
            } else {
                format!("{indent}{raw_key}: {REDACTED}")
            })
        });
        match redacted {
            Some(r) => out.push_str(&r),
            None => out.push_str(body),
        }
        out.push_str(ending);
    }
    out
}

/// Loads a YAML file with tokens redacted; `Err` is a ready warning.
fn load_yaml(path: &Path) -> Result<(Yaml, Vec<String>), ImportWarning> {
    let warn = |kind, message: String| ImportWarning {
        kind,
        source: SourceRef::new(path, None),
        message,
    };
    let mut bytes =
        std::fs::read(path).map_err(|e| warn(WarningKind::Unreadable, e.to_string()))?;
    let text = match std::str::from_utf8(&bytes) {
        Ok(t) => redact_tokens(t.trim_start_matches('\u{feff}')),
        Err(_) => {
            bytes.zeroize();
            return Err(warn(
                WarningKind::Unreadable,
                "file is not valid UTF-8".into(),
            ));
        }
    };
    bytes.zeroize();
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    let docs = YamlLoader::load_from_str(&text).map_err(|e| {
        let mut w = warn(WarningKind::Parse, e.to_string());
        w.source.line = u32::try_from(e.marker().line()).ok();
        w
    })?;
    let doc = docs.into_iter().next().unwrap_or(Yaml::Null);
    Ok((doc, lines))
}

fn key_str(y: &Yaml) -> Option<String> {
    match y {
        Yaml::String(s) | Yaml::Real(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

fn str_of(y: &Yaml) -> Option<String> {
    key_str(y).filter(|s| !s.is_empty())
}

fn has_token(y: &Yaml) -> bool {
    matches!(y, Yaml::String(s) if !s.is_empty())
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// 0-based index of the line defining `key` below the line `parent` (None = top level).
fn find_key(lines: &[String], key: &str, parent: Option<usize>) -> Option<usize> {
    let (start, parent_indent) = match parent {
        Some(p) => (p + 1, Some(indent_of(&lines[p]))),
        None => (0, None),
    };
    for (i, line) in lines.iter().enumerate().skip(start) {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let indent = indent_of(line);
        match parent_indent {
            Some(p) if indent <= p => return None,
            None if indent > 0 => continue,
            _ => {}
        }
        let matches = [
            format!("{key}:"),
            format!("\"{key}\":"),
            format!("'{key}':"),
        ]
        .iter()
        .any(|k| t.starts_with(k.as_str()));
        if matches {
            return Some(i);
        }
    }
    None
}

fn line_ref(path: &Path, idx: Option<usize>) -> SourceRef {
    SourceRef::new(path, idx.and_then(|i| u32::try_from(i + 1).ok()))
}

fn notes_for(storage: TokenStorage) -> Vec<SuggestionNote> {
    let mut notes = vec![SuggestionNote::SignInAgain];
    if storage == TokenStorage::ConfigFile {
        notes.push(SuggestionNote::TokenInPlainText);
    }
    notes
}

/// Parses gh's `hosts.yml` (multi-account `users:` format of gh ≥ 2.40 and the older
/// single-user format).
pub fn scan_gh_hosts(path: &Path) -> CliHostsScan {
    let mut scan = CliHostsScan::default();
    let (doc, lines) = match load_yaml(path) {
        Ok(v) => v,
        Err(w) => {
            scan.warnings.push(w);
            return scan;
        }
    };
    let Some(hosts) = doc.as_hash() else {
        if !doc.is_null() {
            scan.warnings.push(ImportWarning {
                kind: WarningKind::Parse,
                source: SourceRef::new(path, None),
                message: "expected a mapping of hosts".into(),
            });
        }
        return scan;
    };
    for (host_key, host_val) in hosts {
        let (Some(host), Some(_)) = (key_str(host_key), host_val.as_hash()) else {
            continue;
        };
        let host_line = find_key(&lines, &host, None);
        let active_user = str_of(&host_val["user"]);
        let git_protocol = str_of(&host_val["git_protocol"]);
        let top_token = has_token(&host_val["oauth_token"]);
        let provider = match ProviderKind::guess(&host) {
            ProviderKind::Github => ProviderKind::Github,
            _ => ProviderKind::GithubEnterprise,
        };
        let storage = |in_file: bool| {
            if in_file {
                TokenStorage::ConfigFile
            } else {
                TokenStorage::Keyring
            }
        };
        let mut users: Vec<(String, bool, Option<usize>)> = Vec::new();
        if let Some(map) = host_val["users"].as_hash() {
            let users_line = host_line.and_then(|h| find_key(&lines, "users", Some(h)));
            for (u, uval) in map {
                let Some(name) = key_str(u) else { continue };
                let line = users_line.and_then(|l| find_key(&lines, &name, Some(l)));
                let in_file = has_token(&uval["oauth_token"])
                    || (top_token && active_user.as_deref() == Some(name.as_str()));
                users.push((name, in_file, line));
            }
        }
        if let Some(active) = &active_user
            && !users.iter().any(|(n, _, _)| n == active)
        {
            let line = host_line.and_then(|h| find_key(&lines, "user", Some(h)));
            users.push((active.clone(), top_token, line.or(host_line)));
        }
        if users.is_empty() && top_token {
            scan.accounts.push(SuggestedAccount {
                tool: CliTool::Gh,
                provider,
                host: host.clone(),
                api_host: None,
                username: None,
                active: true,
                git_protocol: git_protocol.clone(),
                token_storage: TokenStorage::ConfigFile,
                notes: notes_for(TokenStorage::ConfigFile),
                source: line_ref(path, host_line),
            });
        }
        for (name, in_file, line) in users {
            scan.accounts.push(SuggestedAccount {
                tool: CliTool::Gh,
                provider,
                host: host.clone(),
                api_host: None,
                active: active_user.as_deref() == Some(name.as_str()),
                username: Some(name),
                git_protocol: git_protocol.clone(),
                token_storage: storage(in_file),
                notes: notes_for(storage(in_file)),
                source: line_ref(path, line.or(host_line)),
            });
        }
    }
    scan
}

/// Parses glab's `config.yml` (`hosts:` map). Hosts with neither user nor token (glab's
/// default template entry) are skipped.
pub fn scan_glab_config(path: &Path) -> CliHostsScan {
    let mut scan = CliHostsScan::default();
    let (doc, lines) = match load_yaml(path) {
        Ok(v) => v,
        Err(w) => {
            scan.warnings.push(w);
            return scan;
        }
    };
    let default_protocol = str_of(&doc["git_protocol"]);
    let Some(hosts) = doc["hosts"].as_hash() else {
        return scan;
    };
    let hosts_line = find_key(&lines, "hosts", None);
    for (host_key, host_val) in hosts {
        let (Some(host), Some(_)) = (key_str(host_key), host_val.as_hash()) else {
            continue;
        };
        let user = str_of(&host_val["user"]);
        let in_file = has_token(&host_val["token"]);
        if user.is_none() && !in_file {
            continue;
        }
        let line = hosts_line.and_then(|h| find_key(&lines, &host, Some(h)));
        let api_host = str_of(&host_val["api_host"]).filter(|a| !a.eq_ignore_ascii_case(&host));
        let provider = match ProviderKind::guess(&host) {
            ProviderKind::Gitlab => ProviderKind::Gitlab,
            _ => ProviderKind::GitlabSelfHosted,
        };
        let storage = if in_file {
            TokenStorage::ConfigFile
        } else {
            TokenStorage::Unknown
        };
        scan.accounts.push(SuggestedAccount {
            tool: CliTool::Glab,
            provider,
            host,
            api_host,
            username: user,
            active: true,
            git_protocol: str_of(&host_val["git_protocol"]).or_else(|| default_protocol.clone()),
            token_storage: storage,
            notes: notes_for(storage),
            source: line_ref(path, line),
        });
    }
    scan
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_every_token_value() {
        let text = "github.com:\r\n    oauth_token: gho_SECRET\r\n    user: an\r\n\
                    x:\n  token: !!null\n  \"oauth2_refresh_token\": 'abc'\n  job_token:\n  # token: c\n";
        let r = redact_tokens(text);
        assert!(!r.contains("SECRET") && !r.contains("abc"), "{r}");
        assert!(r.contains("    oauth_token: \"<redacted>\"\r\n"));
        assert!(r.contains("  token: null\n"));
        assert!(r.contains("  job_token: null\n"));
        assert!(r.contains("  # token: c\n"));
        assert!(r.contains("    user: an\r\n"));
    }

    #[test]
    fn finds_nested_key_lines() {
        let lines: Vec<String> = "a:\n  users:\n    bob:\n    alice:\nb:\n  users:\n    bob:\n"
            .lines()
            .map(str::to_string)
            .collect();
        let b = find_key(&lines, "b", None);
        assert_eq!(b, Some(4));
        let users = find_key(&lines, "users", b);
        assert_eq!(users, Some(5));
        assert_eq!(find_key(&lines, "bob", users), Some(6));
        let a_users = find_key(&lines, "users", Some(0));
        assert_eq!(find_key(&lines, "alice", a_users), Some(3));
        assert_eq!(find_key(&lines, "alice", users), None);
    }
}
