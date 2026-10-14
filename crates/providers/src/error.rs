//! Typed provider errors the UI can translate, plus HTTP response → error mapping.

use reqwest::StatusCode;
use reqwest::header::HeaderMap;
use serde::Serialize;

use crate::endpoint::ProviderFamily;

/// Why access was refused, when the provider tells us enough to suggest a fix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AccessHint {
    /// The GitHub organization enabled "OAuth App access restrictions" and has not approved our
    /// OAuth app (PLAN 4.2). Suggest a fine-grained PAT or asking an org owner to approve it.
    #[serde(rename_all = "camelCase")]
    OauthAppRestricted { organization: Option<String> },
    /// The GitHub organization enforces SAML SSO and the token is not authorized for it yet.
    #[serde(rename_all = "camelCase")]
    SamlSsoRequired { url: Option<String> },
    /// The token lacks a scope; `required` lists scopes of which any one would do.
    #[serde(rename_all = "camelCase")]
    MissingScopes {
        required: Vec<String>,
        granted: Option<Vec<String>>,
    },
    /// A fine-grained / project token does not carry the permission for this resource
    /// (GitHub: "Resource not accessible by personal access token").
    TokenPermissionMissing,
}

/// Errors from provider API calls. Serialized as `{ "kind": "...", ...fields }` so the
/// frontend can pick a translated message per `kind`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProviderError {
    /// 401: token invalid, expired or revoked — sign in again / replace the PAT.
    #[error("the provider rejected the token (invalid, expired or revoked): {message}")]
    #[serde(rename_all = "camelCase")]
    Unauthorized { message: String },
    /// 403 that is not a rate limit.
    #[error("access denied: {message}")]
    #[serde(rename_all = "camelCase")]
    Forbidden {
        message: String,
        hint: Option<AccessHint>,
    },
    /// HTTP 404: GitHub and GitLab also answer 404 for private resources the token cannot see, so a
    /// hint may be attached (missing scope, OAuth app restriction).
    #[error("not found: {message}")]
    #[serde(rename_all = "camelCase")]
    NotFound {
        message: String,
        hint: Option<AccessHint>,
    },
    /// Primary or secondary rate limit. `reset_at` is a Unix timestamp (seconds).
    #[error("rate limit exceeded")]
    #[serde(rename_all = "camelCase")]
    RateLimited {
        reset_at: Option<i64>,
        retry_after_secs: Option<u64>,
    },
    /// The object already exists (e.g. SSH key already registered).
    #[error("already exists: {message}")]
    #[serde(rename_all = "camelCase")]
    AlreadyExists { message: String },
    /// 400 / 409 / 422 validation failures.
    #[error("request rejected ({status}): {message}")]
    #[serde(rename_all = "camelCase")]
    Rejected { status: u16, message: String },
    /// HTTP 5xx.
    #[error("provider server error ({status}): {message}")]
    #[serde(rename_all = "camelCase")]
    Server { status: u16, message: String },
    #[error("unexpected HTTP status {status}: {message}")]
    #[serde(rename_all = "camelCase")]
    UnexpectedStatus { status: u16, message: String },
    #[error("the request timed out")]
    Timeout,
    /// DNS, connect, TLS or connection reset.
    #[error("network error: {message}")]
    #[serde(rename_all = "camelCase")]
    Network { message: String },
    /// Body was not the JSON we expected.
    #[error("invalid response from provider: {message}")]
    #[serde(rename_all = "camelCase")]
    InvalidResponse { message: String },
    #[error("invalid input: {message}")]
    #[serde(rename_all = "camelCase")]
    InvalidInput { message: String },
    /// A remote URL points at a different host than the account's provider.
    #[error("remote host `{actual}` does not belong to provider host `{expected}`")]
    #[serde(rename_all = "camelCase")]
    HostMismatch { expected: String, actual: String },
    /// Feature not available on this server/version.
    #[error("not supported by this server: {message}")]
    #[serde(rename_all = "camelCase")]
    Unsupported { message: String },
    #[error("cancelled")]
    Cancelled,
}

