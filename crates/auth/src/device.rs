//! OAuth 2.0 Device Authorization Grant (RFC 8628) for GitHub and GitLab.
//!
//! ```no_run
//! # async fn demo(http: reqwest::Client) -> Result<(), nanomgg_auth::AuthError> {
//! use nanomgg_auth::*;
//! let cfg = DeviceFlowConfig::github(&ProviderEndpoint::github(), "Iv1.0123456789abcdef", None)?;
//! let auth = request_device_code(&http, &cfg).await?;
//! println!("Open {} and enter {}", auth.prompt.verification_uri, auth.prompt.user_code);
//! let cancel = CancellationToken::new(); // UI "Cancel" button calls cancel.cancel()
//! let tokens = poll_device_token(&http, &cfg, &auth, &cancel).await?;
//! # let _ = tokens; Ok(()) }
//! ```

use std::time::{Duration, Instant};

use nanomgg_providers::{ProviderEndpoint, ProviderError, ProviderFamily, SecretToken, now_unix};
use reqwest::header::ACCEPT;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::scopes::default_scopes;
use crate::token::{TokenReply, TokenSet, post_token_endpoint, status_error};
use crate::{AuthError, ProviderKind};

const GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";
/// RFC 8628 §3.5: add 5 s on `slow_down` unless the server sends a new interval.
const SLOW_DOWN_STEP: Duration = Duration::from_secs(5);
/// Consecutive network/5xx failures tolerated while polling before giving up.
const MAX_TRANSIENT_FAILURES: u32 = 3;

/// Endpoints and client settings of a device flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceFlowConfig {
    pub device_code_url: Url,
    pub token_url: Url,
    /// Public OAuth client id (configuration input; no secret).
    pub client_id: String,
    pub scopes: Vec<String>,
    /// Lower bound for the polling interval regardless of what the server says (default 1 s;
    /// tests use zero).
    pub min_poll_interval: Duration,
}

impl DeviceFlowConfig {
    /// GitHub.com / GitHub Enterprise: `<base>/login/device/code` and
    /// `<base>/login/oauth/access_token`. `scopes = None` → [`default_scopes`] for GitHub.
    pub fn github(
        endpoint: &ProviderEndpoint,
        client_id: &str,
        scopes: Option<Vec<String>>,
    ) -> Result<Self, AuthError> {
        Self::build(
            endpoint,
            ProviderFamily::Github,
            "login/device/code",
            "login/oauth/access_token",
            client_id,
            scopes,
        )
    }

    /// GitLab (where enabled): `<base>/oauth/authorize_device` and `<base>/oauth/token`.
    /// A 404 from the instance surfaces as [`AuthError::DeviceFlowUnsupported`].
    pub fn gitlab(
        endpoint: &ProviderEndpoint,
        client_id: &str,
        scopes: Option<Vec<String>>,
    ) -> Result<Self, AuthError> {
        Self::build(
            endpoint,
            ProviderFamily::Gitlab,
            "oauth/authorize_device",
            "oauth/token",
            client_id,
            scopes,
        )
    }

    /// Picks [`Self::github`] or [`Self::gitlab`] from the endpoint kind.
    pub fn for_endpoint(
        endpoint: &ProviderEndpoint,
        client_id: &str,
        scopes: Option<Vec<String>>,
    ) -> Result<Self, AuthError> {
        match endpoint.kind() {
            ProviderKind::Github | ProviderKind::GithubEnterprise => {
                Self::github(endpoint, client_id, scopes)
            }
            ProviderKind::Gitlab | ProviderKind::GitlabSelfHosted => {
                Self::gitlab(endpoint, client_id, scopes)
            }
        }
    }

