//! Remote URL parsing for rule matching and host checks.
//!
//! Understands every form git accepts for a network remote:
//! - `https://[user[:password]@]host[:port]/owner/repo.git` (and `http://`),
//! - `ssh://[user@]host[:port]/owner/repo.git` (also `git+ssh://`, `ssh+git://`),
//! - scp-like `[user@]host:owner/repo.git` (no `/` before the first `:`),
//! - `git://host[:port]/owner/repo.git`,
//! - local paths and `file://` URLs (reported as [`UrlScheme::Local`], never matched by host).
//!
//! GitLab subgroups are kept: `group/sub/repo` has owner `group` and namespace `group/sub`.
//! A password embedded in the URL is dropped while parsing so it can never leak into rule
//! previews or logs.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum UrlScheme {
    Https,
    Http,
    Ssh,
    Git,
    Local,
}

impl UrlScheme {
    fn default_port(self) -> Option<u16> {
        match self {
            Self::Https => Some(443),
            Self::Http => Some(80),
            Self::Ssh => Some(22),
            Self::Git => Some(9418),
            Self::Local => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RemoteUrl {
    pub scheme: UrlScheme,
    /// User name from the URL (`git` for most SSH remotes). Never the password.
    pub user: Option<String>,
    /// Lower-cased host name without brackets or port; empty for local remotes.
    pub host: String,
    pub port: Option<u16>,
    /// Repository path without leading `/`, trailing `/` or `.git`, e.g. `acme/platform/api`.
    /// For local remotes: the path as written, with `/` separators.
    pub path: String,
}

fn is_dos_drive(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && (b.len() == 2 || matches!(b[2], b'/' | b'\\'))
}

fn clean_path(path: &str) -> String {
    let mut p = path.trim_start_matches('/');
    p = p.strip_prefix("~/").unwrap_or(p);
    let p = p.trim_end_matches('/');
    let p = p.strip_suffix(".git").unwrap_or(p);
    p.trim_end_matches('/').to_owned()
}

/// Splits `host[:port]` / `[v6]:port`. Returns `None` for an empty host or invalid port.
fn split_host_port(hostport: &str) -> Option<(String, Option<u16>)> {
    let (host, port) = if let Some(rest) = hostport.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        match after.strip_prefix(':') {
            Some(p) if !p.is_empty() => (host, Some(p.parse().ok()?)),
            _ if after.is_empty() || after == ":" => (host, None),
            _ => return None,
        }
    } else {
        match hostport.rsplit_once(':') {
            Some((h, "")) => (h, None),
            Some((h, p)) => (h, Some(p.parse().ok()?)),
            None => (hostport, None),
        }
    };
    if host.is_empty() || host.contains(['/', '\\', ' ', '@']) {
        return None;
    }
    Some((host.to_ascii_lowercase(), port))
}

/// `user[:password]` → `user`.
fn user_of(userinfo: &str) -> Option<String> {
    let user = userinfo.split(':').next().unwrap_or_default();
    (!user.is_empty()).then(|| user.to_owned())
}

impl RemoteUrl {
    pub fn parse(input: &str) -> Option<Self> {
        let s = input.trim();
        if s.is_empty() {
            return None;
        }
        if let Some((scheme, rest)) = s.split_once("://") {
            let scheme = match scheme.to_ascii_lowercase().as_str() {
                "https" => UrlScheme::Https,
                "http" => UrlScheme::Http,
                "ssh" | "git+ssh" | "ssh+git" => UrlScheme::Ssh,
                "git" => UrlScheme::Git,
                "file" => return Some(Self::local(rest)),
                _ => return None,
            };
            let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
            let (user, hostport) = match authority.rsplit_once('@') {
                Some((userinfo, hp)) => (user_of(userinfo), hp),
                None => (None, authority),
            };
            let (host, port) = split_host_port(hostport)?;
            return Some(Self {
                scheme,
                user,
                host,
                port,
                path: clean_path(path),
            });
        }
        if is_dos_drive(s) || s.starts_with(['/', '\\', '.', '~']) {
            return Some(Self::local(s));
        }
        // scp-like syntax applies only when no `/` comes before the first `:` (git's rule).
        let first_colon = s.find(':')?;
        let colon = if s[..first_colon].contains('[') {
            s.find("]:").map(|i| i + 1)?
        } else {
            first_colon
        };
        if s[..colon].contains('/') {
            return Some(Self::local(s));
        }
        let (authority, path) = (&s[..colon], &s[colon + 1..]);
        let (user, host) = match authority.rsplit_once('@') {
            Some((u, h)) => (user_of(u), h),
            None => (None, authority),
        };
        let host = host.trim_start_matches('[').trim_end_matches(']');
        if host.is_empty() || host.contains(['\\', ' ', '@']) {
            return None;
        }
        Some(Self {
            scheme: UrlScheme::Ssh,
            user,
            host: host.to_ascii_lowercase(),
            port: None,
            path: clean_path(path),
        })
    }

    fn local(path: &str) -> Self {
        Self {
            scheme: UrlScheme::Local,
            user: None,
            host: String::new(),
            port: None,
            path: path.replace('\\', "/"),
        }
    }

    pub fn is_local(&self) -> bool {
        self.scheme == UrlScheme::Local
    }

    pub fn is_ssh(&self) -> bool {
        self.scheme == UrlScheme::Ssh
    }

    /// `host` plus `:port` when the port differs from the scheme default — the form git puts
    /// in credential requests and providers use in their base URL.
    pub fn host_port(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        match self.port {
            Some(p) if Some(p) != self.scheme.default_port() => format!("{host}:{p}"),
            _ => host,
        }
    }

    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.path.split('/').filter(|s| !s.is_empty())
    }

    /// First path segment: user or top-level group.
    pub fn owner(&self) -> Option<&str> {
        if self.is_local() {
            return None;
        }
        let mut segs = self.segments();
        let first = segs.next()?;
        segs.next().map(|_| first)
    }

    /// Everything but the repository name: `group/sub` for `group/sub/repo`.
    pub fn namespace(&self) -> Option<&str> {
        if self.is_local() {
            return None;
        }
        self.path.rsplit_once('/').map(|(ns, _)| ns)
    }

    pub fn repo_name(&self) -> Option<&str> {
        self.segments().last()
    }
}

/// Removes a password from `scheme://user:password@host/...`, leaving the rest untouched.
/// Used before showing or regex-matching a URL as the user typed it.
pub fn redact_url(raw: &str) -> String {
    let Some((scheme, rest)) = raw.split_once("://") else {
        return raw.to_owned();
    };
    let authority_end = rest.find('/').unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    match authority.rsplit_once('@') {
        Some((userinfo, hostport)) if userinfo.contains(':') => {
            let user = userinfo.split(':').next().unwrap_or_default();
            format!("{scheme}://{user}@{hostport}{}", &rest[authority_end..])
        }
        _ => raw.to_owned(),
    }
}

/// Host name without port (`gitlab.acme.dev:8443` → `gitlab.acme.dev`), lower-cased.
pub fn hostname_of(host_port: &str) -> String {
    let h = host_port.trim().to_ascii_lowercase();
    if let Some(rest) = h.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest).to_owned();
    }
    match h.rsplit_once(':') {
        Some((name, port)) if port.chars().all(|c| c.is_ascii_digit()) => name.to_owned(),
        _ => h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> RemoteUrl {
        RemoteUrl::parse(s).unwrap_or_else(|| panic!("failed to parse {s}"))
    }

    #[test]
    fn https_urls() {
        let u = p("https://github.com/acme-corp/api.git");
        assert_eq!(u.scheme, UrlScheme::Https);
        assert_eq!(u.host, "github.com");
        assert_eq!(u.path, "acme-corp/api");
        assert_eq!(u.owner(), Some("acme-corp"));
        assert_eq!(u.repo_name(), Some("api"));
        assert_eq!(u.host_port(), "github.com");

        let u = p("HTTPS://alice:ghp_secret@GitHub.com:443/Acme/Api/");
        assert_eq!(u.user.as_deref(), Some("alice"));
        assert_eq!(u.host, "github.com");
        assert_eq!(u.host_port(), "github.com");
        assert_eq!(u.path, "Acme/Api");
        assert!(!format!("{u:?}").contains("ghp_secret"));

        let u = p("http://127.0.0.1:8080/alice/app.git");
        assert_eq!(u.host_port(), "127.0.0.1:8080");
        assert_eq!(u.port, Some(8080));
    }

    #[test]
    fn ssh_urls_and_scp_syntax() {
        let u = p("git@github.com:acme-corp/api.git");
        assert_eq!(u.scheme, UrlScheme::Ssh);
        assert_eq!(u.user.as_deref(), Some("git"));
        assert_eq!(u.host, "github.com");
        assert_eq!(u.path, "acme-corp/api");

        let u = p("ssh://git@gitlab.acme.dev:2222/platform/backend/api.git");
        assert_eq!(u.host_port(), "gitlab.acme.dev:2222");
        assert_eq!(u.owner(), Some("platform"));
        assert_eq!(u.namespace(), Some("platform/backend"));
        assert_eq!(u.repo_name(), Some("api"));

        let u = p("ssh://git@github.com:22/acme/api");
        assert_eq!(u.host_port(), "github.com");

        let u = p("gitlab.com:group/sub/repo");
        assert_eq!(u.user, None);
        assert_eq!(u.namespace(), Some("group/sub"));

        let u = p("git+ssh://me@[::1]:2200/a/b.git");
        assert_eq!(u.host, "::1");
        assert_eq!(u.host_port(), "[::1]:2200");

        let u = p("git@[fe80::1]:team/repo.git");
        assert_eq!(u.host, "fe80::1");
        assert_eq!(u.path, "team/repo");

        let u = p("ssh://git@host/~user/repo.git");
        assert_eq!(u.path, "~user/repo");

        let u = p("git://example.org:9418/x/y.git");
        assert_eq!(u.scheme, UrlScheme::Git);
        assert_eq!(u.host_port(), "example.org");
    }

    #[test]
    fn local_paths_are_not_network_remotes() {
        for s in [
            "C:/Users/me/repo.git",
            "C:\\Users\\me\\repo",
            "/srv/git/repo.git",
            "../sibling",
            "./x",
            "file:///srv/git/repo.git",
            "dir/with:colon",
        ] {
            let u = p(s);
            assert!(u.is_local(), "{s}");
            assert_eq!(u.owner(), None);
            assert!(u.host.is_empty());
        }
        assert_eq!(p("C:\\a\\b").path, "C:/a/b");
    }

    #[test]
    fn rejects_garbage() {
        for s in [
            "",
            "   ",
            "ftp://x/y",
            "https://",
            "https://host:notaport/x",
            "@:x",
        ] {
            assert_eq!(RemoteUrl::parse(s), None, "{s}");
        }
    }

    #[test]
    fn redaction_and_hostnames() {
        assert_eq!(
            redact_url("https://bob:glpat-123@gitlab.com/a/b.git"),
            "https://bob@gitlab.com/a/b.git"
        );
        assert_eq!(
            redact_url("git@github.com:a/b.git"),
            "git@github.com:a/b.git"
        );
        assert_eq!(
            redact_url("https://github.com/a:b@c"),
            "https://github.com/a:b@c"
        );
        assert_eq!(hostname_of("GitLab.acme.dev:8443"), "gitlab.acme.dev");
        assert_eq!(hostname_of("[::1]:80"), "::1");
        assert_eq!(hostname_of("github.com"), "github.com");
    }
}
