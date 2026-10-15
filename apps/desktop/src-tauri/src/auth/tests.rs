//! Auth commands driven through their inner functions against wiremock providers, with an
//! in-memory database and the in-memory keychain of `AppState::new`.

use std::time::Duration;

use nanomgg_accounts::{AccountSecret, AuthMethod, Provider, ProviderKind};
use nanomgg_auth::{AuthError, Feature};
use nanomgg_providers::{ProviderEmail, ProviderFamily, ProviderUser, now_unix};
use nanomgg_storage::Storage;
use serde_json::json;
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use zeroize::Zeroizing;

use super::service::{self, compute_health, pick_identity_email};
use super::*;
use crate::accounts::commands::{
    ProviderInput, account_secret, account_store_tokens, provider_save_impl,
};

const PAT: &str = "ghp_testtoken0123456789abcdefghijklmnop";

fn state() -> AppState {
    AppState::new(Storage::open_in_memory().unwrap())
}

fn runtime() -> AuthRuntime {
    let mut rt = AuthRuntime::new().unwrap();
    rt.min_poll_interval = Duration::ZERO;
    rt
}

fn ghe(st: &AppState, server: &MockServer, client_id: Option<&str>) -> Provider {
    provider_save_impl(
        st,
        ProviderInput {
            id: None,
            kind: ProviderKind::GithubEnterprise,
            name: "GHE test".into(),
            base_url: server.uri(),
            api_url: Some(format!("{}/api/v3", server.uri())),
            oauth_client_id: client_id.map(str::to_owned),
            large_file_warn_mb: None,
            large_file_block_mb: None,
        },
    )
    .unwrap()
}

fn gitlab(st: &AppState, server: &MockServer) -> Provider {
    provider_save_impl(
        st,
        ProviderInput {
            id: None,
            kind: ProviderKind::GitlabSelfHosted,
            name: "GitLab test".into(),
            base_url: server.uri(),
            api_url: None,
            oauth_client_id: Some("gl-app-id".into()),
            large_file_warn_mb: None,
            large_file_block_mb: None,
        },
    )
    .unwrap()
}

async fn mount_github_user(server: &MockServer, token: &str, scopes: &str) {
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .and(header("authorization", format!("Bearer {token}").as_str()))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-oauth-scopes", scopes)
                .set_body_json(json!({
                    "id": 42, "login": "hoangnv", "name": "Hoàng NV",
                    "avatar_url": "https://avatars.example/42.png",
                    "html_url": "https://ghe.example/hoangnv", "email": null
                })),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user/emails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"email": "old@x.vn", "primary": false, "verified": true, "visibility": null},
            {"email": "hoang@x.vn", "primary": true, "verified": true, "visibility": "private"},
            {"email": "nope@x.vn", "primary": false, "verified": false, "visibility": null}
        ])))
        .mount(server)
        .await;
}

#[tokio::test]
async fn pat_creates_account_identity_and_keychain_entry() {
    let server = MockServer::start().await;
    mount_github_user(&server, PAT, "repo, read:user, user:email").await;
    let st = state();
    let rt = runtime();
    let p = ghe(&st, &server, None);

    let acc = service::add_pat(
        &st,
        &rt,
        &p.id,
        Zeroizing::new(format!("  {PAT}\n")),
        Some("Cá nhân".into()),
    )
    .await
    .unwrap();
    assert_eq!(acc.username, "hoangnv");
    assert_eq!(acc.display_name.as_deref(), Some("Hoàng NV"));
    assert_eq!(acc.auth_method, AuthMethod::Pat);
    assert_eq!(acc.label.as_deref(), Some("Cá nhân"));
    assert_eq!(acc.scopes, ["repo", "read:user", "user:email"]);
    assert_eq!(acc.secrets, [AccountSecret::Token]);
    let stored = account_secret(&st, &acc.id, AccountSecret::Token)
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.as_str(),
        PAT,
        "token trimmed and stored in the keychain"
    );
    assert!(
        !serde_json::to_string(&acc).unwrap().contains(PAT),
        "token never in the account row"
    );

    let identities = st.storage.identities(Some(&acc.id)).unwrap();
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].email, "hoang@x.vn");
    assert_eq!(identities[0].name, "Hoàng NV");
    assert!(identities[0].is_default);

    let health = service::health(&st, &acc.id).unwrap();
    assert!(health.has_token && health.scopes_known && !health.needs_sign_in);
    assert!(health.missing_scopes.is_empty(), "{health:?}");
    assert!(
        health
            .optional_missing
            .iter()
            .any(|m| m.feature == Feature::UploadSshKey)
    );

    // Same provider + username again: the account is updated, label kept, no new identity.
    let again = service::add_pat(&st, &rt, &p.id, Zeroizing::new(PAT.into()), None)
        .await
        .unwrap();
    assert_eq!(again.id, acc.id);
    assert_eq!(again.label.as_deref(), Some("Cá nhân"));
    assert_eq!(st.storage.accounts().unwrap().len(), 1);
    assert_eq!(st.storage.identities(Some(&acc.id)).unwrap().len(), 1);
}

