//! Token validation: who owns the token, which scopes it has and when it expires.

use nanomgg_providers::{
    ApiClient, GithubClient, GitlabClient, Provider, ProviderEndpoint, ProviderError,
    ProviderFamily, ProviderUser, SecretToken, now_unix, parse_timestamp,
};
use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::AuthError;

/// What kind of credential a token is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum TokenKind {
    /// `ghp_…` or legacy 40-hex token with `X-OAuth-Scopes`.
    GithubClassicPat,
    /// `github_pat_…` (no scope header; permissions are per repository).
    GithubFineGrainedPat,
    /// `gho_…` (OAuth app, e.g. device flow).
    GithubOauth,
    /// `ghu_…` / `ghs_…` (GitHub App user/installation token).
    GithubApp,
    /// GitLab personal, project or group access token (`glpat-…`).
    GitlabAccessToken,
    /// GitLab OAuth access token.
    GitlabOauth,
    Unknown,
}

/// Result of [`validate_token`]; safe to serialize (no secret).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TokenInfo {
    /// Username, display name, avatar URL, ...
    pub user: ProviderUser,
    /// `None` when the provider does not expose scopes for this token (fine-grained PAT).
    pub scopes: Option<Vec<String>>,
    /// Unix seconds; `None` = no expiry (or unknown).
    pub expires_at: Option<i64>,
    pub token_kind: TokenKind,
    /// GitLab token name when available.
    pub token_name: Option<String>,
}

/// Validates a PAT or OAuth token against the provider and returns its owner + metadata.
///
/// - GitHub: `GET /user`, scopes from `X-OAuth-Scopes`, expiry from
///   `github-authentication-token-expiration`, kind from the token prefix.
/// - GitLab: `GET /personal_access_tokens/self` (scopes, expiry, revoked/active); for OAuth
///   tokens or old versions `GET /oauth/token/info`; finally `GET /user`.
///
/// An invalid token yields `AuthError::Provider { error: ProviderError::Unauthorized }`.
pub async fn validate_token(
    http: &reqwest::Client,
    endpoint: &ProviderEndpoint,
    token: &SecretToken,
) -> Result<TokenInfo, AuthError> {
    if token.is_empty() {
        return Err(AuthError::InvalidConfig {
            message: "empty token".into(),
        });
    }
    let api = ApiClient::new(http.clone(), endpoint.clone(), token.clone());
    match endpoint.kind().family() {
        ProviderFamily::Github => validate_github(api, token).await,
        ProviderFamily::Gitlab => validate_gitlab(api).await,
    }
}

fn github_kind(token: &str, has_scope_header: bool) -> TokenKind {
    let t = token.trim();
    if t.starts_with("github_pat_") {
        TokenKind::GithubFineGrainedPat
    } else if t.starts_with("ghp_") {
        TokenKind::GithubClassicPat
    } else if t.starts_with("gho_") {
        TokenKind::GithubOauth
    } else if t.starts_with("ghu_") || t.starts_with("ghs_") {
        TokenKind::GithubApp
    } else if has_scope_header {
        TokenKind::GithubClassicPat
    } else {
        TokenKind::Unknown
    }
}

async fn validate_github(api: ApiClient, token: &SecretToken) -> Result<TokenInfo, AuthError> {
    let client = GithubClient::from_api(api);
    let (user, headers) = client.current_user_with_token_info().await?;
    let token_kind = github_kind(token.expose(), headers.scopes.is_some());
    Ok(TokenInfo {
        user,
        // Fine-grained tokens have no scope list even if a proxy adds an empty header.
        scopes: if token_kind == TokenKind::GithubFineGrainedPat {
            None
        } else {
            headers.scopes
        },
        expires_at: headers.expires_at,
        token_kind,
        token_name: None,
    })
}

#[derive(Deserialize)]
struct RawOauthTokenInfo {
    #[serde(default)]
    scope: Option<serde_json::Value>,
    #[serde(default)]
    scopes: Option<serde_json::Value>,
    expires_in: Option<i64>,
}

fn scopes_from_value(v: &serde_json::Value) -> Vec<String> {
    match v {
        serde_json::Value::Array(a) => a
            .iter()
            .filter_map(|s| s.as_str().map(str::to_owned))
            .collect(),
        serde_json::Value::String(s) => nanomgg_providers::split_scopes(s),
        _ => Vec::new(),
    }
}

async fn validate_gitlab(api: ApiClient) -> Result<TokenInfo, AuthError> {
    let client = GitlabClient::from_api(api.clone());
    let mut scopes = None;
    let mut expires_at = None;
    let mut token_name = None;
    let mut token_kind = TokenKind::Unknown;

    match client.personal_access_token_self().await {
        Ok(pat) => {
            if pat.revoked || !pat.active {
                return Err(AuthError::TokenRevoked);
            }
            scopes = Some(pat.scopes);
            expires_at = pat
                .expires_at
                .as_deref()
                .and_then(parse_timestamp)
                // GitLab tokens expire at the end of `expires_at` (UTC).
                .map(|day_start| day_start + 86_399);
            token_name = Some(pat.name);
            token_kind = TokenKind::GitlabAccessToken;
        }
        Err(
            ProviderError::NotFound { .. }
            | ProviderError::Unauthorized { .. }
            | ProviderError::Forbidden { .. },
        ) => {
            // OAuth token, or GitLab < 15.5. `/oauth/token/info` lives on the web base URL.
            let url = api.endpoint().web("oauth/token/info")?;
            if let Ok(info) = api
                .send::<RawOauthTokenInfo, ()>(Method::GET, url, None)
                .await
            {
                scopes = info
                    .body
                    .scope
                    .as_ref()
                    .or(info.body.scopes.as_ref())
                    .map(scopes_from_value);
                expires_at = info
                    .body
                    .expires_in
                    .filter(|e| *e > 0)
                    .map(|e| now_unix() + e);
                token_kind = TokenKind::GitlabOauth;
            }
        }
        Err(other) => return Err(other.into()),
    }

    let user = client.current_user().await?;
    Ok(TokenInfo {
        user,
        scopes,
        expires_at,
        token_kind,
        token_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_prefixes() {
        assert_eq!(
            github_kind("github_pat_11AA", false),
            TokenKind::GithubFineGrainedPat
        );
        assert_eq!(github_kind("ghp_x", true), TokenKind::GithubClassicPat);
        assert_eq!(github_kind("gho_x", true), TokenKind::GithubOauth);
        assert_eq!(github_kind("ghu_x", false), TokenKind::GithubApp);
        assert_eq!(github_kind("0123abcd", true), TokenKind::GithubClassicPat);
        assert_eq!(github_kind("0123abcd", false), TokenKind::Unknown);
    }
}
