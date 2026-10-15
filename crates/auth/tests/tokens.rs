//! Refresh and token validation against a wiremock server (no real network).

use nanomgg_auth::*;
use nanomgg_providers::{HttpConfig, ProviderError, build_http_client, now_unix, parse_timestamp};
use serde_json::json;
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn http() -> reqwest::Client {
    build_http_client(&HttpConfig::default()).unwrap()
}

fn gitlab(server: &MockServer) -> ProviderEndpoint {
    ProviderEndpoint::gitlab_self_hosted(&server.uri()).unwrap()
}

fn github(server: &MockServer) -> ProviderEndpoint {
    ProviderEndpoint::custom(
        ProviderKind::GithubEnterprise,
        &server.uri(),
        &format!("{}/api/v3", server.uri()),
    )
    .unwrap()
}

fn gh_user() -> serde_json::Value {
    json!({"id": 1, "login": "hoangnv", "name": "Hoàng", "avatar_url": "https://a/1.png", "html_url": "https://github.com/hoangnv"})
}

fn gl_user() -> serde_json::Value {
    json!({"id": 9, "username": "hoang", "name": "Hoàng", "avatar_url": "https://g/a.png", "web_url": "https://gitlab/hoang", "email": "h@acme.vn"})
}

#[tokio::test]
async fn refresh_rotates_tokens() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=old-refresh"))
        .and(body_string_contains("client_id=gl-app"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "new-access", "token_type": "Bearer", "expires_in": 7200,
            "refresh_token": "new-refresh", "scope": "api", "created_at": now_unix()
        })))
        .expect(1)
        .mount(&server)
        .await;
    let set = refresh_gitlab_token(
        &http(),
        &gitlab(&server),
        "gl-app",
        &SecretToken::new("old-refresh"),
    )
    .await
    .unwrap();
    assert_eq!(set.access_token.expose(), "new-access");
    assert_eq!(set.refresh_token.as_ref().unwrap().expose(), "new-refresh");
    assert!(set.expires_at.unwrap() > now_unix() + 7000);
    assert!(!needs_refresh(set.expires_at, now_unix(), 300));
}

#[tokio::test]
async fn refresh_keeps_old_refresh_token_when_not_rotated() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "new-access", "expires_in": 3600
        })))
        .mount(&server)
        .await;
    let set = refresh_gitlab_token(
        &http(),
        &gitlab(&server),
        "gl-app",
        &SecretToken::new("keep-me"),
    )
    .await
    .unwrap();
    assert_eq!(set.refresh_token.unwrap().expose(), "keep-me");
}

#[tokio::test]
async fn refresh_rejected_means_sign_in_again() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": "invalid_grant", "error_description": "The provided authorization grant is invalid, expired, revoked"
        })))
        .mount(&server)
        .await;
    let e = refresh_gitlab_token(
        &http(),
        &gitlab(&server),
        "gl-app",
        &SecretToken::new("dead"),
    )
    .await
    .unwrap_err();
    assert!(matches!(e, AuthError::RefreshRejected { .. }), "{e:?}");
    assert_eq!(e.code(), "refreshRejected");
}

#[tokio::test]
async fn github_refresh_rotates_expiring_user_tokens() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=ghr_old"))
        .and(body_string_contains("client_id=Ov23app"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "ghu_new", "token_type": "bearer", "expires_in": 28800,
            "refresh_token": "ghr_new", "refresh_token_expires_in": 15897600, "scope": ""
        })))
        .expect(1)
        .mount(&server)
        .await;
    let set = refresh_github_token(
        &http(),
        &github(&server),
        "Ov23app",
        &SecretToken::new("ghr_old"),
    )
    .await
    .unwrap();
    assert_eq!(set.access_token.expose(), "ghu_new");
    assert_eq!(set.refresh_token.as_ref().unwrap().expose(), "ghr_new");
    assert!(set.expires_at.unwrap() > now_unix() + 28_000);
}

#[tokio::test]
async fn github_bad_refresh_token_means_sign_in_again() {
    let server = MockServer::start().await;
    // GitHub reports OAuth errors with HTTP 200.
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "error": "bad_refresh_token", "error_description": "The refresh token passed is incorrect or expired."
        })))
        .mount(&server)
        .await;
    let e = refresh_github_token(
        &http(),
        &github(&server),
        "Ov23app",
        &SecretToken::new("ghr_dead"),
    )
    .await
    .unwrap_err();
    assert!(matches!(e, AuthError::RefreshRejected { .. }), "{e:?}");
}

