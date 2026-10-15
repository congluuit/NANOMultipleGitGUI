//! Device flow against a wiremock server (no real network).

use std::time::{Duration, Instant};

use nanomgg_auth::*;
use nanomgg_providers::{HttpConfig, build_http_client};
use serde_json::json;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn http() -> reqwest::Client {
    build_http_client(&HttpConfig::default()).unwrap()
}

fn github_cfg(server: &MockServer) -> DeviceFlowConfig {
    let endpoint = ProviderEndpoint::custom(
        ProviderKind::GithubEnterprise,
        &server.uri(),
        &format!("{}/api/v3", server.uri()),
    )
    .unwrap();
    let mut cfg = DeviceFlowConfig::github(&endpoint, "Iv1.test", None).unwrap();
    cfg.min_poll_interval = Duration::ZERO;
    cfg
}

async fn mount_device_code(server: &MockServer, interval: u64, expires_in: u64) {
    Mock::given(method("POST"))
        .and(path("/login/device/code"))
        .and(body_string_contains("client_id=Iv1.test"))
        .and(body_string_contains(
            "scope=repo+workflow+read%3Auser+user%3Aemail",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "device_code": "dev-secret-123", "user_code": "WDJB-MJHT",
            "verification_uri": "https://github.com/login/device",
            "expires_in": expires_in, "interval": interval
        })))
        .expect(1)
        .mount(server)
        .await;
}