impl ProviderError {
    /// Stable machine-readable code (`unauthorized`, `rateLimited`, ...), same as serde `kind`.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unauthorized { .. } => "unauthorized",
            Self::Forbidden { .. } => "forbidden",
            Self::NotFound { .. } => "notFound",
            Self::RateLimited { .. } => "rateLimited",
            Self::AlreadyExists { .. } => "alreadyExists",
            Self::Rejected { .. } => "rejected",
            Self::Server { .. } => "server",
            Self::UnexpectedStatus { .. } => "unexpectedStatus",
            Self::Timeout => "timeout",
            Self::Network { .. } => "network",
            Self::InvalidResponse { .. } => "invalidResponse",
            Self::InvalidInput { .. } => "invalidInput",
            Self::HostMismatch { .. } => "hostMismatch",
            Self::Unsupported { .. } => "unsupported",
            Self::Cancelled => "cancelled",
        }
    }

    /// The access hint, if any.
    pub fn hint(&self) -> Option<&AccessHint> {
        match self {
            Self::Forbidden { hint, .. } | Self::NotFound { hint, .. } => hint.as_ref(),
            _ => None,
        }
    }

    /// True when the UI should suggest a (fine-grained) PAT instead of the OAuth login.
    pub fn suggests_fine_grained_pat(&self) -> bool {
        matches!(self.hint(), Some(AccessHint::OauthAppRestricted { .. }))
    }

    /// Worth retrying automatically / offering a [Retry] button.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::Timeout | Self::Network { .. } | Self::Server { .. } | Self::RateLimited { .. }
        )
    }

    pub(crate) fn invalid_response(e: impl std::fmt::Display) -> Self {
        Self::InvalidResponse {
            message: truncate(&e.to_string()),
        }
    }
}

impl From<reqwest::Error> for ProviderError {
    fn from(err: reqwest::Error) -> Self {
        // `without_url`: URLs never carry secrets here, but keep error text minimal anyway.
        let is_timeout = err.is_timeout();
        let is_decode = err.is_decode() || err.is_body();
        let err = err.without_url();
        let mut message = err.to_string();
        // Append the innermost cause (e.g. "connection refused", "invalid peer certificate").
        let mut source = std::error::Error::source(&err);
        let mut root = None;
        while let Some(s) = source {
            root = Some(s.to_string());
            source = s.source();
        }
        if let Some(root) = root.filter(|r| !message.contains(r.as_str())) {
            message = format!("{message}: {root}");
        }
        if is_timeout {
            Self::Timeout
        } else if is_decode {
            Self::invalid_response(message)
        } else {
            Self::Network {
                message: truncate(&message),
            }
        }
    }
}

/// Maps a non-success response to a typed error. `body` is the (possibly empty) response text.
pub(crate) fn map_error_response(
    family: ProviderFamily,
    status: StatusCode,
    headers: &HeaderMap,
    body: &str,
) -> ProviderError {
    let json: Option<serde_json::Value> = serde_json::from_str(body).ok();
    let message = json
        .as_ref()
        .map(extract_message)
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| {
            status
                .canonical_reason()
                .unwrap_or("request failed")
                .to_owned()
        });
    let message = truncate(&message);
    let code = status.as_u16();

    if let Some(rate) = rate_limit(family, status, headers, &message) {
        return rate;
    }

    match status {
        StatusCode::UNAUTHORIZED => ProviderError::Unauthorized { message },
        StatusCode::FORBIDDEN => ProviderError::Forbidden {
            hint: access_hint(family, headers, json.as_ref(), &message),
            message,
        },
        StatusCode::NOT_FOUND => ProviderError::NotFound {
            hint: access_hint(family, headers, json.as_ref(), &message),
            message,
        },
        StatusCode::BAD_REQUEST | StatusCode::CONFLICT | StatusCode::UNPROCESSABLE_ENTITY => {
            if looks_like_already_exists(&message) {
                ProviderError::AlreadyExists { message }
            } else {
                ProviderError::Rejected {
                    status: code,
                    message,
                }
            }
        }
        s if s.is_server_error() => ProviderError::Server {
            status: code,
            message,
        },
        _ => ProviderError::UnexpectedStatus {
            status: code,
            message,
        },
    }
}

fn looks_like_already_exists(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    m.contains("already in use") || m.contains("already been taken") || m.contains("already exists")
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
}