#[tokio::test]
async fn invalid_pat_and_wrong_account_are_typed_errors() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .and(header("authorization", "Bearer ghp_bad"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message": "Bad credentials"})),
        )
        .mount(&server)
        .await;
    mount_github_user(&server, PAT, "repo").await;
    let st = state();
    let rt = runtime();
    let p = ghe(&st, &server, None);

    let err = service::add_pat(&st, &rt, &p.id, Zeroizing::new("ghp_bad".into()), None)
        .await
        .unwrap_err();
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["kind"], "auth");
    assert_eq!(json["error"]["kind"], "provider");
    assert_eq!(json["error"]["error"]["kind"], "unauthorized");
    assert!(st.storage.accounts().unwrap().is_empty());

    // Re-auth of another user's account with this token is refused.
    let other = crate::accounts::commands::account_save_impl(
        &st,
        crate::accounts::commands::AccountInput {
            id: None,
            provider_id: p.id.clone(),
            username: "someone-else".into(),
            display_name: None,
            avatar_url: None,
            auth_method: AuthMethod::Pat,
            scopes: vec![],
            token_expires_at: None,
            color: None,
            label: None,
            default_clone_dir: None,
        },
    )
    .unwrap();
    let err = service::update_token(&st, &rt, &other.id, Zeroizing::new(PAT.into()))
        .await
        .unwrap_err();
    assert!(
        matches!(err, AuthCommandError::WrongAccount { ref actual, .. } if actual == "hoangnv")
    );
    let health = service::health(&st, &other.id).unwrap();
    assert!(health.needs_sign_in, "no token stored → needs sign-in");
}

