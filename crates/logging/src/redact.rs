use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;

pub const REDACTED: &str = "[REDACTED]";

struct Rule {
    re: Regex,
    replacement: &'static str,
}

fn rule(pattern: &str, replacement: &'static str) -> Rule {
    Rule {
        re: Regex::new(pattern).expect("valid redaction regex"),
        replacement,
    }
}

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    vec![
        // `Authorization: Bearer xyz` / `authorization=...` — drop everything after the separator.
        rule(
            r"(?im)(\bauthorization\s*[:=]\s*)[^\r\n]+",
            "${1}[REDACTED]",
        ),
        // Bare auth schemes, e.g. `Bearer eyJ...` or `token ghp_...` in error messages.
        rule(
            r"(?i)\b(bearer|basic|token)(\s+)[A-Za-z0-9._~+/\-]{8,}=*",
            "${1}${2}[REDACTED]",
        ),
        // git credential protocol lines (`password=...`) and similar key/value dumps.
        rule(
            r"(?im)^(\s*(?:password|passphrase|oauth_refresh_token)\s*=\s*)[^\r\n]*",
            "${1}[REDACTED]",
        ),
        // Credentials embedded in URLs: scheme://user:secret@host
        rule(
            r"(?i)\b([a-z][a-z0-9+.\-]*://[^/\s:@]+):[^@\s/]+@",
            "${1}:[REDACTED]@",
        ),
        // GitHub tokens: classic/OAuth/user-to-server/server-to-server/refresh, fine-grained PATs.
        rule(r"\bgh[pousr]_[A-Za-z0-9]{20,255}\b", "[REDACTED]"),
        rule(r"\bgithub_pat_[A-Za-z0-9_]{20,255}\b", "[REDACTED]"),
        // GitLab tokens: glpat-, gloas-, gldt-, glrt-, glcbt-, glptt-, glft-, glimt-, glsoat-...
        rule(r"\bgl[a-z]{2,8}-[A-Za-z0-9_\-]{20,}", "[REDACTED]"),
    ]
});

/// Removes access tokens, passwords and `Authorization` values from `input`.
/// Returns the input unchanged (borrowed) when nothing matched.
pub fn redact(input: &str) -> Cow<'_, str> {
    let mut out = Cow::Borrowed(input);
    for Rule { re, replacement } in RULES.iter() {
        if re.is_match(&out) {
            out = Cow::Owned(re.replace_all(&out, *replacement).into_owned());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GHP: &str = "ghp_1234567890abcdefghijABCDEFGHIJ123456";

    #[test]
    fn leaves_plain_text_untouched_and_borrowed() {
        let s = "fetch origin main: 12 objects, 3.4 MiB, token count = 5";
        assert!(matches!(redact(s), Cow::Borrowed(_)));
    }

    #[test]
    fn github_tokens() {
        for token in [
            GHP,
            "gho_abcdefghijklmnopqrstuvwxyz0123456789",
            "ghs_abcdefghijklmnopqrstuvwxyz0123456789",
            "github_pat_11ABCDEFG0123456789_abcdefghijklmnopqrstuvwxyzABCDEFGHIJ",
        ] {
            let input = format!("using {token} for push");
            let out = redact(&input);
            assert_eq!(out, "using [REDACTED] for push", "{token}");
        }
    }

    #[test]
    fn gitlab_tokens() {
        for token in [
            "glpat-abcdefghijklmnopqrst",
            "glpat-AbCd_EfGh-IjKlMnOpQrSt12",
            "gloas-0123456789abcdefghijklmnop",
            "gldt-abcdefghijklmnopqrstuv",
        ] {
            assert_eq!(redact(&format!("[{token}]")), "[[REDACTED]]", "{token}");
        }
    }

    #[test]
    fn authorization_headers() {
        assert_eq!(
            redact("> Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.payload.sig\n> Accept: */*"),
            "> Authorization: [REDACTED]\n> Accept: */*"
        );
        assert_eq!(
            redact("authorization=Basic dXNlcjpwYXNz"),
            "authorization=[REDACTED]"
        );
    }

    #[test]
    fn bare_auth_schemes() {
        assert_eq!(
            redact("request failed with Bearer abcdefgh12345678"),
            "request failed with Bearer [REDACTED]"
        );
        assert_eq!(
            redact("header: token 0123456789abcdef"),
            "header: token [REDACTED]"
        );
    }

    #[test]
    fn credential_protocol_password_line() {
        let msg = "protocol=https\nhost=github.com\nusername=alice\npassword=s3cr3t-value\n";
        assert_eq!(
            redact(msg),
            "protocol=https\nhost=github.com\nusername=alice\npassword=[REDACTED]\n"
        );
    }

    #[test]
    fn url_userinfo() {
        assert_eq!(
            redact("cloning https://x-access-token:abc123def@github.com/acme/api.git"),
            "cloning https://x-access-token:[REDACTED]@github.com/acme/api.git"
        );
        assert_eq!(
            redact(&format!("remote https://{GHP}@github.com/a/b")),
            "remote https://[REDACTED]@github.com/a/b"
        );
        // Plain URLs and SSH remotes must survive.
        let ssh = "git@github.com:acme/api.git and https://gitlab.com/acme/api";
        assert_eq!(redact(ssh), ssh);
    }

    #[test]
    fn multiple_secrets_in_one_line() {
        let input = format!("a={GHP} b=glpat-abcdefghijklmnopqrst c=https://u:p@h/");
        let out = redact(&input);
        assert_eq!(out, "a=[REDACTED] b=[REDACTED] c=https://u:[REDACTED]@h/");
    }
}
