//! Token sets, OAuth token-endpoint calls and refresh.

use nanomgg_providers::{ProviderEndpoint, ProviderError, SecretToken, now_unix, split_scopes};
use reqwest::StatusCode;
use reqwest::header::ACCEPT;
use serde::{Deserialize, Serialize};
use url::Url;
use zeroize::Zeroizing;

use crate::AuthError;

/// Tokens returned by a login or refresh. Store `access_token` / `refresh_token` in the keychain
/// (`nanomgg:<account_id>:<kind>`, PLAN 4.1); keep only [`TokenSummary`] in SQLite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenSet {
    pub access_token: SecretToken,
    /// GitLab OAuth (and GitHub Apps with expiring tokens). GitHub OAuth apps: `None`.
    pub refresh_token: Option<SecretToken>,
    /// Usually `bearer`.
    pub token_type: String,
    /// Scopes actually granted (may differ from those requested).
    pub scopes: Vec<String>,
    /// Unix seconds; `None` = does not expire (GitHub OAuth app tokens).
    pub expires_at: Option<i64>,
    pub refresh_token_expires_at: Option<i64>,
}

/// Non-secret view of a [`TokenSet`] for the UI / database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TokenSummary {
    pub scopes: Vec<String>,
    pub expires_at: Option<i64>,
    pub has_refresh_token: bool,
}

impl TokenSet {
    pub fn summary(&self) -> TokenSummary {
        TokenSummary {
            scopes: self.scopes.clone(),
            expires_at: self.expires_at,
            has_refresh_token: self.refresh_token.is_some(),
        }
    }
}

/// True when the access token expires within `margin_secs` (refresh ahead of time; GitLab OAuth
/// tokens live ~2 h). Tokens without expiry never need a refresh.
pub fn needs_refresh(expires_at: Option<i64>, now: i64, margin_secs: i64) -> bool {
    expires_at.is_some_and(|e| e - now <= margin_secs)
}

/// What a token endpoint answered: success, or an OAuth error code.
pub(crate) enum TokenReply {
    Token(TokenSet),
    Error {
        error: String,
        description: Option<String>,
        /// Device flow `slow_down` may carry a new interval (GitHub).
        interval: Option<u64>,
    },
}

#[derive(Deserialize)]
struct RawToken {
    access_token: Option<Zeroizing<String>>,
    refresh_token: Option<Zeroizing<String>>,
    token_type: Option<String>,
    scope: Option<String>,
    expires_in: Option<i64>,
    refresh_token_expires_in: Option<i64>,
    created_at: Option<i64>,
    error: Option<String>,
    error_description: Option<String>,
    interval: Option<u64>,
}

/// POSTs `params` (form-encoded) to an OAuth endpoint and decodes the JSON answer. Both
/// GitHub (200 + `error`) and RFC style (400 + `error`) error replies become
/// [`TokenReply::Error`]; responses without JSON map to [`ProviderError`]s.
pub(crate) async fn post_token_endpoint(
    http: &reqwest::Client,
    url: &Url,
    params: &[(&str, &str)],
) -> Result<TokenReply, AuthError> {
    let resp = http
        .post(url.clone())
        .header(ACCEPT, "application/json")
        .form(params)
        .send()
        .await?;
    let status = resp.status();
    let text = Zeroizing::new(resp.text().await?);
    let raw: Option<RawToken> = serde_json::from_str(&text).ok();
    match raw {
        Some(raw) if raw.error.is_some() => Ok(TokenReply::Error {
            error: raw.error.unwrap_or_default(),
            description: raw.error_description,
            interval: raw.interval,
        }),
        Some(raw) if status.is_success() && raw.access_token.is_some() => {
            Ok(TokenReply::Token(raw.into_token_set()))
        }
        _ => Err(status_error(status)),
    }
}