fn rate_limit(
    family: ProviderFamily,
    status: StatusCode,
    headers: &HeaderMap,
    message: &str,
) -> Option<ProviderError> {
    if status != StatusCode::FORBIDDEN && status != StatusCode::TOO_MANY_REQUESTS {
        return None;
    }
    let retry_after_secs = header(headers, "retry-after").and_then(|v| v.parse::<u64>().ok());
    let (remaining, reset) = match family {
        ProviderFamily::Github => ("x-ratelimit-remaining", "x-ratelimit-reset"),
        ProviderFamily::Gitlab => ("ratelimit-remaining", "ratelimit-reset"),
    };
    let reset_at = header(headers, reset).and_then(|v| v.parse::<i64>().ok());
    let exhausted = header(headers, remaining) == Some("0");
    let mentions = message.to_ascii_lowercase().contains("rate limit");
    if status == StatusCode::TOO_MANY_REQUESTS
        || exhausted
        || retry_after_secs.is_some()
        || mentions
    {
        Some(ProviderError::RateLimited {
            reset_at,
            retry_after_secs,
        })
    } else {
        None
    }
}

/// Splits a scope list separated by commas and/or whitespace (`repo, read:user` / `api read_user`).
pub fn split_scopes(raw: &str) -> Vec<String> {
    raw.split([',', ' ', '\t'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn access_hint(
    family: ProviderFamily,
    headers: &HeaderMap,
    json: Option<&serde_json::Value>,
    message: &str,
) -> Option<AccessHint> {
    match family {
        ProviderFamily::Github => {
            if let Some(sso) = header(headers, "x-github-sso") {
                let url = sso
                    .split(';')
                    .filter_map(|p| p.trim().strip_prefix("url="))
                    .next()
                    .map(str::to_owned);
                return Some(AccessHint::SamlSsoRequired { url });
            }
            if message.contains("OAuth App access restrictions") {
                // "...the `acme` organization has enabled OAuth App access restrictions..."
                let organization = message
                    .split_once("the `")
                    .and_then(|(_, rest)| rest.split_once('`'))
                    .map(|(org, _)| org.to_owned());
                return Some(AccessHint::OauthAppRestricted { organization });
            }
            if message.contains("Resource not accessible by personal access token")
                || message.contains("Resource not accessible by integration")
            {
                return Some(AccessHint::TokenPermissionMissing);
            }
            let accepted = header(headers, "x-accepted-oauth-scopes").map(split_scopes)?;
            let granted = header(headers, "x-oauth-scopes").map(split_scopes);
            let has_any = granted
                .as_ref()
                .is_some_and(|g| accepted.iter().any(|a| g.contains(a)));
            (!accepted.is_empty() && granted.is_some() && !has_any).then_some(
                AccessHint::MissingScopes {
                    required: accepted,
                    granted,
                },
            )
        }
        ProviderFamily::Gitlab => {
            let body_scope = json.and_then(|j| {
                (j.get("error")?.as_str()? == "insufficient_scope")
                    .then(|| j.get("scope").and_then(|s| s.as_str()).unwrap_or(""))
                    .map(split_scopes)
            });
            let header_scope = header(headers, "www-authenticate")
                .filter(|v| v.contains("insufficient_scope"))
                .map(|v| {
                    v.split_once("scope=\"")
                        .and_then(|(_, rest)| rest.split_once('"'))
                        .map(|(s, _)| split_scopes(s))
                        .unwrap_or_default()
                });
            body_scope
                .or(header_scope)
                .map(|required| AccessHint::MissingScopes {
                    required,
                    granted: None,
                })
        }
    }
}

/// Best-effort human message from GitHub/GitLab error JSON.
fn extract_message(v: &serde_json::Value) -> String {
    let mut parts = Vec::new();
    match v.get("message") {
        Some(serde_json::Value::String(s)) => parts.push(s.clone()),
        // GitLab validation: {"message": {"key": ["has already been taken"]}}
        Some(serde_json::Value::Object(map)) => {
            for (field, errs) in map {
                match errs {
                    serde_json::Value::Array(list) => {
                        for e in list.iter().filter_map(|e| e.as_str()) {
                            parts.push(format!("{field} {e}"));
                        }
                    }
                    serde_json::Value::String(e) => parts.push(format!("{field} {e}")),
                    _ => {}
                }
            }
        }
        _ => {}
    }
    // GitHub validation: {"errors": [{"message": "key is already in use"}]}
    if let Some(errors) = v.get("errors").and_then(|e| e.as_array()) {
        for e in errors {
            if let Some(m) = e.get("message").and_then(|m| m.as_str()) {
                parts.push(m.to_owned());
            } else if let Some(m) = e.as_str() {
                parts.push(m.to_owned());
            }
        }
    }
    if parts.is_empty() {
        // OAuth style: {"error": "invalid_token", "error_description": "..."}
        if let Some(d) = v.get("error_description").and_then(|d| d.as_str()) {
            parts.push(d.to_owned());
        } else if let Some(e) = v.get("error").and_then(|e| e.as_str()) {
            parts.push(e.to_owned());
        }
    }
    parts.join("; ")
}

pub(crate) fn truncate(s: &str) -> String {
    const MAX: usize = 300;
    let s = s.trim();
    if s.chars().count() <= MAX {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(MAX).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderName, HeaderValue};

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(
                HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_str(v).unwrap(),
            );
        }
        h
    }

    #[test]
    fn github_oauth_restriction_is_detected() {
        let body = r#"{"message":"Although you appear to have the correct authorization credentials, the `acme-corp` organization has enabled OAuth App access restrictions, meaning that data access to third-parties is limited."}"#;
        let e = map_error_response(
            ProviderFamily::Github,
            StatusCode::FORBIDDEN,
            &HeaderMap::new(),
            body,
        );
        assert_eq!(
            e.hint(),
            Some(&AccessHint::OauthAppRestricted {
                organization: Some("acme-corp".into())
            })
        );
        assert!(e.suggests_fine_grained_pat());
    }

    #[test]
    fn github_rate_limit_and_sso_and_scopes() {
        let e = map_error_response(
            ProviderFamily::Github,
            StatusCode::FORBIDDEN,
            &headers(&[
                ("x-ratelimit-remaining", "0"),
                ("x-ratelimit-reset", "1700000000"),
            ]),
            r#"{"message":"API rate limit exceeded"}"#,
        );
        assert_eq!(
            e,
            ProviderError::RateLimited {
                reset_at: Some(1_700_000_000),
                retry_after_secs: None
            }
        );
        let e = map_error_response(
            ProviderFamily::Github,
            StatusCode::FORBIDDEN,
            &headers(&[(
                "x-github-sso",
                "required; url=https://github.com/orgs/acme/sso?x=1",
            )]),
            r#"{"message":"Resource protected by organization SAML enforcement."}"#,
        );
        assert_eq!(
            e.hint(),
            Some(&AccessHint::SamlSsoRequired {
                url: Some("https://github.com/orgs/acme/sso?x=1".into())
            })
        );
        let e = map_error_response(
            ProviderFamily::Github,
            StatusCode::NOT_FOUND,
            &headers(&[
                ("x-accepted-oauth-scopes", "repo"),
                ("x-oauth-scopes", "read:user"),
            ]),
            r#"{"message":"Not Found"}"#,
        );
        assert!(matches!(
            e.hint(),
            Some(AccessHint::MissingScopes { required, .. }) if required == &["repo"]
        ));
    }

    #[test]
    fn gitlab_insufficient_scope_and_validation() {
        let e = map_error_response(
            ProviderFamily::Gitlab,
            StatusCode::FORBIDDEN,
            &HeaderMap::new(),
            r#"{"error":"insufficient_scope","error_description":"The request requires higher privileges than provided by the access token.","scope":"api read_api"}"#,
        );
        assert!(matches!(
            e.hint(),
            Some(AccessHint::MissingScopes { required, .. }) if required == &["api", "read_api"]
        ));
        let e = map_error_response(
            ProviderFamily::Gitlab,
            StatusCode::BAD_REQUEST,
            &HeaderMap::new(),
            r#"{"message":{"key":["has already been taken"]}}"#,
        );
        assert_eq!(
            e,
            ProviderError::AlreadyExists {
                message: "key has already been taken".into()
            }
        );
        let e = map_error_response(
            ProviderFamily::Gitlab,
            StatusCode::TOO_MANY_REQUESTS,
            &headers(&[("retry-after", "30"), ("ratelimit-reset", "1700000100")]),
            "Retry later",
        );
        assert_eq!(e.code(), "rateLimited");
        assert!(e.is_transient());
        let e = map_error_response(
            ProviderFamily::Gitlab,
            StatusCode::BAD_GATEWAY,
            &HeaderMap::new(),
            "<html>",
        );
        assert_eq!(
            e,
            ProviderError::Server {
                status: 502,
                message: "Bad Gateway".into()
            }
        );
    }

    #[test]
    fn errors_serialize_with_kind_tag() {
        let e = ProviderError::RateLimited {
            reset_at: Some(5),
            retry_after_secs: None,
        };
        assert_eq!(
            serde_json::to_string(&e).unwrap(),
            r#"{"kind":"rateLimited","resetAt":5,"retryAfterSecs":null}"#
        );
        assert_eq!(
            split_scopes("repo, read:user user:email"),
            ["repo", "read:user", "user:email"]
        );
    }
}