    fn build(
        endpoint: &ProviderEndpoint,
        family: ProviderFamily,
        device_path: &str,
        token_path: &str,
        client_id: &str,
        scopes: Option<Vec<String>>,
    ) -> Result<Self, AuthError> {
        if endpoint.kind().family() != family {
            return Err(AuthError::InvalidConfig {
                message: "endpoint kind does not match the device flow provider".into(),
            });
        }
        let client_id = client_id.trim();
        if client_id.is_empty() {
            return Err(AuthError::InvalidConfig {
                message: "missing OAuth client id".into(),
            });
        }
        Ok(Self {
            device_code_url: endpoint.web(device_path)?,
            token_url: endpoint.web(token_path)?,
            client_id: client_id.to_owned(),
            scopes: scopes.unwrap_or_else(|| default_scopes(family)),
            min_poll_interval: Duration::from_secs(1),
        })
    }
}

/// What the UI shows while waiting: the code to type and where.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DevicePrompt {
    /// e.g. `WDJB-MJHT` — show large, offer "copy".
    pub user_code: String,
    pub verification_uri: String,
    /// URL with the code pre-filled (GitLab); open this one when present.
    pub verification_uri_complete: Option<String>,
    /// Unix seconds after which the code is useless.
    pub expires_at: i64,
    pub interval_secs: u64,
}

/// A pending device authorization. Holds the secret device code; not serializable.
#[derive(Debug, Clone)]
pub struct DeviceAuthorization {
    pub prompt: DevicePrompt,
    device_code: SecretToken,
    interval: Duration,
    deadline: Instant,
}

#[derive(Deserialize)]
struct RawDeviceCode {
    device_code: Option<zeroize::Zeroizing<String>>,
    user_code: Option<String>,
    verification_uri: Option<String>,
    /// Some servers (older drafts) use `verification_url`.
    verification_url: Option<String>,
    verification_uri_complete: Option<String>,
    expires_in: Option<u64>,
    interval: Option<u64>,
    error: Option<String>,
    error_description: Option<String>,
}

/// Step 1: asks for a device + user code.
pub async fn request_device_code(
    http: &reqwest::Client,
    config: &DeviceFlowConfig,
) -> Result<DeviceAuthorization, AuthError> {
    let scope = config.scopes.join(" ");
    let resp = http
        .post(config.device_code_url.clone())
        .header(ACCEPT, "application/json")
        .form(&[("client_id", config.client_id.as_str()), ("scope", &scope)])
        .send()
        .await?;
    let status = resp.status();
    let text = resp.text().await?;
    let raw: Option<RawDeviceCode> = serde_json::from_str(&text).ok();
    let Some(raw) = raw else {
        return Err(if status.as_u16() == 404 {
            AuthError::DeviceFlowUnsupported {
                message: "the server has no device authorization endpoint".into(),
            }
        } else {
            status_error(status)
        });
    };
    if let Some(error) = raw.error {
        return Err(match error.as_str() {
            "device_flow_disabled" | "unauthorized_client" | "unsupported_grant_type" => {
                AuthError::DeviceFlowUnsupported {
                    message: raw.error_description.unwrap_or(error),
                }
            }
            _ => AuthError::OAuth {
                error,
                description: raw.error_description,
            },
        });
    }
    if status.as_u16() == 404 {
        return Err(AuthError::DeviceFlowUnsupported {
            message: "the server has no device authorization endpoint".into(),
        });
    }
    let missing = |field: &str| AuthError::Provider {
        error: ProviderError::InvalidResponse {
            message: format!("device authorization response without `{field}`"),
        },
    };
    let device_code = raw.device_code.ok_or_else(|| missing("device_code"))?;
    let user_code = raw.user_code.ok_or_else(|| missing("user_code"))?;
    let verification_uri = raw
        .verification_uri
        .or(raw.verification_url)
        .ok_or_else(|| missing("verification_uri"))?;
    let expires_in = raw.expires_in.unwrap_or(900);
    let interval = raw.interval.unwrap_or(5);
    Ok(DeviceAuthorization {
        prompt: DevicePrompt {
            user_code,
            verification_uri,
            verification_uri_complete: raw.verification_uri_complete,
            expires_at: now_unix() + expires_in as i64,
            interval_secs: interval,
        },
        device_code: SecretToken::from(device_code),
        interval: Duration::from_secs(interval),
        deadline: Instant::now() + Duration::from_secs(expires_in),
    })
}

