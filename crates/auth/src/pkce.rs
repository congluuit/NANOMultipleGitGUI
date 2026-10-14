//! OAuth Authorization Code + PKCE (RFC 7636, S256) with a loopback redirect (RFC 8252).
//!
//! ```no_run
//! # async fn demo(http: reqwest::Client) -> Result<(), nanomgg_auth::AuthError> {
//! use nanomgg_auth::*;
//! let cfg = PkceConfig::gitlab(&ProviderEndpoint::gitlab(), "<application id>", None)?;
//! let login = LoopbackLogin::start(cfg).await?;
//! // open login.authorize_url() in the system browser, then:
//! let cancel = CancellationToken::new();
//! let tokens = login.finish(&http, &cancel).await?;
//! # let _ = tokens; Ok(()) }
//! ```
//!
//! The listener binds `127.0.0.1` only, answers exactly one valid `GET /callback`, checks
//! `state` before touching `code`, and serves a tiny bilingual page that never echoes request
//! data. Register `http://127.0.0.1/callback` as redirect URI in the GitLab application;
//! GitLab (Doorkeeper ≥ 5.5) accepts any port for loopback IPs.

use std::time::Duration;

use base64::Engine;
use nanomgg_providers::{ProviderEndpoint, ProviderFamily, SecretToken};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;
use url::Url;
use zeroize::Zeroizing;

use crate::scopes::default_scopes;
use crate::token::{TokenReply, TokenSet, post_token_endpoint};
use crate::{AuthError, random_urlsafe};

const CALLBACK_PATH: &str = "/callback";
const MAX_REQUEST_HEAD: usize = 8 * 1024;
const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Endpoints and client settings of a PKCE login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkceConfig {
    pub authorize_url: Url,
    pub token_url: Url,
    /// Public OAuth client id (GitLab "Application ID"; configuration input, no secret).
    pub client_id: String,
    pub scopes: Vec<String>,
    /// Loopback port; `0` (default) picks a free random port.
    pub port: u16,
    /// How long [`LoopbackLogin::finish`] waits for the browser (default 5 minutes).
    pub timeout: Duration,
}

impl PkceConfig {
    /// GitLab.com / self-hosted: `<base>/oauth/authorize` and `<base>/oauth/token`.
    /// `scopes = None` → [`default_scopes`] for GitLab.
    pub fn gitlab(
        endpoint: &ProviderEndpoint,
        client_id: &str,
        scopes: Option<Vec<String>>,
    ) -> Result<Self, AuthError> {
        if endpoint.kind().family() != ProviderFamily::Gitlab {
            return Err(AuthError::InvalidConfig {
                message: "PKCE loopback login is implemented for GitLab endpoints".into(),
            });
        }
        let client_id = client_id.trim();
        if client_id.is_empty() {
            return Err(AuthError::InvalidConfig {
                message: "missing OAuth client id".into(),
            });
        }
        Ok(Self {
            authorize_url: endpoint.web("oauth/authorize")?,
            token_url: endpoint.web("oauth/token")?,
            client_id: client_id.to_owned(),
            scopes: scopes.unwrap_or_else(|| default_scopes(ProviderFamily::Gitlab)),
            port: 0,
            timeout: Duration::from_secs(300),
        })
    }
}

/// A started loopback login: listener bound, PKCE verifier and `state` generated.
/// Dropping it closes the listener (aborts the login).
#[derive(Debug)]
pub struct LoopbackLogin {
    config: PkceConfig,
    listener: TcpListener,
    redirect_uri: String,
    authorize_url: Url,
    state: SecretToken,
    verifier: SecretToken,
}

/// S256 code challenge for `verifier`.
pub(crate) fn code_challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

