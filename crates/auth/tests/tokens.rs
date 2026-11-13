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

#[tokio::test]
async fn gitea_token_is_validated_through_the_v1_api() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/user"))
        .and(header("authorization", "Bearer gitea-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 42, "login": "hoang", "login_name": "", "full_name": "Hoang Nguyen",
            "email": "h@codeberg.example", "avatar_url": "https://c/a.png",
            "html_url": "https://c/hoang", "is_admin": false
        })))
        .expect(1)
        .mount(&server)
        .await;
    let endpoint = ProviderEndpoint::gitea(&server.uri()).unwrap();
    assert_eq!(endpoint.kind(), ProviderKind::Gitea);
    let info = validate_token(&http(), &endpoint, &SecretToken::new("gitea-token"))
        .await
        .unwrap();
    assert_eq!(info.user.username, "hoang");
    assert_eq!(info.user.display_name.as_deref(), Some("Hoang Nguyen"));
    // Gitea does not report scopes in a header: they are simply unknown.
    assert_eq!(info.scopes, None);
    assert_eq!(info.token_kind, TokenKind::Unknown);

    // Device sign-in is not offered for Gitea.
    assert!(DeviceFlowConfig::for_endpoint(&endpoint, "client", None).is_err());
}

fn custom(kind: ProviderKind, server: &MockServer) -> ProviderEndpoint {
    ProviderEndpoint::custom(kind, &server.uri(), &server.uri()).unwrap()
}

#[tokio::test]
async fn bitbucket_token_is_checked_with_the_account_email() {
    let server = MockServer::start().await;
    // base64("me@acme.vn:bb-token")
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(header("authorization", "Basic bWVAYWNtZS52bjpiYi10b2tlbg=="))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "username": "hoangnv", "nickname": "Hoang", "display_name": "Hoang Nguyen",
            "uuid": "{1-2-3}", "account_id": "557058:abc",
            "links": {"avatar": {"href": "https://b/a.png"}, "html": {"href": "https://bitbucket.org/hoangnv/"}}
        })))
        .mount(&server)
        .await;
    let endpoint = custom(ProviderKind::Bitbucket, &server);
    let token = SecretToken::new("bb-token");
    let info = validate_git_token(&http(), &endpoint, Some("me@acme.vn"), &token)
        .await
        .unwrap();
    assert_eq!(info.user.username, "hoangnv");
    assert_eq!(info.user.display_name.as_deref(), Some("Hoang Nguyen"));
    assert_eq!(info.user.email.as_deref(), Some("me@acme.vn"));
    assert_eq!(info.scopes, None);

    // The email is required, and a wrong token is refused.
    let err = validate_git_token(&http(), &endpoint, None, &token)
        .await
        .unwrap_err();
    assert!(matches!(err, AuthError::InvalidConfig { .. }), "{err}");
    let wrong = SecretToken::new("nope");
    let err = validate_git_token(&http(), &endpoint, Some("me@acme.vn"), &wrong)
        .await
        .unwrap_err();
    assert!(!matches!(err, AuthError::InvalidConfig { .. }), "{err}");
}

#[tokio::test]
async fn azure_devops_token_is_checked_against_the_profile() {
    let server = MockServer::start().await;
    // base64(":az-token")
    Mock::given(method("GET"))
        .and(path("/_apis/profile/profiles/me"))
        .and(header("authorization", "Basic OmF6LXRva2Vu"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "aaaa-bbbb", "displayName": "Hoang Nguyen", "publicAlias": "aaaa-bbbb",
            "emailAddress": "hoang@acme.vn"
        })))
        .mount(&server)
        .await;
    // An invalid token gets the sign-in page instead of JSON.
    Mock::given(method("GET"))
        .and(path("/_apis/profile/profiles/me"))
        .and(header("authorization", "Basic OmJhZA=="))
        .respond_with(ResponseTemplate::new(203).set_body_string("<html>Sign in</html>"))
        .mount(&server)
        .await;
    let endpoint = custom(ProviderKind::AzureDevops, &server);
    let info = validate_git_token(&http(), &endpoint, None, &SecretToken::new("az-token"))
        .await
        .unwrap();
    assert_eq!(info.user.username, "hoang@acme.vn");
    assert_eq!(info.user.display_name.as_deref(), Some("Hoang Nguyen"));

    let err = validate_git_token(&http(), &endpoint, None, &SecretToken::new("bad"))
        .await
        .unwrap_err();
    assert!(
        matches!(
            &err,
            AuthError::Provider {
                error: ProviderError::Unauthorized { .. }
            }
        ),
        "{err}"
    );
}

#[tokio::test]
async fn git_only_servers_never_get_api_requests() {
    let server = MockServer::start().await;
    for kind in [ProviderKind::Bitbucket, ProviderKind::AzureDevops] {
        let endpoint = custom(kind, &server);
        assert!(kind.git_only());
        let client =
            nanomgg_providers::ProviderClient::new(http(), endpoint.clone(), SecretToken::new("t"));
        let err = client
            .api()
            .get_json::<serde_json::Value>(&["user"], &[])
            .await
            .unwrap_err();
        assert!(matches!(err, ProviderError::Unsupported { .. }), "{err}");
        // Nor does the GitHub / GitLab token check run against them.
        assert!(
            validate_token(&http(), &endpoint, &SecretToken::new("t"))
                .await
                .is_err()
        );
    }
    assert!(server.received_requests().await.unwrap().is_empty());
    assert_eq!(ProviderEndpoint::bitbucket().web_host(), "bitbucket.org");
    assert_eq!(ProviderEndpoint::azure_devops().web_host(), "dev.azure.com");
}

/// Talks to the real services (no account needed): each must refuse a wrong token the way
/// the sign-in code expects. Run with `cargo test -p nanomgg-auth --test tokens -- --ignored`.
#[tokio::test]
#[ignore = "needs network access to bitbucket.org, dev.azure.com and codeberg.org"]
async fn real_servers_refuse_a_wrong_token() {
    let wrong = SecretToken::new("not-a-real-token");
    let unauthorized = |result: Result<TokenInfo, AuthError>, who: &str| {
        let err = result.expect_err(who);
        assert!(
            matches!(
                &err,
                AuthError::Provider {
                    error: ProviderError::Unauthorized { .. }
                }
            ),
            "{who}: {err}"
        );
    };
    unauthorized(
        validate_git_token(
            &http(),
            &ProviderEndpoint::bitbucket(),
            Some("nobody@example.invalid"),
            &wrong,
        )
        .await,
        "bitbucket",
    );
    unauthorized(
        validate_git_token(&http(), &ProviderEndpoint::azure_devops(), None, &wrong).await,
        "azure devops",
    );
    let codeberg = ProviderEndpoint::gitea("https://codeberg.org").unwrap();
    unauthorized(validate_token(&http(), &codeberg, &wrong).await, "codeberg");
}