/// Error for a token-endpoint reply without a usable JSON body.
pub(crate) fn status_error(status: StatusCode) -> AuthError {
    let message = format!(
        "unexpected reply from the sign-in server ({})",
        status.as_u16()
    );
    AuthError::Provider {
        error: match status.as_u16() {
            404 => ProviderError::NotFound {
                message,
                hint: None,
            },
            401 => ProviderError::Unauthorized { message },
            429 => ProviderError::RateLimited {
                reset_at: None,
                retry_after_secs: None,
            },
            s @ 500..=599 => ProviderError::Server { status: s, message },
            s if (200..300).contains(&s) => ProviderError::InvalidResponse { message },
            s => ProviderError::UnexpectedStatus { status: s, message },
        },
    }
}

impl RawToken {
    fn into_token_set(self) -> TokenSet {
        let now = now_unix();
        // GitLab returns `created_at`; prefer it so clock skew does not shorten the lifetime.
        let base = self
            .created_at
            .filter(|c| (c - now).abs() < 300)
            .unwrap_or(now);
        TokenSet {
            access_token: self
                .access_token
                .map(SecretToken::from)
                .unwrap_or_else(|| SecretToken::new("")),
            refresh_token: self
                .refresh_token
                .filter(|r| !r.is_empty())
                .map(SecretToken::from),
            token_type: self.token_type.unwrap_or_else(|| "bearer".to_owned()),
            scopes: self.scope.as_deref().map(split_scopes).unwrap_or_default(),
            expires_at: self.expires_in.filter(|e| *e > 0).map(|e| base + e),
            refresh_token_expires_at: self
                .refresh_token_expires_in
                .filter(|e| *e > 0)
                .map(|e| base + e),
        }
    }
}

/// Exchanges a refresh token (`grant_type=refresh_token`, public client: no secret).
/// GitLab rotates refresh tokens: always persist the returned `refresh_token`.
/// `invalid_grant` → [`AuthError::RefreshRejected`] (sign in again).
pub async fn refresh_access_token(
    http: &reqwest::Client,
    token_url: &Url,
    client_id: &str,
    refresh_token: &SecretToken,
) -> Result<TokenSet, AuthError> {
    if client_id.trim().is_empty() {
        return Err(AuthError::InvalidConfig {
            message: "missing OAuth client id".into(),
        });
    }
    let params = [
        ("grant_type", "refresh_token"),
        ("client_id", client_id.trim()),
        ("refresh_token", refresh_token.expose()),
    ];
    match post_token_endpoint(http, token_url, &params).await? {
        TokenReply::Token(mut set) => {
            if set.refresh_token.is_none() {
                // Non-rotating servers: keep using the old refresh token.
                set.refresh_token = Some(refresh_token.clone());
            }
            Ok(set)
        }
        TokenReply::Error {
            error, description, ..
        } if error == "invalid_grant" => Err(AuthError::RefreshRejected {
            message: description.unwrap_or(error),
        }),
        TokenReply::Error {
            error, description, ..
        } => Err(AuthError::OAuth { error, description }),
    }
}

/// [`refresh_access_token`] against `<gitlab>/oauth/token`.
pub async fn refresh_gitlab_token(
    http: &reqwest::Client,
    endpoint: &ProviderEndpoint,
    client_id: &str,
    refresh_token: &SecretToken,
) -> Result<TokenSet, AuthError> {
    let url = endpoint.web("oauth/token")?;
    refresh_access_token(http, &url, client_id, refresh_token).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_window() {
        assert!(!needs_refresh(None, 100, 300));
        assert!(needs_refresh(Some(350), 100, 300));
        assert!(!needs_refresh(Some(1000), 100, 300));
    }

    #[test]
    fn token_set_debug_is_redacted() {
        let raw: RawToken = serde_json::from_str(
            r#"{"access_token":"gloas-abcdefghijklmnopqrstuvwxyz","refresh_token":"r123","token_type":"Bearer","scope":"api read_user","expires_in":7200}"#,
        )
        .unwrap();
        let set = raw.into_token_set();
        let dbg = format!("{set:?}");
        assert!(!dbg.contains("gloas-") && !dbg.contains("r123"), "{dbg}");
        assert_eq!(set.scopes, ["api", "read_user"]);
        assert!(set.expires_at.unwrap() > now_unix() + 7000);
        assert!(set.summary().has_refresh_token);
    }
}
