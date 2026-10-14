//! Turning git-lfs failures into something a user can act on (PLAN §6.4).
//!
//! git-lfs prints the server's own message when the Batch API returns one (GitHub and GitLab
//! do), otherwise a generic text per HTTP status (`Authorization error: <url>` for 401/403,
//! `Repository or object not found` for 404, …). Recognition is therefore by phrases, checked
//! from the most specific (quota, size limit) to the most generic (network).

use serde::Serialize;

/// Recognisable reasons an LFS transfer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum LfsFailureKind {
    /// 401/403: the account bound to the repository may not read/write its LFS storage.
    NoPermission,
    /// No usable credential (missing, expired or rejected token): sign in again.
    AuthRequired,
    /// LFS storage or bandwidth quota of the account/organisation is used up.
    QuotaExceeded,
    /// An object is larger than the server accepts.
    ObjectTooLarge,
    /// LFS is turned off for the project (GitLab) or the whole server.
    LfsDisabled,
    /// Repository (or LFS endpoint) not found, or not visible to this account.
    NotFound,
    /// The server is throttling requests.
    RateLimited,
    /// Connection problems, timeouts, 5xx: retry, maybe with fewer parallel transfers.
    Network,
    /// An object to push is missing from the local LFS store.
    MissingObject,
    /// Not enough free space on the local disk.
    InsufficientDiskSpace,
    Other,
}

/// A translated LFS failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LfsFailure {
    pub kind: LfsFailureKind,
    /// git-lfs' own output (the app redacts it before showing it).
    pub message: String,
    /// Trying again may succeed (network, rate limit, server errors).
    pub retryable: bool,
    /// Where to buy more LFS storage/bandwidth, for [`LfsFailureKind::QuotaExceeded`].
    pub billing_url: Option<String>,
    /// Project settings page where LFS can be enabled, for [`LfsFailureKind::LfsDisabled`].
    pub settings_url: Option<String>,
    /// Size limit mentioned by the server, for [`LfsFailureKind::ObjectTooLarge`].
    pub limit_bytes: Option<u64>,
    /// Path or oid of the offending object, when the message names one.
    pub object: Option<String>,
}

impl std::fmt::Display for LfsFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

/// Classifies git-lfs output.
pub fn failure_kind(message: &str) -> LfsFailureKind {
    use LfsFailureKind::*;
    let s = message.to_ascii_lowercase();
    let any = |ps: &[&str]| ps.iter().any(|p| s.contains(p));
    if any(&[
        "data quota",
        "lfs budget",
        "quota exceeded",
        "exceeded its quota",
        "storage quota",
        "over the storage limit",
        "over its storage limit",
        "exceeded the storage limit",
        "exceeded its size limit",
        "namespace storage",
        "insufficient server storage",
        "bandwidth limit exceeded",
        "bandwidth quota",
        "purchase more data packs",
    ]) {
        QuotaExceeded
    } else if any(&[
        "size must be less than",
        "exceeds the maximum",
        "maximum allowed size",
        "maximum file size",
        "file size limit",
        "object too large",
        "too large",
        "payload too large",
        "entity too large",
        "[413]",
    ]) {
        ObjectTooLarge
    } else if any(&[
        "lfs is not enabled",
        "lfs is disabled",
        "lfs disabled",
        "lfs support is disabled",
        "lfs is not supported",
        "lfs not enabled",
        "not implemented",
        "[501]",
    ]) {
        LfsDisabled
    } else if any(&[
        "credentials for",
        "terminal prompts disabled",
        "bad credentials",
        "invalid username or password",
        "invalid credentials",
        "authentication failed",
        "could not read username",
        "token has expired",
        "[401]",
    ]) {
        AuthRequired
    } else if any(&[
        "authorization error",
        "authentication required",
        "access forbidden",
        "access denied",
        "permission denied",
        "you do not have access",
        "do not have permission",
        "not allowed to push",
        "write access",
        "[403]",
    ]) {
        NoPermission
    } else if any(&["rate limit", "too many requests", "[429]"]) {
        RateLimited
    } else if any(&[
        "not enough space",
        "no space left",
        "disk full",
        "insufficient disk space",
    ]) {
        InsufficientDiskSpace
    } else if any(&[
        "unable to find source for object",
        "missing object",
        "object does not exist locally",
    ]) {
        MissingObject
    } else if any(&[
        "repository or object not found",
        "repository not found",
        "not found: ",
        "[404]",
    ]) {
        NotFound
    } else if any(&[
        "dial tcp",
        "connection refused",
        "connection reset",
        "connectex",
        "wsarecv",
        "wsasend",
        "broken pipe",
        "i/o timeout",
        "timeout",
        "timed out",
        "deadline exceeded",
        "tls handshake",
        "no such host",
        "could not resolve",
        "network is unreachable",
        "unexpected eof",
        ": eof",
        "server error",
        "lfs: client error",
        "bad gateway",
        "service unavailable",
        "[500]",
        "[502]",
        "[503]",
        "[504]",
    ]) {
        Network
    } else {
        Other
    }
}

