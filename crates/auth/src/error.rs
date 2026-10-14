//! Typed auth errors the UI can translate.

use nanomgg_providers::ProviderError;
use serde::Serialize;

/// Errors of the login flows and token validation. Serialized as `{ "kind": "...", ... }`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AuthError {
    /// The user refused the authorization (`access_denied`).
    #[error("sign-in was declined in the browser")]
    Denied,
    /// The device code expired before the user approved it — start again.
    #[error("the sign-in code expired before it was approved")]
    Expired,
    /// The loopback login waited longer than its timeout for the browser redirect.
    #[error("timed out waiting for the browser to finish signing in")]
    TimedOut,
    #[error("sign-in was cancelled")]
    Cancelled,
    /// The instance/app does not offer the device flow (404 or `device_flow_disabled`):
    /// fall back to PKCE or a PAT.
    #[error("this server or OAuth app does not support the device flow: {message}")]
    #[serde(rename_all = "camelCase")]
    DeviceFlowUnsupported { message: String },
    /// The browser redirect carried a missing/foreign `state` (possible CSRF) — nothing was
    /// exchanged.
    #[error("the sign-in response did not match this login attempt")]
    StateMismatch,
    /// The refresh token was rejected (`invalid_grant`): the user must sign in again.
    #[error("the session expired, please sign in again: {message}")]
    #[serde(rename_all = "camelCase")]
    RefreshRejected { message: String },
    /// The token exists but is revoked or inactive (GitLab `/personal_access_tokens/self`).
    #[error("the token is revoked or expired")]
    TokenRevoked,
    /// Any other OAuth error response (`invalid_client`, `unsupported_grant_type`, ...).
    #[error("OAuth error `{error}`")]
    #[serde(rename_all = "camelCase")]
    OAuth {
        error: String,
        description: Option<String>,
    },
    /// Could not bind or serve the loopback redirect listener.
    #[error("local sign-in listener failed: {message}")]
    #[serde(rename_all = "camelCase")]
    Loopback { message: String },
    #[error("invalid sign-in configuration: {message}")]
    #[serde(rename_all = "camelCase")]
    InvalidConfig { message: String },
    #[error("internal error: {message}")]
    #[serde(rename_all = "camelCase")]
    Internal { message: String },
    /// HTTP/API failure (network, rate limit, 401/403, ...), see [`ProviderError`].
    #[error(transparent)]
    #[serde(rename_all = "camelCase")]
    Provider {
        #[from]
        error: ProviderError,
    },
}

impl AuthError {
    /// Stable machine-readable code, same as serde `kind`.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Denied => "denied",
            Self::Expired => "expired",
            Self::TimedOut => "timedOut",
            Self::Cancelled => "cancelled",
            Self::DeviceFlowUnsupported { .. } => "deviceFlowUnsupported",
            Self::StateMismatch => "stateMismatch",
            Self::RefreshRejected { .. } => "refreshRejected",
            Self::TokenRevoked => "tokenRevoked",
            Self::OAuth { .. } => "oAuth",
            Self::Loopback { .. } => "loopback",
            Self::InvalidConfig { .. } => "invalidConfig",
            Self::Internal { .. } => "internal",
            Self::Provider { .. } => "provider",
        }
    }
}

impl From<reqwest::Error> for AuthError {
    fn from(err: reqwest::Error) -> Self {
        Self::Provider {
            error: ProviderError::from(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_with_kind() {
        assert_eq!(
            serde_json::to_string(&AuthError::Denied).unwrap(),
            r#"{"kind":"denied"}"#
        );
        let e = AuthError::from(ProviderError::Timeout);
        assert_eq!(
            serde_json::to_string(&e).unwrap(),
            r#"{"kind":"provider","error":{"kind":"timeout"}}"#
        );
        assert_eq!(e.code(), "provider");
        let o = AuthError::OAuth {
            error: "invalid_client".into(),
            description: Some("bad id".into()),
        };
        assert_eq!(o.to_string(), "OAuth error `invalid_client`");
    }
}
