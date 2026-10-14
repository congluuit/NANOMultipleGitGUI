//! Decides whether a credential request may receive an account's token.

/// Lower-cases the host and strips the port when it is the protocol default.
pub fn normalize_host(protocol: &str, host: &str) -> String {
    let host = host.trim().to_ascii_lowercase();
    let default_port = match protocol {
        "https" => Some(":443"),
        "http" => Some(":80"),
        _ => None,
    };
    match default_port {
        Some(port) if host.ends_with(port) => host[..host.len() - port.len()].to_owned(),
        _ => host,
    }
}

fn hostname(host: &str) -> &str {
    if let Some(rest) = host.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest);
    }
    host.rsplit_once(':').map_or(host, |(name, _)| name)
}

pub fn is_loopback(host: &str) -> bool {
    matches!(hostname(host), "localhost" | "127.0.0.1" | "::1")
}

/// Why a request was refused; the text is sent back to the helper and may be shown to users.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostCheck {
    Allowed,
    Refused(String),
}

/// A token is released only to the exact host of the account's provider, and only over
/// HTTPS (plain HTTP is accepted for loopback test servers).
pub fn check(account_host: &str, protocol: &str, requested_host: &str) -> HostCheck {
    let protocol = protocol.to_ascii_lowercase();
    match protocol.as_str() {
        "https" => {}
        "http" if is_loopback(requested_host) => {}
        other => {
            return HostCheck::Refused(format!(
                "refusing to send credentials over `{other}` to {requested_host}"
            ));
        }
    }
    let expected = normalize_host("https", account_host);
    let actual = normalize_host(&protocol, requested_host);
    if expected == actual {
        HostCheck::Allowed
    } else {
        HostCheck::Refused(format!(
            "host `{actual}` does not belong to this account (expected `{expected}`)"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_host_is_allowed() {
        assert_eq!(
            check("github.com", "https", "github.com"),
            HostCheck::Allowed
        );
        assert_eq!(
            check("GitHub.com", "https", "github.COM"),
            HostCheck::Allowed
        );
        assert_eq!(
            check("github.com", "https", "github.com:443"),
            HostCheck::Allowed
        );
        assert_eq!(
            check("gitlab.acme.dev:8443", "https", "gitlab.acme.dev:8443"),
            HostCheck::Allowed
        );
    }

    #[test]
    fn other_hosts_are_refused() {
        for host in [
            "evil.example.com",
            "github.com.evil.example",
            "api.github.com",
            "lfs.thirdparty.io",
            "gitlab.acme.dev:9443",
        ] {
            assert!(
                matches!(check("github.com", "https", host), HostCheck::Refused(_)),
                "{host}"
            );
        }
    }

    #[test]
    fn plain_http_only_for_loopback() {
        assert!(matches!(
            check("github.com", "http", "github.com"),
            HostCheck::Refused(_)
        ));
        assert_eq!(
            check("127.0.0.1:8080", "http", "127.0.0.1:8080"),
            HostCheck::Allowed
        );
        assert!(matches!(
            check("127.0.0.1:8080", "http", "127.0.0.1:9090"),
            HostCheck::Refused(_)
        ));
        assert!(matches!(
            check("github.com", "ssh", "github.com"),
            HostCheck::Refused(_)
        ));
    }

    #[test]
    fn loopback_detection() {
        assert!(is_loopback("localhost:3000"));
        assert!(is_loopback("127.0.0.1"));
        assert!(is_loopback("[::1]:8080"));
        assert!(!is_loopback("127.0.0.1.evil.com"));
        assert!(!is_loopback("github.com"));
    }
}