/// Number following `less than or equal to` / `exceeds` / `limit of` (bytes, or with a
/// `KB/MB/GB` unit) in a size-limit message.
fn limit_bytes(message: &str) -> Option<u64> {
    let lower = message.to_ascii_lowercase();
    for marker in [
        "less than or equal to ",
        "less than ",
        "limit of ",
        "maximum of ",
        "exceeds ",
    ] {
        let Some(pos) = lower.find(marker) else {
            continue;
        };
        let rest = &lower[pos + marker.len()..];
        let digits: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        let Ok(value) = digits.parse::<f64>() else {
            continue;
        };
        let unit = rest[digits.len()..].trim_start();
        let factor: f64 = if unit.starts_with("gib") || unit.starts_with("gb") {
            1024.0 * 1024.0 * 1024.0
        } else if unit.starts_with("mib") || unit.starts_with("mb") {
            1024.0 * 1024.0
        } else if unit.starts_with("kib") || unit.starts_with("kb") {
            1024.0
        } else {
            1.0
        };
        return Some((value * factor) as u64);
    }
    None
}

/// First 64-hex-digit word (an LFS oid) in the message.
fn object_in(message: &str) -> Option<String> {
    message
        .split(|c: char| !c.is_ascii_hexdigit())
        .find(|w| w.len() == 64)
        .map(str::to_owned)
}

/// Host and `owner/name` path of a remote URL (`https://host/a/b.git`, `git@host:a/b.git`,
/// `ssh://git@host:22/a/b.git`). Credentials and ports are dropped.
pub fn remote_location(url: &str) -> Option<(String, String)> {
    let url = url.trim();
    let (host, path) = if let Some((_, rest)) = url.split_once("://") {
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let host = authority.rsplit('@').next().unwrap_or(authority);
        (host.split(':').next().unwrap_or(host), path)
    } else if let Some((authority, path)) = url.split_once(':') {
        // scp-like syntax; reject Windows drive letters (`C:/x`).
        if authority.len() < 2 || path.starts_with('\\') {
            return None;
        }
        let host = authority.rsplit('@').next().unwrap_or(authority);
        (host, path)
    } else {
        return None;
    };
    let path = path.trim_matches('/').trim_end_matches(".git").to_owned();
    if host.is_empty() || path.is_empty() {
        return None;
    }
    Some((host.to_ascii_lowercase(), path))
}

/// Billing page for LFS storage/bandwidth of the provider hosting `remote_url`.
pub fn billing_url(remote_url: &str) -> Option<String> {
    let (host, path) = remote_location(remote_url)?;
    let owner = path.split('/').next().unwrap_or_default();
    if host == "github.com" || host.ends_with(".github.com") {
        // Personal accounts and organisations have separate billing pages; the remote URL
        // does not say which one `owner` is, so link the user's page that lists both.
        Some("https://github.com/settings/billing".to_owned())
    } else if host == "gitlab.com" {
        Some(format!("https://gitlab.com/groups/{owner}/-/usage_quotas"))
    } else if host.contains("gitlab") {
        Some(format!("https://{host}/-/profile/usage_quotas"))
    } else {
        None
    }
}

/// GitLab project settings page (Visibility, project features, permissions → Git LFS).
pub fn lfs_settings_url(remote_url: &str) -> Option<String> {
    let (host, path) = remote_location(remote_url)?;
    if host == "github.com" || host.ends_with(".github.com") {
        return None;
    }
    Some(format!("https://{host}/{path}/edit#js-shared-permissions"))
}

