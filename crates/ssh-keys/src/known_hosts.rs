//! Host key pinning for github.com / gitlab.com and helpers for the app's `known_hosts` file.
//!
//! The app keeps its own `known_hosts` (passed with `UserKnownHostsFile`, see
//! [`crate::command`]) and pre-seeds it with the host keys published by GitHub and GitLab, so
//! the first connection to those hosts is verified instead of trusted blindly.

use std::io::Write;
use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::error::{Result, SshKeyError};

/// A host key published by the provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedHostKey {
    pub algorithm: &'static str,
    /// `SHA256:…` as published.
    pub fingerprint: &'static str,
    /// Base64 key blob (second field of a known_hosts line).
    pub key: &'static str,
}

/// GitHub's SSH host keys.
///
/// Source: <https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/githubs-ssh-key-fingerprints>
/// (also `GET https://api.github.com/meta` → `ssh_keys`). The RSA key is the one rotated in on
/// 2023-03-24. Verified against `ssh-keyscan github.com` on 2026-10-14. The same keys serve
/// `ssh.github.com:443`.
pub const GITHUB_HOST_KEYS: [PinnedHostKey; 3] = [
    PinnedHostKey {
        algorithm: "ssh-ed25519",
        fingerprint: "SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU",
        key: "AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl",
    },
    PinnedHostKey {
        algorithm: "ecdsa-sha2-nistp256",
        fingerprint: "SHA256:p2QAMXNIC1TJYWeIOttrVc98/R1BUFWu3/LiyKgUfQM",
        key: "AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBEmKSENjQEezOmxkZMy7opKgwFB9nkt5YRrYMjNuG5N87uRgg6CLrbo5wAdT/y6v0mKV0U2w0WZ2YB/++Tpockg=",
    },
    PinnedHostKey {
        algorithm: "ssh-rsa",
        fingerprint: "SHA256:uNiVztksCsDhcc0u9e8BujQXVUpKZIDTMczCvj3tD2s",
        key: "AAAAB3NzaC1yc2EAAAADAQABAAABgQCj7ndNxQowgcQnjshcLrqPEiiphnt+VTTvDP6mHBL9j1aNUkY4Ue1gvwnGLVlOhGeYrnZaMgRK6+PKCUXaDbC7qtbW8gIkhL7aGCsOr/C56SJMy/BCZfxd1nWzAOxSDPgVsmerOBYfNqltV9/hWCqBywINIR+5dIg6JTJ72pcEpEjcYgXkE2YEFXV1JHnsKgbLWNlhScqb2UmyRkQyytRLtL+38TGxkxCflmO+5Z8CSSNY7GidjMIZ7Q4zMjA2n1nGrlTDkzwDCsw+wqFPGQA179cnfGWOWRVruj16z6XyvxvjJwbz0wQZ75XK5tKSb7FNyeIEs4TT4jk+S4dhPeAUC5y+bDYirYgM4GC7uEnztnZyaVWQ7B381AK4Qdrwt51ZqExKbQpTUNn+EjqoTwvqNj4kqx5QUCI0ThS/YkOxJCXmPUWZbhjpCg56i+2aB6CmK2JGhn57K5mj0MNdBXA4/WnwH6XoPWJzK5Nyu2zB3nAZp+S5hpQs+p1vN1/wsjk=",
    },
];