impl LoopbackLogin {
    /// Binds `127.0.0.1:<port>` and prepares the authorization URL.
    pub async fn start(config: PkceConfig) -> Result<Self, AuthError> {
        let listener = TcpListener::bind(("127.0.0.1", config.port))
            .await
            .map_err(|e| AuthError::Loopback {
                message: format!("cannot listen on 127.0.0.1:{}: {e}", config.port),
            })?;
        let port = listener
            .local_addr()
            .map_err(|e| AuthError::Loopback {
                message: e.to_string(),
            })?
            .port();
        let redirect_uri = format!("http://127.0.0.1:{port}{CALLBACK_PATH}");
        // 32 bytes → 43 base64url chars: the RFC 7636 minimum verifier length, 256 bits.
        let verifier = SecretToken::new(random_urlsafe(32)?);
        let state = SecretToken::new(random_urlsafe(24)?);
        let mut authorize_url = config.authorize_url.clone();
        authorize_url
            .query_pairs_mut()
            .append_pair("client_id", &config.client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", &config.scopes.join(" "))
            .append_pair("state", state.expose())
            .append_pair("code_challenge", &code_challenge(verifier.expose()))
            .append_pair("code_challenge_method", "S256");
        Ok(Self {
            config,
            listener,
            redirect_uri,
            authorize_url,
            state,
            verifier,
        })
    }

    /// Open this in the system browser. Contains `state` and the code challenge (not secret
    /// enough to matter without the verifier) — do not log it anyway.
    pub fn authorize_url(&self) -> &Url {
        &self.authorize_url
    }

    /// `http://127.0.0.1:<port>/callback`.
    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    /// Waits for the browser redirect, verifies `state`, exchanges the code (with the PKCE
    /// verifier) and answers the browser with a success/failure page.
    ///
    /// Errors: [`AuthError::Cancelled`] (token fired), [`AuthError::TimedOut`],
    /// [`AuthError::Denied`] (user clicked "Deny"), [`AuthError::StateMismatch`],
    /// [`AuthError::OAuth`] / [`AuthError::Provider`] for exchange failures.
    pub async fn finish(
        self,
        http: &reqwest::Client,
        cancel: &CancellationToken,
    ) -> Result<TokenSet, AuthError> {
        let deadline = tokio::time::sleep(self.config.timeout);
        tokio::pin!(deadline);
        loop {
            let accepted = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(AuthError::Cancelled),
                _ = &mut deadline => return Err(AuthError::TimedOut),
                a = self.listener.accept() => a,
            };
            let Ok((stream, _)) = accepted else {
                continue;
            };
            let outcome = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(AuthError::Cancelled),
                o = self.handle(stream, http) => o,
            };
            if let Some(result) = outcome {
                return result;
            }
        }
    }

    /// `None`: not our callback (favicon, stray request) — keep waiting.
    async fn handle(
        &self,
        mut stream: TcpStream,
        http: &reqwest::Client,
    ) -> Option<Result<TokenSet, AuthError>> {
        let head = match tokio::time::timeout(REQUEST_READ_TIMEOUT, read_head(&mut stream)).await {
            Ok(Some(head)) => head,
            _ => return None,
        };
        let target = head
            .lines()
            .next()
            .and_then(|l| {
                let mut it = l.split_whitespace();
                (it.next() == Some("GET")).then(|| it.next()).flatten()
            })
            .map(str::to_owned);
        let Some(target) = target else {
            respond(&mut stream, 400, Page::Failure).await;
            return None;
        };
        let (path, query) = target.split_once('?').unwrap_or((target.as_str(), ""));
        if path != CALLBACK_PATH {
            respond(&mut stream, 404, Page::NotFound).await;
            return None;
        }
        let mut code: Option<Zeroizing<String>> = None;
        let mut state: Option<Zeroizing<String>> = None;
        let mut error: Option<String> = None;
        let mut description: Option<String> = None;
        for (k, v) in url::form_urlencoded::parse(query.as_bytes()) {
            match &*k {
                "code" => code = Some(Zeroizing::new(v.into_owned())),
                "state" => state = Some(Zeroizing::new(v.into_owned())),
                "error" => error = Some(v.into_owned()),
                "error_description" => description = Some(v.into_owned()),
                _ => {}
            }
        }
        if code.is_none() && error.is_none() {
            respond(&mut stream, 400, Page::Failure).await;
            return None;
        }
        let state_ok = state
            .as_deref()
            .is_some_and(|s| constant_time_eq(s.as_bytes(), self.state.expose().as_bytes()));
        if !state_ok {
            respond(&mut stream, 400, Page::Failure).await;
            return Some(Err(AuthError::StateMismatch));
        }
        if let Some(error) = error {
            let denied = error == "access_denied";
            respond(
                &mut stream,
                200,
                if denied { Page::Denied } else { Page::Failure },
            )
            .await;
            return Some(Err(if denied {
                AuthError::Denied
            } else {
                AuthError::OAuth { error, description }
            }));
        }
        let code = code.unwrap_or_default();
        let result = self.exchange(http, &code).await;
        let page = if result.is_ok() {
            Page::Success
        } else {
            Page::Failure
        };
        respond(&mut stream, 200, page).await;
        Some(result)
    }

    async fn exchange(&self, http: &reqwest::Client, code: &str) -> Result<TokenSet, AuthError> {
        let params = [
            ("grant_type", "authorization_code"),
            ("client_id", self.config.client_id.as_str()),
            ("code", code),
            ("redirect_uri", self.redirect_uri.as_str()),
            ("code_verifier", self.verifier.expose()),
        ];
        match post_token_endpoint(http, &self.config.token_url, &params).await? {
            TokenReply::Token(set) => Ok(set),
            TokenReply::Error { error, .. } if error == "access_denied" => Err(AuthError::Denied),
            TokenReply::Error {
                error, description, ..
            } => Err(AuthError::OAuth { error, description }),
        }
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Reads up to the end of the HTTP request head. `None` on EOF/oversize/garbage.
async fn read_head(stream: &mut TcpStream) -> Option<Zeroizing<String>> {
    let mut buf = Zeroizing::new(Vec::with_capacity(1024));
    let mut chunk = [0u8; 1024];
    loop {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") || buf.windows(2).any(|w| w == b"\n\n") {
            break;
        }
        if buf.len() > MAX_REQUEST_HEAD {
            return None;
        }
    }
    String::from_utf8(std::mem::take(&mut *buf))
        .ok()
        .map(Zeroizing::new)
}

