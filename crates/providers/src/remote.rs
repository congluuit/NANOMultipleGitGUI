//! Parsing git remote URLs into `host` + repository path.

use serde::Serialize;

/// `host` (lower-case, with a non-default HTTP(S) port) and repository `path` without `.git`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RemoteRef {
    pub host: String,
    /// `owner/repo` or `group/sub/project`.
    pub path: String,
}

/// Parses `https://host/owner/repo(.git)`, `ssh://git@host[:port]/owner/repo.git` and the scp-like
/// `git@host:owner/repo.git`. Credentials in HTTPS URLs are ignored (never returned).
/// SSH ports are dropped (the API host is what matters); HTTP(S) ports are kept.
pub fn parse_remote_url(url: &str) -> Option<RemoteRef> {
    let url = url.trim();
    let (host, path) = if let Some((scheme, rest)) = url.split_once("://") {
        let scheme = scheme.to_ascii_lowercase();
        let (authority, path) = rest.split_once('/')?;
        let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
        let host = match (scheme.as_str(), authority.rsplit_once(':')) {
            ("ssh" | "git+ssh" | "git", Some((h, _))) => h.to_owned(),
            ("http", Some((h, "80"))) | ("https", Some((h, "443"))) => h.to_owned(),
            ("http" | "https" | "ssh" | "git+ssh" | "git", _) => authority.to_owned(),
            _ => return None,
        };
        (host, path)
    } else {
        // scp-like: [user@]host:path — but not a Windows path such as C:/x.
        let (left, path) = url.split_once(':')?;
        let host = left.rsplit_once('@').map_or(left, |(_, h)| h);
        if host.len() < 2 || host.contains('/') || host.contains('\\') {
            return None;
        }
        (host.to_owned(), path)
    };
    let path = path.split(['?', '#']).next().unwrap_or("");
    let path = path.trim_matches('/');
    let path = path
        .strip_suffix(".git")
        .unwrap_or(path)
        .trim_end_matches('/');
    if host.is_empty() || !path.contains('/') {
        return None;
    }
    Some(RemoteRef {
        host: host.to_ascii_lowercase(),
        path: path.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(host: &str, path: &str) -> Option<RemoteRef> {
        Some(RemoteRef {
            host: host.into(),
            path: path.into(),
        })
    }

    #[test]
    fn parses_common_remote_forms() {
        assert_eq!(
            parse_remote_url("https://github.com/acme/api.git"),
            r("github.com", "acme/api")
        );
        assert_eq!(
            parse_remote_url("https://user:tok@GitHub.com/acme/api/"),
            r("github.com", "acme/api")
        );
        assert_eq!(
            parse_remote_url("git@github.com:acme/api.git"),
            r("github.com", "acme/api")
        );
        assert_eq!(
            parse_remote_url("ssh://git@gitlab.acme.vn:2222/grp/sub/proj.git"),
            r("gitlab.acme.vn", "grp/sub/proj")
        );
        assert_eq!(
            parse_remote_url("https://ghe.acme.com:8443/a/b"),
            r("ghe.acme.com:8443", "a/b")
        );
        assert_eq!(
            parse_remote_url("https://gitlab.com:443/a/b"),
            r("gitlab.com", "a/b")
        );
        assert_eq!(parse_remote_url("C:/repos/x"), None);
        assert_eq!(parse_remote_url("https://github.com/onlyowner"), None);
        assert_eq!(parse_remote_url("file:///tmp/x/y"), None);
    }
}