/// GitLab.com's SSH host keys.
///
/// Source: <https://docs.gitlab.com/user/gitlab_com/#ssh-host-keys-fingerprints> and
/// <https://docs.gitlab.com/user/gitlab_com/#ssh-known_hosts-entries>. Verified against
/// `ssh-keyscan gitlab.com` on 2026-10-14. The same keys serve `altssh.gitlab.com:443`.
pub const GITLAB_HOST_KEYS: [PinnedHostKey; 3] = [
    PinnedHostKey {
        algorithm: "ssh-ed25519",
        fingerprint: "SHA256:eUXGGm1YGsMAS7vkcx6JOJdOGHPem5gQp4taiCfCLB8",
        key: "AAAAC3NzaC1lZDI1NTE5AAAAIAfuCHKVTjquxvt6CM6tdG4SLp1Btn/nOeHHE5UOzRdf",
    },
    PinnedHostKey {
        algorithm: "ecdsa-sha2-nistp256",
        fingerprint: "SHA256:HbW3g8zUjNSksFbqTiUWPWg2Bq1x8xdGUrliXFzSnUw",
        key: "AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBFSMqzJeV9rUzU4kWitGjeR4PWSa29SPqJ1fVkhtj3Hw9xjLVXVYrU9QlYWrOLXBpQ6KWjbjTDTdDkoohFzgbEY=",
    },
    PinnedHostKey {
        algorithm: "ssh-rsa",
        fingerprint: "SHA256:ROQFvPThGrW4RuWLoL9tq9I9zJ42fK4XywyRtbOz/EQ",
        key: "AAAAB3NzaC1yc2EAAAADAQABAAABAQCsj2bNKTBSpIYDEGk9KxsGh3mySTRgMtXL583qmBpzeQ+jqCMRgBqB98u3z++J1sKlXHWfM9dyhSevkMwSbhoR8XIq/U0tCNyokEi/ueaBMCvbcTHhO7FcwzY92WK4Yt0aGROY5qX2UKSeOvuP4D6TPqKF1onrSzH9bx9XUf2lEdWT/ia1NEKjunUqu1xOB/StKDHMoX4/OKyIzuS0q/T1zOATthvasJFoPrAjkohTyaDUz2LN5JoH839hViyEG82yB+MjcFV5MU3N1l1QL3cVUCh93xSaua1N85qivl+siMkPGbO5xR/En4iEY6K2XPASUEMaieWVNTRCtJ4S8H+9",
    },
];

/// known_hosts host fields that carry pinned keys (`[host]:port` for non-22 ports).
const PINNED_HOSTS: [(&str, &[PinnedHostKey]); 4] = [
    ("github.com", &GITHUB_HOST_KEYS),
    ("[ssh.github.com]:443", &GITHUB_HOST_KEYS),
    ("gitlab.com", &GITLAB_HOST_KEYS),
    ("[altssh.gitlab.com]:443", &GITLAB_HOST_KEYS),
];

/// Pinned keys for a host name (port ignored), empty for unknown hosts.
pub fn pinned_host_keys(host: &str) -> &'static [PinnedHostKey] {
    match normalize_host(host).as_str() {
        "github.com" | "ssh.github.com" => &GITHUB_HOST_KEYS,
        "gitlab.com" | "altssh.gitlab.com" => &GITLAB_HOST_KEYS,
        _ => &[],
    }
}

/// Result of comparing a host key fingerprint with the published ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum HostKeyVerdict {
    /// Matches a key published by the provider.
    Trusted,
    /// The host is pinned but this key is not one of its keys — possible MITM, refuse.
    Mismatch,
    /// No pinned keys for this host (self-hosted instance): ask the user.
    UnknownHost,
}

pub fn verify_host_key(host: &str, fingerprint: &str) -> HostKeyVerdict {
    let pinned = pinned_host_keys(host);
    if pinned.is_empty() {
        HostKeyVerdict::UnknownHost
    } else if pinned.iter().any(|k| k.fingerprint == fingerprint) {
        HostKeyVerdict::Trusted
    } else {
        HostKeyVerdict::Mismatch
    }
}

/// known_hosts lines for every pinned host (LF-terminated).
pub fn pinned_known_hosts() -> String {
    let mut out = String::new();
    for (host, keys) in PINNED_HOSTS {
        for k in keys {
            out.push_str(&format!("{host} {} {}\n", k.algorithm, k.key));
        }
    }
    out
}