/// New polling interval after `slow_down`.
pub(crate) fn slow_down_interval(current: Duration, server_interval: Option<u64>) -> Duration {
    match server_interval {
        Some(secs) => Duration::from_secs(secs).max(current),
        None => current + SLOW_DOWN_STEP,
    }
}

/// Step 2: polls the token endpoint until the user approves, declines, the code expires or
/// `cancel` fires. Dropping the returned future also stops polling.
///
/// Handles `authorization_pending`, `slow_down` (interval grows), `expired_token`
/// ([`AuthError::Expired`]), `access_denied` ([`AuthError::Denied`]); up to 3 consecutive
/// network/5xx failures are retried.
pub async fn poll_device_token(
    http: &reqwest::Client,
    config: &DeviceFlowConfig,
    auth: &DeviceAuthorization,
    cancel: &CancellationToken,
) -> Result<TokenSet, AuthError> {
    let mut interval = auth.interval.max(config.min_poll_interval);
    let mut failures = 0u32;
    loop {
        if Instant::now() >= auth.deadline {
            return Err(AuthError::Expired);
        }
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(AuthError::Cancelled),
            _ = tokio::time::sleep(interval) => {}
        }
        let params = [
            ("client_id", config.client_id.as_str()),
            ("device_code", auth.device_code.expose()),
            ("grant_type", GRANT_TYPE),
        ];
        let reply = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(AuthError::Cancelled),
            r = post_token_endpoint(http, &config.token_url, &params) => r,
        };
        let reply = match reply {
            Ok(reply) => {
                failures = 0;
                reply
            }
            Err(AuthError::Provider { error }) if error.is_transient() => {
                failures += 1;
                if failures > MAX_TRANSIENT_FAILURES {
                    return Err(AuthError::Provider { error });
                }
                if let ProviderError::RateLimited { .. } = error {
                    interval = slow_down_interval(interval, None);
                }
                continue;
            }
            Err(e) => return Err(e),
        };
        match reply {
            TokenReply::Token(set) => return Ok(set),
            TokenReply::Error {
                error,
                interval: server_interval,
                description,
            } => match error.as_str() {
                "authorization_pending" => {}
                "slow_down" => {
                    interval =
                        slow_down_interval(interval, server_interval).max(config.min_poll_interval);
                }
                "expired_token" | "token_expired" => return Err(AuthError::Expired),
                "access_denied" => return Err(AuthError::Denied),
                "device_flow_disabled" => {
                    return Err(AuthError::DeviceFlowUnsupported {
                        message: description.unwrap_or(error),
                    });
                }
                _ => return Err(AuthError::OAuth { error, description }),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_down_grows_interval() {
        let five = Duration::from_secs(5);
        assert_eq!(slow_down_interval(five, None), Duration::from_secs(10));
        assert_eq!(slow_down_interval(five, Some(15)), Duration::from_secs(15));
        // Never shrink on a bogus server value.
        assert_eq!(slow_down_interval(five, Some(1)), five);
    }

    #[test]
    fn config_urls_for_github_enterprise_and_gitlab() {
        let ghe = ProviderEndpoint::github_enterprise("https://ghe.acme.com").unwrap();
        let c = DeviceFlowConfig::github(&ghe, " Iv1.abc ", None).unwrap();
        assert_eq!(
            c.device_code_url.as_str(),
            "https://ghe.acme.com/login/device/code"
        );
        assert_eq!(
            c.token_url.as_str(),
            "https://ghe.acme.com/login/oauth/access_token"
        );
        assert_eq!(c.client_id, "Iv1.abc");
        assert_eq!(c.scopes, ["repo", "read:user", "user:email"]);
        let gl = DeviceFlowConfig::for_endpoint(&ProviderEndpoint::gitlab(), "id", None).unwrap();
        assert_eq!(
            gl.device_code_url.as_str(),
            "https://gitlab.com/oauth/authorize_device"
        );
        assert!(DeviceFlowConfig::github(&ProviderEndpoint::gitlab(), "id", None).is_err());
        assert!(DeviceFlowConfig::github(&ProviderEndpoint::github(), " ", None).is_err());
    }
}