#[derive(Clone, Copy)]
enum Page {
    Success,
    Denied,
    Failure,
    NotFound,
}

fn page_html(page: Page) -> String {
    let (title, en, vi) = match page {
        Page::Success => (
            "Signed in · Đã đăng nhập",
            "You are signed in. You can close this tab and return to NANOMultipleGitGUI.",
            "Đăng nhập thành công. Bạn có thể đóng tab này và quay lại NANOMultipleGitGUI.",
        ),
        Page::Denied => (
            "Sign-in cancelled · Đã hủy đăng nhập",
            "Sign-in was cancelled. You can close this tab.",
            "Đăng nhập đã bị hủy. Bạn có thể đóng tab này.",
        ),
        Page::Failure => (
            "Sign-in failed · Đăng nhập không thành công",
            "Sign-in did not complete. Please return to NANOMultipleGitGUI and try again.",
            "Đăng nhập chưa hoàn tất. Vui lòng quay lại NANOMultipleGitGUI và thử lại.",
        ),
        Page::NotFound => (
            "Not found · Không tìm thấy",
            "Not found.",
            "Không tìm thấy.",
        ),
    };
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title>\
         <meta name=\"viewport\" content=\"width=device-width\"></head>\
         <body style=\"font-family:system-ui,sans-serif;max-width:32rem;margin:4rem auto;padding:0 1rem\">\
         <h1 style=\"font-size:1.25rem\">NANOMultipleGitGUI</h1>\
         <p lang=\"en\">{en}</p><p lang=\"vi\">{vi}</p></body></html>"
    )
}

async fn respond(stream: &mut TcpStream, status: u16, page: Page) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Not Found",
    };
    let body = page_html(page);
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\n\
         X-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(body.as_bytes()).await;
    let _ = stream.shutdown().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s256_challenge_matches_rfc7636_example() {
        // RFC 7636 Appendix B.
        assert_eq!(
            code_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn constant_time_compare() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }

    #[test]
    fn config_requires_gitlab_and_client_id() {
        assert!(PkceConfig::gitlab(&ProviderEndpoint::github(), "x", None).is_err());
        assert!(PkceConfig::gitlab(&ProviderEndpoint::gitlab(), "", None).is_err());
        let c = PkceConfig::gitlab(&ProviderEndpoint::gitlab(), "app", None).unwrap();
        assert_eq!(
            c.authorize_url.as_str(),
            "https://gitlab.com/oauth/authorize"
        );
        assert_eq!(c.port, 0);
    }
}