#[tokio::test]
async fn device_flow_requires_client_id_and_can_be_cancelled() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/login/device/code"))
        .and(body_string_contains("client_id=Iv1.test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "device_code": "dev-secret", "user_code": "WDJB-MJHT",
            "verification_uri": "https://ghe.example/login/device",
            "expires_in": 900, "interval": 30
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"error": "authorization_pending"})),
        )
        .mount(&server)
        .await;
    let st = state();
    let rt = std::sync::Arc::new(runtime());

    let no_id = ghe(&st, &server, None);
    let err = service::device_start(&st, &rt, &no_id.id, None)
        .await
        .unwrap_err();
    match &err {
        AuthCommandError::MissingClientId {
            pat_url,
            pat_scopes,
            ..
        } => {
            assert!(pat_url.ends_with(
                "/settings/tokens/new?scopes=repo,workflow,read:user,user:email&description=NANOMultipleGitGUI"
            ));
            assert_eq!(pat_scopes, &["repo", "workflow", "read:user", "user:email"]);
        }
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(
        serde_json::to_value(&err).unwrap()["kind"],
        "missingClientId"
    );

    // A provider row with its own client id (GHE admins register one).
    st.storage.delete_provider(&no_id.id).unwrap();
    let p = ghe(&st, &server, Some("Iv1.test"));
    let start = service::device_start(&st, &rt, &p.id, None).await.unwrap();
    assert_eq!(start.prompt.user_code, "WDJB-MJHT");
    assert_eq!(rt.flow_count(), 1);

    let st = std::sync::Arc::new(st);
    let wait = {
        let (st, rt, id) = (st.clone(), rt.clone(), start.flow_id.clone());
        tokio::spawn(async move { service::flow_wait(&st, &rt, &id).await })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(rt.cancel(&start.flow_id));
    let result = tokio::time::timeout(Duration::from_secs(5), wait)
        .await
        .expect("wait returns promptly after cancel")
        .unwrap();
    assert!(matches!(
        result,
        Err(AuthCommandError::Auth {
            error: AuthError::Cancelled,
            ..
        })
    ));
    assert_eq!(rt.flow_count(), 0);
    assert!(!rt.cancel(&start.flow_id), "already gone");
    assert!(matches!(
        service::flow_wait(&st, &rt, &start.flow_id).await,
        Err(AuthCommandError::UnknownFlow { .. })
    ));
    assert!(st.storage.accounts().unwrap().is_empty());
}

#[tokio::test]
async fn device_flow_success_creates_oauth_account() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/login/device/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "device_code": "dev-secret", "user_code": "ABCD-EFGH",
            "verification_uri": "https://ghe.example/login/device",
            "expires_in": 900, "interval": 0
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .and(body_string_contains("device_code=dev-secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "gho_oauthtoken", "token_type": "bearer",
            "scope": "repo,read:user,user:email"
        })))
        .mount(&server)
        .await;
    mount_github_user(&server, "gho_oauthtoken", "repo, read:user, user:email").await;
    let st = state();
    let rt = runtime();
    let p = ghe(&st, &server, Some("Iv1.test"));
    let start = service::device_start(&st, &rt, &p.id, None).await.unwrap();
    let acc = service::flow_wait(&st, &rt, &start.flow_id).await.unwrap();
    assert_eq!(acc.auth_method, AuthMethod::Oauth);
    assert_eq!(acc.token_expires_at, None);
    assert_eq!(
        account_secret(&st, &acc.id, AccountSecret::Token)
            .unwrap()
            .unwrap()
            .as_str(),
        "gho_oauthtoken"
    );
    assert_eq!(
        st.storage.identities(Some(&acc.id)).unwrap()[0].email,
        "hoang@x.vn"
    );
    assert_eq!(rt.flow_count(), 0);
}

fn gitlab_oauth_account(st: &AppState, p: &Provider, expires_in_secs: i64) -> String {
    let acc = crate::accounts::commands::account_save_impl(
        st,
        crate::accounts::commands::AccountInput {
            id: None,
            provider_id: p.id.clone(),
            username: "lan".into(),
            display_name: None,
            avatar_url: None,
            auth_method: AuthMethod::Oauth,
            scopes: vec!["api".into()],
            token_expires_at: None,
            color: None,
            label: None,
            default_clone_dir: None,
        },
    )
    .unwrap();
    account_store_tokens(
        st,
        &acc.id,
        Zeroizing::new("old-access".into()),
        Some(Zeroizing::new("old-refresh".into())),
        Some((now_unix() + expires_in_secs) * 1000),
        vec!["api".into()],
    )
    .unwrap();
    acc.id
}