/// One entry of a known_hosts file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct KnownHostEntry {
    /// 1-based line number.
    pub line: u32,
    /// `@cert-authority` / `@revoked`.
    pub marker: Option<String>,
    /// Host patterns (`github.com`, `[host]:2222`, `*.corp`); empty when hashed.
    pub hosts: Vec<String>,
    /// `|1|salt|hash` entry (hosts cannot be read back).
    pub hashed: bool,
    pub algorithm: String,
    /// Base64 key blob.
    pub key: String,
    /// `SHA256:…`, `None` when the key blob cannot be decoded.
    pub fingerprint: Option<String>,
}

/// Parses known_hosts text; malformed lines are skipped.
pub fn parse_known_hosts(text: &str) -> Vec<KnownHostEntry> {
    let mut entries = Vec::new();
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let mut first = fields.next().unwrap_or("");
        let mut marker = None;
        if first.starts_with('@') {
            marker = Some(first.to_string());
            first = fields.next().unwrap_or("");
        }
        let (Some(algorithm), Some(key)) = (fields.next(), fields.next()) else {
            continue;
        };
        let hashed = first.starts_with('|');
        let hosts = if hashed {
            Vec::new()
        } else {
            first.split(',').map(str::to_string).collect()
        };
        let fingerprint = crate::key::parse_public_key(&format!("{algorithm} {key}"))
            .ok()
            .map(|i| i.fingerprint);
        entries.push(KnownHostEntry {
            line: u32::try_from(idx + 1).unwrap_or(u32::MAX),
            marker,
            hosts,
            hashed,
            algorithm: algorithm.to_string(),
            key: key.to_string(),
            fingerprint,
        });
    }
    entries
}

/// Entries whose (plain, non-negated) host patterns match `host` on `port`.
pub fn entries_for_host<'a>(
    entries: &'a [KnownHostEntry],
    host: &str,
    port: u16,
) -> Vec<&'a KnownHostEntry> {
    let target = host_field(host, port).to_ascii_lowercase();
    entries
        .iter()
        .filter(|e| {
            let mut matched = false;
            for pat in &e.hosts {
                let pat = pat.to_ascii_lowercase();
                if let Some(neg) = pat.strip_prefix('!') {
                    if wildcard_match(neg, &target) {
                        return false;
                    }
                } else if wildcard_match(&pat, &target) {
                    matched = true;
                }
            }
            matched
        })
        .collect()
}

/// What [`seed_known_hosts`] changed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SeedReport {
    pub added: u32,
    /// Stale entries for pinned hosts (e.g. GitHub's pre-2023 RSA key) that were removed.
    pub removed: u32,
}

/// Adds the pinned github.com / gitlab.com keys to the app's known_hosts file (created with
/// its parent directory if needed) and drops stale keys recorded for those exact hosts.
/// Other lines are kept verbatim. Atomic replace.
pub fn seed_known_hosts(path: &Path) -> Result<SeedReport> {
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let mut report = SeedReport::default();
    let mut out = String::new();
    let mut present: Vec<(String, String)> = Vec::new();
    for raw in existing.lines() {
        let entry = parse_known_hosts(raw).into_iter().next();
        if let Some(e) = entry.filter(|e| e.marker.is_none() && !e.hashed) {
            let pinned_for: Vec<&[PinnedHostKey]> = e
                .hosts
                .iter()
                .filter_map(|h| {
                    PINNED_HOSTS
                        .iter()
                        .find(|(p, _)| p.eq_ignore_ascii_case(h))
                        .map(|(_, k)| *k)
                })
                .collect();
            if !pinned_for.is_empty() {
                let known = pinned_for
                    .iter()
                    .all(|keys| keys.iter().any(|k| k.key == e.key));
                if !known {
                    report.removed += 1;
                    continue;
                }
                for h in &e.hosts {
                    present.push((h.to_ascii_lowercase(), e.key.clone()));
                }
            }
        }
        out.push_str(raw.trim_end_matches('\r'));
        out.push('\n');
    }
    for (host, keys) in PINNED_HOSTS {
        for k in keys {
            if !present
                .iter()
                .any(|(h, key)| h.as_str() == host && key == k.key)
            {
                out.push_str(&format!("{host} {} {}\n", k.algorithm, k.key));
                report.added += 1;
            }
        }
    }
    if report.added > 0 || report.removed > 0 {
        let dir = path.parent().filter(|p| !p.as_os_str().is_empty());
        let dir = dir.unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir)?;
        let mut tmp = tempfile::Builder::new()
            .prefix(".tmp-known-hosts-")
            .tempfile_in(dir)?;
        tmp.write_all(out.as_bytes())?;
        tmp.as_file().sync_all()?;
        tmp.persist(path).map_err(|e| SshKeyError::Io(e.error))?;
    }
    Ok(report)
}