fn token_reply(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

async fn mount_poll(server: &MockServer, body: serde_json::Value, times: Option<u64>) {
    let mock = Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .and(body_string_contains("device_code=dev-secret-123"))
        .and(body_string_contains(
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code",
        ))
        .respond_with(token_reply(body));
    match times {
        Some(n) => mock.up_to_n_times(n).mount(server).await,
        None => mock.mount(server).await,
    }
}

#[tokio::test]
async fn happy_path_after_pending() {
    let server = MockServer::start().await;
    mount_device_code(&server, 0, 900).await;
    mount_poll(&server, json!({"error": "authorization_pending"}), Some(2)).await;
    mount_poll(
        &server,
        json!({"access_token": "gho_abcdefghijklmnopqrstuvwxyz0123", "token_type": "bearer", "scope": "repo,read:user,user:email"}),
        None,
    )
    .await;

    let cfg = github_cfg(&server);
    let http = http();
    let auth = request_device_code(&http, &cfg).await.unwrap();
    assert_eq!(auth.prompt.user_code, "WDJB-MJHT");
    assert_eq!(
        auth.prompt.verification_uri,
        "https://github.com/login/device"
    );
    assert!(!format!("{auth:?}").contains("dev-secret-123"));
    let prompt_json = serde_json::to_value(&auth.prompt).unwrap();
    assert!(prompt_json.get("userCode").is_some() && prompt_json.get("deviceCode").is_none());

    let tokens = poll_device_token(&http, &cfg, &auth, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(
        tokens.access_token.expose(),
        "gho_abcdefghijklmnopqrstuvwxyz0123"
    );
    assert_eq!(tokens.scopes, ["repo", "read:user", "user:email"]);
    assert_eq!(tokens.expires_at, None);
    assert!(tokens.refresh_token.is_none());
    let polls = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/login/oauth/access_token")
        .count();
    assert_eq!(polls, 3);
}

#[tokio::test]
async fn slow_down_increases_interval() {
    let server = MockServer::start().await;
    mount_device_code(&server, 0, 900).await;
    // GitHub sends the new interval with `slow_down`.
    mount_poll(
        &server,
        json!({"error": "slow_down", "interval": 1}),
        Some(1),
    )
    .await;
    mount_poll(
        &server,
        json!({"access_token": "gho_x", "scope": "repo"}),
        None,
    )
    .await;
    let cfg = github_cfg(&server);
    let http = http();
    let auth = request_device_code(&http, &cfg).await.unwrap();
    let started = Instant::now();
    let tokens = poll_device_token(&http, &cfg, &auth, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(tokens.access_token.expose(), "gho_x");
    assert!(
        started.elapsed() >= Duration::from_millis(950),
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn expired_and_denied() {
    let server = MockServer::start().await;
    mount_device_code(&server, 0, 900).await;
    mount_poll(
        &server,
        json!({"error": "expired_token", "error_description": "too late"}),
        None,
    )
    .await;
    let cfg = github_cfg(&server);
    let http = http();
    let auth = request_device_code(&http, &cfg).await.unwrap();
    let e = poll_device_token(&http, &cfg, &auth, &CancellationToken::new())
        .await
        .unwrap_err();
    assert_eq!(e, AuthError::Expired);

    let server = MockServer::start().await;
    mount_device_code(&server, 0, 900).await;
    mount_poll(&server, json!({"error": "access_denied"}), None).await;
    let cfg = github_cfg(&server);
    let auth = request_device_code(&http, &cfg).await.unwrap();
    let e = poll_device_token(&http, &cfg, &auth, &CancellationToken::new())
        .await
        .unwrap_err();
    assert_eq!(e, AuthError::Denied);
}

#[tokio::test]
async fn local_deadline_expires_without_polling() {
    let server = MockServer::start().await;
    mount_device_code(&server, 0, 0).await;
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(token_reply(json!({"error": "authorization_pending"})))
        .expect(0)
        .mount(&server)
        .await;
    let cfg = github_cfg(&server);
    let http = http();
    let auth = request_device_code(&http, &cfg).await.unwrap();
    let e = poll_device_token(&http, &cfg, &auth, &CancellationToken::new())
        .await
        .unwrap_err();
    assert_eq!(e, AuthError::Expired);
}

#[tokio::test]
async fn cancel_stops_polling() {
    let server = MockServer::start().await;
    mount_device_code(&server, 1, 900).await;
    mount_poll(&server, json!({"error": "authorization_pending"}), None).await;
    let cfg = github_cfg(&server);
    let http = http();
    let auth = request_device_code(&http, &cfg).await.unwrap();
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        trigger.cancel();
    });
    let started = Instant::now();
    let e = poll_device_token(&http, &cfg, &auth, &cancel)
        .await
        .unwrap_err();
    assert_eq!(e, AuthError::Cancelled);
    assert!(started.elapsed() < Duration::from_millis(900));
}

#[tokio::test]
async fn gitlab_style_400_errors_and_refresh_token() {
    let server = MockServer::start().await;
    let endpoint = ProviderEndpoint::gitlab_self_hosted(&server.uri()).unwrap();
    let mut cfg =
        DeviceFlowConfig::for_endpoint(&endpoint, "gl-app", Some(vec!["api".into()])).unwrap();
    cfg.min_poll_interval = Duration::ZERO;
    Mock::given(method("POST"))
        .and(path("/oauth/authorize_device"))
        .and(body_string_contains("scope=api"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "device_code": "gl-dev", "user_code": "ABCD-1234",
            "verification_uri": "https://gitlab.example/oauth/device",
            "verification_uri_complete": "https://gitlab.example/oauth/device?user_code=ABCD-1234",
            "expires_in": 300, "interval": 0
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": "authorization_pending", "error_description": "pending"
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(502).set_body_string("bad gateway"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .and(body_string_contains("device_code=gl-dev"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "gloas-token", "token_type": "Bearer", "expires_in": 7200,
            "refresh_token": "gl-refresh", "scope": "api", "created_at": nanomgg_providers::now_unix()
        })))
        .mount(&server)
        .await;
    let http = http();
    let auth = request_device_code(&http, &cfg).await.unwrap();
    assert!(auth.prompt.verification_uri_complete.is_some());
    let tokens = poll_device_token(&http, &cfg, &auth, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(
        tokens.refresh_token.as_ref().unwrap().expose(),
        "gl-refresh"
    );
    let left = tokens.expires_at.unwrap() - nanomgg_providers::now_unix();
    assert!((7100..=7200).contains(&left), "{left}");
}

#[tokio::test]
async fn unsupported_device_flow() {
    let server = MockServer::start().await;
    let endpoint = ProviderEndpoint::gitlab_self_hosted(&server.uri()).unwrap();
    let cfg = DeviceFlowConfig::gitlab(&endpoint, "gl-app", None).unwrap();
    Mock::given(method("POST"))
        .and(path("/oauth/authorize_device"))
        .respond_with(ResponseTemplate::new(404).set_body_string("<html>404</html>"))
        .mount(&server)
        .await;
    let e = request_device_code(&http(), &cfg).await.unwrap_err();
    assert!(
        matches!(e, AuthError::DeviceFlowUnsupported { .. }),
        "{e:?}"
    );

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/login/device/code"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": "device_flow_disabled", "error_description": "Device Flow must be explicitly enabled for this App"
        })))
        .mount(&server)
        .await;
    let e = request_device_code(&http(), &github_cfg(&server))
        .await
        .unwrap_err();
    assert!(
        matches!(e, AuthError::DeviceFlowUnsupported { .. }),
        "{e:?}"
    );
}