#[tokio::test]
async fn github_classic_pat_scopes_and_expiry() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .and(header("authorization", "Bearer ghp_classic0123456789"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-oauth-scopes", "repo, read:user, user:email")
                .insert_header(
                    "github-authentication-token-expiration",
                    "2030-01-01 00:00:00 UTC",
                )
                .set_body_json(gh_user()),
        )
        .mount(&server)
        .await;
    let info = validate_token(
        &http(),
        &github(&server),
        &SecretToken::new("ghp_classic0123456789"),
    )
    .await
    .unwrap();
    assert_eq!(info.user.username, "hoangnv");
    assert_eq!(info.user.avatar_url.as_deref(), Some("https://a/1.png"));
    assert_eq!(info.token_kind, TokenKind::GithubClassicPat);
    assert_eq!(
        info.scopes.as_deref().unwrap(),
        ["repo", "read:user", "user:email"]
    );
    assert_eq!(info.expires_at, parse_timestamp("2030-01-01T00:00:00Z"));

    let health = token_health(
        ProviderFamily::Github,
        info.scopes.as_deref(),
        info.expires_at,
        &[Feature::GitHttps, Feature::UploadSshKey],
        now_unix(),
    );
    assert_eq!(health.missing.len(), 1);
    assert_eq!(health.missing[0].feature, Feature::UploadSshKey);
    assert!(!health.expires_soon);
    let json = serde_json::to_value(&info).unwrap();
    assert_eq!(json["tokenKind"], "githubClassicPat");
}

#[tokio::test]
async fn github_fine_grained_pat_has_unknown_scopes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "github-authentication-token-expiration",
                    "2030-06-01 10:00:00 +0700",
                )
                .set_body_json(gh_user()),
        )
        .mount(&server)
        .await;
    let info = validate_token(
        &http(),
        &github(&server),
        &SecretToken::new("github_pat_11ABCDEF"),
    )
    .await
    .unwrap();
    assert_eq!(info.token_kind, TokenKind::GithubFineGrainedPat);
    assert_eq!(info.scopes, None);
    assert_eq!(info.expires_at, parse_timestamp("2030-06-01T03:00:00Z"));
}

#[tokio::test]
async fn github_bad_token() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message": "Bad credentials"})),
        )
        .mount(&server)
        .await;
    let e = validate_token(&http(), &github(&server), &SecretToken::new("ghp_bad"))
        .await
        .unwrap_err();
    assert!(
        matches!(
            e,
            AuthError::Provider {
                error: ProviderError::Unauthorized { .. }
            }
        ),
        "{e:?}"
    );
    assert!(
        validate_token(&http(), &github(&server), &SecretToken::new(" "))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn gitlab_pat_self() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/personal_access_tokens/self"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 5, "name": "nano laptop", "revoked": false, "active": true,
            "scopes": ["api", "write_repository"], "user_id": 9, "expires_at": "2030-01-01"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(gl_user()))
        .mount(&server)
        .await;
    let info = validate_token(&http(), &gitlab(&server), &SecretToken::new("glpat-abc"))
        .await
        .unwrap();
    assert_eq!(info.token_kind, TokenKind::GitlabAccessToken);
    assert_eq!(info.token_name.as_deref(), Some("nano laptop"));
    assert_eq!(info.scopes.as_deref().unwrap(), ["api", "write_repository"]);
    assert_eq!(info.expires_at, parse_timestamp("2030-01-01T23:59:59Z"));
    assert_eq!(info.user.username, "hoang");
}

#[tokio::test]
async fn gitlab_revoked_pat() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/personal_access_tokens/self"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 5, "name": "old", "revoked": true, "active": false, "scopes": ["api"], "expires_at": null
        })))
        .mount(&server)
        .await;
    let e = validate_token(&http(), &gitlab(&server), &SecretToken::new("glpat-old"))
        .await
        .unwrap_err();
    assert_eq!(e, AuthError::TokenRevoked);
}

#[tokio::test]
async fn gitlab_oauth_token_uses_token_info() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/personal_access_tokens/self"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message": "401 Unauthorized"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/oauth/token/info"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "resource_owner_id": 9, "scope": ["read_user", "read_api"], "expires_in": 3600,
            "application": {"uid": "gl-app"}, "created_at": 1
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(gl_user()))
        .mount(&server)
        .await;
    let info = validate_token(&http(), &gitlab(&server), &SecretToken::new("oauth-token"))
        .await
        .unwrap();
    assert_eq!(info.token_kind, TokenKind::GitlabOauth);
    assert_eq!(info.scopes.as_deref().unwrap(), ["read_user", "read_api"]);
    let left = info.expires_at.unwrap() - now_unix();
    assert!((3500..=3600).contains(&left));
}

#[tokio::test]
async fn gitlab_old_version_falls_back_to_user() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/personal_access_tokens/self"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"error": "404 Not Found"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/oauth/token/info"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"error": "invalid_token"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(gl_user()))
        .mount(&server)
        .await;
    let info = validate_token(&http(), &gitlab(&server), &SecretToken::new("legacy"))
        .await
        .unwrap();
    assert_eq!(info.token_kind, TokenKind::Unknown);
    assert_eq!(info.scopes, None);
    assert_eq!(info.user.username, "hoang");
}
