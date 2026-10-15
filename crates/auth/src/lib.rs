//! OAuth device flow / PKCE, personal access tokens and token lifecycle (PLAN 4.2).
//!
//! - [`device`]: RFC 8628 Device Authorization Grant — GitHub.com / GitHub Enterprise and GitLab
//!   instances that enable it.
//! - [`pkce`]: GitLab Authorization Code + PKCE (S256) with a loopback redirect
//!   `http://127.0.0.1:<random port>/callback`.
//! - [`token`]: the [`TokenSet`] returned by every flow and GitLab token refresh.
//! - [`validate`]: PAT/OAuth token validation → user, scopes, expiry, token kind.
//! - [`scopes`]: default scopes, per-feature scope requirements and token health.
//!
//! OAuth apps are public clients: only a client id (a configuration input, never hardcoded
//! here) and no client secret (PLAN 9). All secrets are [`SecretToken`]s (zeroized, redacted in
//! `Debug`). HTTP goes through the shared client from [`nanomgg_providers::build_http_client`].
//!
//! # Driving a login from the app
//! Device flow: `request_device_code` → show [`DevicePrompt`] (code + URL) → `poll_device_token`
//! with a [`CancellationToken`] the UI can trigger → [`validate_token`] → store secrets in the
//! keychain. PKCE: `LoopbackLogin::start` → open `authorize_url()` in the browser →
//! `finish(&http, &cancel)`.

pub mod device;
pub mod error;
pub mod pkce;
pub mod scopes;
pub mod token;
pub mod validate;

pub use device::{
    DeviceAuthorization, DeviceFlowConfig, DevicePrompt, poll_device_token, request_device_code,
};
pub use error::AuthError;
pub use nanomgg_providers::{
    CancellationToken, ProviderEndpoint, ProviderFamily, ProviderKind, SecretToken,
};
pub use pkce::{LoopbackLogin, PkceConfig};
pub use scopes::{
    EXPIRY_WARNING_DAYS, Feature, MissingScope, TokenHealth, default_scopes, expires_within,
    required_scopes, scope_satisfied, token_health,
};
pub use token::{
    TokenSet, TokenSummary, needs_refresh, refresh_access_token, refresh_github_token,
    refresh_gitlab_token,
};
pub use validate::{TokenInfo, TokenKind, validate_token};

/// `bytes` random bytes from the OS, base64url without padding.
pub(crate) fn random_urlsafe(bytes: usize) -> Result<String, AuthError> {
    use base64::Engine;
    let mut buf = zeroize::Zeroizing::new(vec![0u8; bytes]);
    getrandom::fill(&mut buf).map_err(|e| AuthError::Internal {
        message: format!("OS random generator failed: {e}"),
    })?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&*buf))
}