#[tokio::test]
async fn background_refresh_rotates_tokens_and_flags_rejection() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=old-refresh"))
        .and(body_string_contains("client_id=gl-app-id"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "new-access", "refresh_token": "new-refresh",
            "token_type": "Bearer", "expires_in": 7200, "scope": "api"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .and(body_string_contains("refresh_token=other-refresh"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": "invalid_grant", "error_description": "expired"
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .and(body_string_contains("refresh_token=new-refresh"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": "invalid_grant", "error_description": "revoked"
        })))
        .mount(&server)
        .await;
    let st = state();
    let rt = runtime();
    let p = gitlab(&st, &server);
    let soon = gitlab_oauth_account(&st, &p, 10 * 60);
    let later = gitlab_oauth_account_named(&st, &p, "minh", 60 * 60);

    let h = service::health(&st, &soon).unwrap();
    assert!(
        h.auto_refresh && !h.expires_soon && !h.needs_sign_in,
        "{h:?}"
    );

    assert_eq!(service::refresh_due(&st, &rt, now_unix()).await, 1);
    let secret = |id: &str, kind| {
        account_secret(&st, id, kind)
            .unwrap()
            .map(|s| s.as_str().to_owned())
    };
    assert_eq!(
        secret(&soon, AccountSecret::Token).as_deref(),
        Some("new-access")
    );
    assert_eq!(
        secret(&soon, AccountSecret::RefreshToken).as_deref(),
        Some("new-refresh"),
        "rotated refresh token stored"
    );
    assert_eq!(
        secret(&later, AccountSecret::RefreshToken).as_deref(),
        Some("other-refresh"),
        "token valid for an hour is left alone"
    );
    let acc = st.storage.account(&soon).unwrap().unwrap();
    assert!(acc.token_expires_at.unwrap() > (now_unix() + 7000) * 1000);
    // Nothing due anymore.
    assert_eq!(service::refresh_due(&st, &rt, now_unix()).await, 0);

    // A manual refresh with the (now revoked) rotated token marks "needs sign-in".
    let h = service::refresh_account(&st, &rt, &soon).await.unwrap();
    assert!(h.needs_sign_in, "{h:?}");
    assert!(h.last_error.as_deref().unwrap_or("").contains("revoked"));
    // Accounts that need sign-in are skipped by the background task.
    assert_eq!(
        service::refresh_due(&st, &rt, now_unix() + 2 * 3600).await,
        0,
        "`later` is due but its refresh token is rejected; `soon` is not retried"
    );
    assert!(service::health(&st, &later).unwrap().needs_sign_in);
}

fn gitlab_oauth_account_named(
    st: &AppState,
    p: &Provider,
    name: &str,
    expires_in_secs: i64,
) -> String {
    let acc = crate::accounts::commands::account_save_impl(
        st,
        crate::accounts::commands::AccountInput {
            id: None,
            provider_id: p.id.clone(),
            username: name.into(),
            display_name: None,
            avatar_url: None,
            auth_method: AuthMethod::Oauth,
            scopes: vec!["api".into()],
            token_expires_at: None,
            color: None,
            label: None,
            default_clone_dir: None,
        },
    )
    .unwrap();
    account_store_tokens(
        st,
        &acc.id,
        Zeroizing::new(format!("{name}-access")),
        Some(Zeroizing::new("other-refresh".into())),
        Some((now_unix() + expires_in_secs) * 1000),
        vec!["api".into()],
    )
    .unwrap();
    acc.id
}

#[tokio::test]
async fn pkce_start_uses_the_configured_loopback_port() {
    let server = MockServer::start().await;
    let st = state();
    let rt = runtime();
    let p = gitlab(&st, &server);
    assert!(service::set_loopback_port(&st, &p.id, 80).is_err());
    // A probed free port can be taken by a test running in parallel before `pkce_start` binds
    // it (the probe listener is closed first), so probe again in that case.
    let mut attempts = 0;
    let (free, start) = loop {
        let free = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        service::set_loopback_port(&st, &p.id, free).unwrap();
        match service::pkce_start(&st, &rt, &p.id, None).await {
            Ok(start) => break (free, start),
            Err(e) if attempts < 10 => {
                attempts += 1;
                eprintln!("loopback port {free} was taken meanwhile ({e:?}); retrying");
            }
            Err(e) => panic!("pkce_start failed: {e:?}"),
        }
    };
    let cfg = service::auth_config(&st, &p.id).unwrap();
    assert_eq!(
        cfg.redirect_uri,
        format!("http://127.0.0.1:{free}/callback")
    );
    assert!(cfg.pkce && cfg.has_client_id);
    assert!(cfg.pat_url.contains(
        "/-/user_settings/personal_access_tokens?name=NANOMultipleGitGUI&scopes=read_user,read_api,read_repository,write_repository"
    ));

    assert_eq!(start.redirect_uri, cfg.redirect_uri);
    assert!(start.authorize_url.contains("code_challenge_method=S256"));
    assert!(start.authorize_url.contains(&format!(
        "redirect_uri=http%3A%2F%2F127.0.0.1%3A{free}%2Fcallback"
    )));
    // The port is taken while the login runs; cancelling frees it.
    assert!(std::net::TcpListener::bind(("127.0.0.1", free)).is_err());
    assert!(rt.cancel(&start.flow_id));
    assert!(std::net::TcpListener::bind(("127.0.0.1", free)).is_ok());

    // PKCE is GitLab only.
    let other = MockServer::start().await;
    let gh = ghe(&st, &other, Some("Iv1.x"));
    assert!(matches!(
        service::pkce_start(&st, &rt, &gh.id, None).await,
        Err(AuthCommandError::Unsupported { .. })
    ));
}

#[test]
fn health_reports_expiry_and_missing_scopes() {
    let st = state();
    let gh = Provider::github_com();
    let now = 1_000_000;
    let mut acc = crate::accounts::commands::account_save_impl(
        &st,
        crate::accounts::commands::AccountInput {
            id: None,
            provider_id: gh.id.clone(),
            username: "u".into(),
            display_name: None,
            avatar_url: None,
            auth_method: AuthMethod::Pat,
            scopes: vec!["read:user".into()],
            token_expires_at: Some((now + 3 * 86_400 + 60) * 1000),
            color: None,
            label: None,
            default_clone_dir: None,
        },
    )
    .unwrap();
    acc.secrets = vec![AccountSecret::Token];
    let h = compute_health(&acc, &gh, &AuthStatus::default(), now);
    assert!(h.expires_soon && !h.expired && !h.needs_sign_in);
    assert_eq!(h.days_left, Some(3));
    let missing: Vec<_> = h.missing_scopes.iter().map(|m| m.feature).collect();
    assert_eq!(
        missing,
        [
            Feature::GitHttps,
            Feature::ListRepositories,
            Feature::ReadEmails
        ]
    );

    acc.token_expires_at = Some((now - 1) * 1000);
    let h = compute_health(&acc, &gh, &AuthStatus::default(), now);
    assert!(h.expired && h.needs_sign_in);

    // Fine-grained PAT: scopes unknown → no "missing scope" noise.
    let status = AuthStatus {
        scopes_known: Some(false),
        ..AuthStatus::default()
    };
    acc.token_expires_at = None;
    let h = compute_health(&acc, &gh, &status, now);
    assert!(!h.scopes_known && h.missing_scopes.is_empty());
}

#[test]
fn identity_email_preference() {
    let user = ProviderUser {
        id: "7".into(),
        username: "lan".into(),
        display_name: None,
        avatar_url: None,
        profile_url: None,
        email: None,
    };
    let e = |email: &str, verified, primary, commit_email| ProviderEmail {
        email: email.into(),
        verified,
        primary,
        commit_email,
    };
    let list = [
        e("a@x", false, true, false),
        e("b@x", true, false, true),
        e("c@x", true, false, false),
    ];
    assert_eq!(
        pick_identity_email(&list, &user, ProviderFamily::Gitlab, "gitlab.com"),
        "b@x"
    );
    assert_eq!(
        pick_identity_email(&[], &user, ProviderFamily::Github, "github.com"),
        "7+lan@users.noreply.github.com"
    );
    assert_eq!(
        pick_identity_email(&[], &user, ProviderFamily::Gitlab, "git.acme.vn:8443"),
        "7-lan@users.noreply.git.acme.vn"
    );
}

#[test]
fn errors_serialize_with_kind_and_message() {
    let e = AuthCommandError::from(AuthError::Denied);
    let v = serde_json::to_value(&e).unwrap();
    assert_eq!(v["kind"], "auth");
    assert_eq!(v["error"]["kind"], "denied");
    assert!(v["message"].as_str().unwrap().contains("declined"));
    let e = AuthCommandError::from(crate::error::AppError::InvalidInput("x".into()));
    assert_eq!(serde_json::to_value(&e).unwrap()["kind"], "app");
}