/// Translates git-lfs output into an [`LfsFailure`]; `remote_url` adds provider links.
pub fn translate(message: &str, remote_url: Option<&str>) -> LfsFailure {
    let kind = failure_kind(message);
    let url = remote_url.unwrap_or_default();
    LfsFailure {
        kind,
        message: message.trim().to_owned(),
        retryable: matches!(
            kind,
            LfsFailureKind::Network | LfsFailureKind::RateLimited | LfsFailureKind::Other
        ),
        billing_url: (kind == LfsFailureKind::QuotaExceeded)
            .then(|| billing_url(url))
            .flatten(),
        settings_url: (kind == LfsFailureKind::LfsDisabled)
            .then(|| lfs_settings_url(url))
            .flatten(),
        limit_bytes: (kind == LfsFailureKind::ObjectTooLarge)
            .then(|| limit_bytes(message))
            .flatten(),
        object: object_in(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use LfsFailureKind::*;

    #[test]
    fn recognises_real_messages() {
        for (message, kind) in [
            // Local test server, wrong account (HTTP 403), git-lfs 3.5.1.
            (
                "batch response: Authorization error: http://127.0.0.1:53316/alice/app.git/info/lfs/objects/batch\nCheck that you have proper access to the repository",
                NoPermission,
            ),
            // GitLab, Reporter role.
            (
                "batch response: Access forbidden. Check your access level.",
                NoPermission,
            ),
            (
                "Git credentials for https://github.com/acme/app.git not found.",
                AuthRequired,
            ),
            ("batch response: Bad credentials", AuthRequired),
            // GitHub, storage/bandwidth quota.
            (
                "batch response: This repository is over its data quota. Account responsible for LFS bandwidth should purchase more data packs to restore access.",
                QuotaExceeded,
            ),
            (
                "batch response: This repository exceeded its LFS budget. The account responsible for the budget should increase it to restore access.",
                QuotaExceeded,
            ),
            (
                "Insufficient server storage: https://x/info/lfs",
                QuotaExceeded,
            ),
            // GitHub, object over the per-file limit.
            (
                "[e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855] Size must be less than or equal to 2147483648: [422] Size must be less than or equal to 2147483648",
                ObjectTooLarge,
            ),
            // GitLab with LFS turned off.
            (
                "batch response: Git LFS is not enabled on this GitLab server, contact your admin.",
                LfsDisabled,
            ),
            (
                "Repository or object not found: https://gitlab.com/acme/app.git/info/lfs/objects/batch\nCheck that it exists and that you have proper access to it",
                NotFound,
            ),
            ("Rate limit exceeded: https://x", RateLimited),
            (
                "LFS: Put \"https://x/obj\": dial tcp 10.0.0.1:443: connectex: No connection could be made because the target machine actively refused it.",
                Network,
            ),
            (
                "LFS: Put \"https://x/obj\": read tcp 1.2.3.4:5->6.7.8.9:443: i/o timeout",
                Network,
            ),
            ("Fatal error: Server error: https://x [502]", Network),
            (
                "Unable to find source for object 3f1c... (try running `git lfs fetch --all`)",
                MissingObject,
            ),
            (
                "write C:/repo/.git/lfs/tmp/x: There is not enough space on the disk.",
                InsufficientDiskSpace,
            ),
            ("something entirely different", Other),
        ] {
            assert_eq!(failure_kind(message), kind, "{message}");
        }
    }

    #[test]
    fn adds_provider_links_and_details() {
        let f = translate(
            "batch response: This repository is over its data quota.",
            Some("https://github.com/acme/app.git"),
        );
        assert_eq!(f.kind, QuotaExceeded);
        assert_eq!(
            f.billing_url.as_deref(),
            Some("https://github.com/settings/billing")
        );
        assert!(!f.retryable);

        let f = translate(
            "Git LFS is not enabled on this GitLab server",
            Some("git@gitlab.acme.dev:team/sub/app.git"),
        );
        assert_eq!(f.kind, LfsDisabled);
        assert_eq!(
            f.settings_url.as_deref(),
            Some("https://gitlab.acme.dev/team/sub/app/edit#js-shared-permissions")
        );

        let f = translate(
            "over the storage limit",
            Some("https://oauth2:tok@gitlab.com/acme/app.git"),
        );
        assert_eq!(
            f.billing_url.as_deref(),
            Some("https://gitlab.com/groups/acme/-/usage_quotas")
        );

        let oid = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let f = translate(
            &format!("[{oid}] Size must be less than or equal to 2147483648: [422]"),
            None,
        );
        assert_eq!(f.kind, ObjectTooLarge);
        assert_eq!(f.limit_bytes, Some(2_147_483_648));
        assert_eq!(f.object.as_deref(), Some(oid));

        let f = translate("file exceeds the maximum of 5 GB", None);
        assert_eq!(f.limit_bytes, Some(5 * 1024 * 1024 * 1024));

        let f = translate("dial tcp: i/o timeout", None);
        assert!(f.retryable);
        assert_eq!(f.billing_url, None);
    }

    #[test]
    fn parses_remote_locations() {
        for (url, expect) in [
            (
                "https://github.com/acme/app.git",
                Some(("github.com", "acme/app")),
            ),
            (
                "https://user:tok@GitHub.com:443/acme/app",
                Some(("github.com", "acme/app")),
            ),
            (
                "git@gitlab.com:group/sub/app.git",
                Some(("gitlab.com", "group/sub/app")),
            ),
            ("ssh://git@host.dev:2222/a/b.git", Some(("host.dev", "a/b"))),
            ("C:/repos/remote.git", None),
            ("../remote.git", None),
        ] {
            let got = remote_location(url);
            assert_eq!(
                got.as_ref().map(|(h, p)| (h.as_str(), p.as_str())),
                expect,
                "{url}"
            );
        }
        assert_eq!(billing_url("http://127.0.0.1:8080/alice/app.git"), None);
    }
}