/// A host key reported by `ssh-keyscan`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ScannedHostKey {
    pub algorithm: String,
    pub fingerprint: String,
    /// Ready-to-append known_hosts line.
    pub known_hosts_line: String,
    pub verdict: HostKeyVerdict,
}

/// Fetches the host keys of `host:port` with `ssh-keyscan` (shipped with Git for Windows and
/// OpenSSH) and checks them against the pinned keys. Network access; the caller decides
/// whether to show the fingerprints to the user (self-hosted) or to trust them (pinned).
pub fn scan_host_keys(
    keyscan: Option<&Path>,
    host: &str,
    port: u16,
    timeout_secs: u32,
) -> Result<Vec<ScannedHostKey>> {
    if host.is_empty() || host.starts_with('-') || host.contains(char::is_whitespace) {
        return Err(SshKeyError::Tool(format!("invalid host name {host:?}")));
    }
    let program = keyscan
        .map(Path::to_path_buf)
        .unwrap_or_else(|| "ssh-keyscan".into());
    let mut cmd = Command::new(program);
    cmd.args(["-T", &timeout_secs.max(1).to_string()])
        .args(["-p", &port.to_string()])
        .args(["-t", "ed25519,ecdsa,rsa"])
        .arg(host);
    no_window(&mut cmd);
    let out = cmd
        .output()
        .map_err(|e| SshKeyError::Tool(format!("cannot run ssh-keyscan: {e}")))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let keys: Vec<ScannedHostKey> = parse_known_hosts(&text)
        .into_iter()
        .filter_map(|e| {
            let fingerprint = e.fingerprint?;
            Some(ScannedHostKey {
                verdict: verify_host_key(host, &fingerprint),
                known_hosts_line: format!("{} {} {}", host_field(host, port), e.algorithm, e.key),
                algorithm: e.algorithm,
                fingerprint,
            })
        })
        .collect();
    if keys.is_empty() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(SshKeyError::Tool(format!(
            "ssh-keyscan returned no host key for {host}:{port}: {}",
            stderr.trim()
        )));
    }
    Ok(keys)
}

#[cfg(windows)]
fn no_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn no_window(_cmd: &mut Command) {}

