//! GitLab PKCE loopback login end-to-end: the test plays the browser by calling the callback.

use std::collections::HashMap;
use std::time::Duration;

use base64::Engine;
use nanomgg_auth::*;
use nanomgg_providers::{HttpConfig, build_http_client};
use serde_json::json;
use sha2::{Digest, Sha256};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn http() -> reqwest::Client {
    build_http_client(&HttpConfig::default()).unwrap()
}

fn cfg(server: &MockServer) -> PkceConfig {
    let endpoint = ProviderEndpoint::gitlab_self_hosted(&server.uri()).unwrap();
    PkceConfig::gitlab(&endpoint, "gl-app-id", None).unwrap()
}

fn query(url: &url::Url) -> HashMap<String, String> {
    url.query_pairs().into_owned().collect()
}

/// Plays the browser: GET `url` and return (status, body).
async fn browser_get(url: String) -> (u16, String) {
    let resp = reqwest::Client::new().get(url).send().await.unwrap();
    (resp.status().as_u16(), resp.text().await.unwrap())
}

#[tokio::test]
async fn loopback_login_end_to_end() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code=the-auth-code"))
        .and(body_string_contains("client_id=gl-app-id"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "gloas-access", "token_type": "Bearer", "expires_in": 7200,
            "refresh_token": "gl-refresh", "scope": "read_user read_api read_repository write_repository",
            "created_at": nanomgg_providers::now_unix()
        })))
        .expect(1)
        .mount(&server)
        .await;

    let login = LoopbackLogin::start(cfg(&server)).await.unwrap();
    let auth_url = login.authorize_url().clone();
    assert_eq!(auth_url.path(), "/oauth/authorize");
    let q = query(&auth_url);
    assert_eq!(q["client_id"], "gl-app-id");
    assert_eq!(q["response_type"], "code");
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(
        q["scope"],
        "read_user read_api read_repository write_repository"
    );
    assert_eq!(q["redirect_uri"], login.redirect_uri());
    assert!(login.redirect_uri().starts_with("http://127.0.0.1:"));
    assert!(login.redirect_uri().ends_with("/callback"));
    let state = q["state"].clone();
    let challenge = q["code_challenge"].clone();
    let redirect = login.redirect_uri().to_owned();

    let http = http();
    let task = tokio::spawn(async move { login.finish(&http, &CancellationToken::new()).await });

    // A stray request first (browsers ask for a favicon): ignored with 404.
    let base = redirect.trim_end_matches("/callback").to_owned();
    let (status, _) = browser_get(format!("{base}/favicon.ico")).await;
    assert_eq!(status, 404);

    let (status, page) = browser_get(format!("{redirect}?code=the-auth-code&state={state}")).await;
    assert_eq!(status, 200);
    assert!(page.contains("Đăng nhập thành công") && page.contains("You are signed in"));
    assert!(!page.contains("the-auth-code"));

    let tokens = task.await.unwrap().unwrap();
    assert_eq!(tokens.access_token.expose(), "gloas-access");
    assert_eq!(
        tokens.refresh_token.as_ref().unwrap().expose(),
        "gl-refresh"
    );
    assert_eq!(tokens.scopes.len(), 4);

    // The verifier sent to the token endpoint hashes to the challenge sent to the browser.
    let reqs = server.received_requests().await.unwrap();
    let form: HashMap<String, String> = url::form_urlencoded::parse(&reqs[0].body)
        .into_owned()
        .collect();
    let verifier = &form["code_verifier"];
    assert!(verifier.len() >= 43);
    let expected = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(verifier.as_bytes()));
    assert_eq!(expected, challenge);
    assert_eq!(form["redirect_uri"], redirect);
}

#[tokio::test]
async fn wrong_state_is_rejected_without_exchange() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token": "x"})))
        .expect(0)
        .mount(&server)
        .await;
    let login = LoopbackLogin::start(cfg(&server)).await.unwrap();
    let redirect = login.redirect_uri().to_owned();
    let http = http();
    let task = tokio::spawn(async move { login.finish(&http, &CancellationToken::new()).await });
    let (status, page) = browser_get(format!("{redirect}?code=abc&state=forged")).await;
    assert_eq!(status, 400);
    assert!(page.contains("Đăng nhập không thành công"));
    assert_eq!(task.await.unwrap().unwrap_err(), AuthError::StateMismatch);
}

#[tokio::test]
async fn user_denies_in_browser() {
    let server = MockServer::start().await;
    let login = LoopbackLogin::start(cfg(&server)).await.unwrap();
    let state = query(login.authorize_url())["state"].clone();
    let redirect = login.redirect_uri().to_owned();
    let http = http();
    let task = tokio::spawn(async move { login.finish(&http, &CancellationToken::new()).await });
    let (status, page) = browser_get(format!(
        "{redirect}?error=access_denied&error_description=The+resource+owner+denied&state={state}"
    ))
    .await;
    assert_eq!(status, 200);
    assert!(page.contains("Đăng nhập đã bị hủy"));
    assert_eq!(task.await.unwrap().unwrap_err(), AuthError::Denied);
}

#[tokio::test]
async fn exchange_failure_is_reported() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": "invalid_grant", "error_description": "The provided authorization grant is invalid"
        })))
        .mount(&server)
        .await;
    let login = LoopbackLogin::start(cfg(&server)).await.unwrap();
    let state = query(login.authorize_url())["state"].clone();
    let redirect = login.redirect_uri().to_owned();
    let http = http();
    let task = tokio::spawn(async move { login.finish(&http, &CancellationToken::new()).await });
    let (_, page) = browser_get(format!("{redirect}?code=stale&state={state}")).await;
    assert!(page.contains("Sign-in did not complete"));
    let e = task.await.unwrap().unwrap_err();
    assert!(
        matches!(e, AuthError::OAuth { ref error, .. } if error == "invalid_grant"),
        "{e:?}"
    );
}

#[tokio::test]
async fn cancel_and_timeout() {
    let server = MockServer::start().await;
    let login = LoopbackLogin::start(cfg(&server)).await.unwrap();
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        trigger.cancel();
    });
    assert_eq!(
        login.finish(&http(), &cancel).await.unwrap_err(),
        AuthError::Cancelled
    );

    let mut c = cfg(&server);
    c.timeout = Duration::from_millis(100);
    let login = LoopbackLogin::start(c).await.unwrap();
    let redirect = login.redirect_uri().to_owned();
    assert_eq!(
        login
            .finish(&http(), &CancellationToken::new())
            .await
            .unwrap_err(),
        AuthError::TimedOut
    );
    // Listener is closed once the login is over.
    assert!(reqwest::Client::new().get(redirect).send().await.is_err());
}