/// known_hosts host field: `host` on port 22, `[host]:port` otherwise.
pub fn host_field(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

fn normalize_host(host: &str) -> String {
    let h = host.trim().to_ascii_lowercase();
    match h.strip_prefix('[').and_then(|r| r.split_once(']')) {
        Some((inner, _)) => inner.to_string(),
        None => h,
    }
}

/// `*` / `?` glob as used in known_hosts and ssh_config host patterns.
pub(crate) fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_constants_are_self_consistent() {
        for keys in [&GITHUB_HOST_KEYS, &GITLAB_HOST_KEYS] {
            for k in keys {
                let info = crate::key::parse_public_key(&format!("{} {}", k.algorithm, k.key))
                    .expect("pinned key decodes");
                assert_eq!(info.fingerprint, k.fingerprint, "{}", k.algorithm);
                assert_eq!(info.algorithm, k.algorithm);
            }
        }
    }

    #[test]
    fn verdicts() {
        assert_eq!(
            verify_host_key("github.com", GITHUB_HOST_KEYS[0].fingerprint),
            HostKeyVerdict::Trusted
        );
        assert_eq!(
            verify_host_key("[ssh.github.com]:443", GITHUB_HOST_KEYS[2].fingerprint),
            HostKeyVerdict::Trusted
        );
        assert_eq!(
            verify_host_key("GitLab.com", GITHUB_HOST_KEYS[0].fingerprint),
            HostKeyVerdict::Mismatch
        );
        assert_eq!(
            verify_host_key("git.acme.vn", "SHA256:x"),
            HostKeyVerdict::UnknownHost
        );
    }

    #[test]
    fn parses_and_matches_entries() {
        let text = "# comment\r\n\
                    github.com,140.82.121.4 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl\r\n\
                    |1|abc=|def= ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl\n\
                    @cert-authority *.corp ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl\n\
                    [git.corp]:2222,!bad.corp ssh-rsa notbase64\n\
                    broken-line\n";
        let entries = parse_known_hosts(text);
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].line, 2);
        assert_eq!(
            entries[0].fingerprint.as_deref(),
            Some(GITHUB_HOST_KEYS[0].fingerprint)
        );
        assert!(entries[1].hashed && entries[1].hosts.is_empty());
        assert_eq!(entries[2].marker.as_deref(), Some("@cert-authority"));
        assert_eq!(entries[3].fingerprint, None);

        assert_eq!(entries_for_host(&entries, "GitHub.com", 22).len(), 1);
        assert_eq!(entries_for_host(&entries, "git.corp", 2222).len(), 1);
        // Port 22 only matches the `*.corp` CA line, not `[git.corp]:2222`.
        let on_22 = entries_for_host(&entries, "git.corp", 22);
        assert_eq!(on_22.len(), 1);
        assert!(on_22[0].marker.is_some());
        assert!(entries_for_host(&entries, "bad.corp", 2222).is_empty());
    }

    #[test]
    fn seeds_and_replaces_stale_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("known_hosts");
        let first = seed_known_hosts(&path).unwrap();
        assert_eq!(first.added, 12);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            pinned_known_hosts()
        );
        assert_eq!(seed_known_hosts(&path).unwrap(), SeedReport::default());

        // GitHub's old (pre-2023) RSA key must go; unrelated hosts stay untouched.
        let stale = "github.com ssh-rsa AAAAB3NzaC1yc2EAAAABIwAAAQEAq2A7hRGmdnm9tUDbO9IDSwBK6TbQa+PXYPCPy6rbTrTtw7PHkccKrpp0yVhp5HdEIcKr6pLlVDBfOLX9QUsyCOV0wzfjIJNlGEYsdlLJizHhbn2mUjvSAHQqZETYP81eFzLQNnPHt4EVVUh7VfDESU84KezmD5QlWpXLmvU31/yMf+Se8xhHTvKSCZIFImWwoG6mbUoWf9nzpIoaSjB+weqqUUmpaaasXVal72J+UX2B+2RPW3RcT0eOzQgqlJL3RKrTJvdsjE3JEAvGq3lGHSZXy28G3skua2SmVi/w4yCE6gbODqnTWlg7+wC604ydGXA8VJiS5ap43JXiUFFAaQ==\n";
        std::fs::write(&path, format!("git.corp ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl\n{stale}")).unwrap();
        let report = seed_known_hosts(&path).unwrap();
        assert_eq!(
            report,
            SeedReport {
                added: 12,
                removed: 1
            }
        );
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("git.corp ssh-ed25519"));
        assert!(!text.contains("AAAAB3NzaC1yc2EAAAABIwAAAQEAq2A7"));
    }

    #[test]
    fn wildcard() {
        assert!(wildcard_match("*.github.com", "ssh.github.com"));
        assert!(wildcard_match("git?ub.com", "github.com"));
        assert!(!wildcard_match("*.github.com", "github.com"));
        assert!(wildcard_match("*", ""));
    }
}
