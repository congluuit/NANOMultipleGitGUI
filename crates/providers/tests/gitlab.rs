//! GitLab client against a wiremock server (no real network).

use nanomgg_providers::*;
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TOKEN: &str = "glpat-testtoken0123456789abcdef";

fn client(server: &MockServer) -> ProviderClient {
    let endpoint = ProviderEndpoint::gitlab_self_hosted(&server.uri()).unwrap();
    assert_eq!(endpoint.kind(), ProviderKind::GitlabSelfHosted);
    ProviderClient::new(
        build_http_client(&HttpConfig::default()).unwrap(),
        endpoint,
        SecretToken::new(TOKEN),
    )
}

fn project(
    id: u64,
    full: &str,
    project_level: Option<u32>,
    group_level: Option<u32>,
) -> serde_json::Value {
    let (ns, name) = full.rsplit_once('/').unwrap();
    json!({
        "id": id, "name": name.to_uppercase(), "path": name, "path_with_namespace": full,
        "namespace": {"full_path": ns}, "description": "Game assets", "visibility": "private",
        "archived": false, "default_branch": "main",
        "http_url_to_repo": format!("https://gitlab.acme.vn/{full}.git"),
        "ssh_url_to_repo": format!("git@gitlab.acme.vn:{full}.git"),
        "web_url": format!("https://gitlab.acme.vn/{full}"),
        "last_activity_at": "2024-03-01T08:00:00.000+07:00",
        "star_count": 3, "lfs_enabled": true,
        "statistics": {"repository_size": 1000, "lfs_objects_size": 4_200_000_000u64},
        "permissions": {
            "project_access": project_level.map(|l| json!({"access_level": l})),
            "group_access": group_level.map(|l| json!({"access_level": l}))
        }
    })
}

#[tokio::test]
async fn user_and_merged_emails() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/user"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 9, "username": "hoang", "name": "Hoàng", "avatar_url": "https://g/a.png",
            "web_url": "https://gitlab.acme.vn/hoang", "email": "hoang@acme.vn",
            "commit_email": "9-hoang@users.noreply.gitlab.acme.vn", "public_email": "",
            "confirmed_at": "2020-01-01T00:00:00Z"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/user/emails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 1, "email": "hoang@gmail.com", "confirmed_at": "2021-01-01T00:00:00Z"},
            {"id": 2, "email": "old@acme.vn", "confirmed_at": null}
        ])))
        .mount(&server)
        .await;
    let c = client(&server);
    let me = c.current_user().await.unwrap();
    assert_eq!((me.id.as_str(), me.username.as_str()), ("9", "hoang"));
    assert_eq!(me.email.as_deref(), Some("hoang@acme.vn"));
    let emails = c.emails().await.unwrap();
    let summary: Vec<_> = emails
        .iter()
        .map(|e| (e.email.as_str(), e.verified, e.primary, e.commit_email))
        .collect();
    assert_eq!(
        summary,
        [
            ("hoang@acme.vn", true, true, false),
            ("hoang@gmail.com", true, false, false),
            ("old@acme.vn", false, false, false),
            ("9-hoang@users.noreply.gitlab.acme.vn", true, false, true),
        ]
    );
}

#[tokio::test]
async fn paginates_with_x_next_page_and_maps_fields() {
    let server = MockServer::start().await;
    let mut fork = project(3, "hoang/lib", Some(50), None);
    fork["forked_from_project"] = json!({"path_with_namespace": "oss/lib"});
    Mock::given(method("GET"))
        .and(path("/api/v4/projects"))
        .and(query_param("membership", "true"))
        .and(query_param("statistics", "true"))
        .and(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-next-page", "2")
                .set_body_json(json!([
                    project(1, "acme/games/assets", Some(30), Some(40)),
                    project(2, "acme/backend", Some(20), None),
                ])),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects"))
        .and(query_param("page", "2"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-next-page", "")
                .set_body_json(json!([fork])),
        )
        .expect(1)
        .mount(&server)
        .await;

    let listing = list_all_repositories(
        &client(&server),
        &ListOptions::default(),
        &CancellationToken::new(),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(listing.repos.len(), 3);
    let assets = &listing.repos[0];
    assert_eq!(assets.full_name, "acme/games/assets");
    assert_eq!(assets.owner, "acme/games");
    assert_eq!(assets.name, "assets");
    // max(project 30, group 40) → Maintainer
    assert_eq!(assets.permissions.level, AccessLevel::Maintain);
    assert!(assets.permissions.can_push && assets.permissions.can_admin);
    assert_eq!(assets.lfs_enabled, Some(true));
    assert_eq!(assets.lfs_size_bytes, Some(4_200_000_000));
    assert_eq!(assets.updated_at, parse_timestamp("2024-03-01T01:00:00Z"));
    assert!(assets.private);
    let backend = &listing.repos[1];
    assert_eq!(backend.permissions.level, AccessLevel::Read);
    assert!(!backend.permissions.can_push);
    let fork = &listing.repos[2];
    assert!(fork.fork);
    assert_eq!(fork.parent_full_name.as_deref(), Some("oss/lib"));
    assert_eq!(fork.permissions.level, AccessLevel::Admin);
}

#[tokio::test]
async fn permission_uses_encoded_project_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/grp%2Fsub%2Fproj"))
        .respond_with(ResponseTemplate::new(200).set_body_json(project(
            4,
            "grp/sub/proj",
            Some(30),
            None,
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/grp%2Fpublic"))
        .respond_with(ResponseTemplate::new(200).set_body_json({
            let mut p = project(5, "grp/public", None, None);
            p["visibility"] = json!("public");
            p["permissions"] = json!({"project_access": null, "group_access": null});
            p
        }))
        .mount(&server)
        .await;
    let c = client(&server);
    let host = server.address().ip().to_string();
    let p = c
        .repo_permission_for_remote(&format!("ssh://git@{host}:2222/grp/sub/proj.git"))
        .await
        .unwrap();
    assert_eq!(p.level, AccessLevel::Write);
    assert!(p.can_push && !p.can_admin);
    let p = c.repo_permission("grp/public").await.unwrap();
    assert_eq!(p.level, AccessLevel::Read);
    assert!(!p.can_push);
}

#[tokio::test]
async fn ssh_keys_and_duplicates() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v4/user/keys"))
        .and(body_json(
            json!({"title": "laptop", "key": "ssh-ed25519 AAAAC3Nz me"}),
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 11, "title": "laptop", "key": "ssh-ed25519 AAAAC3Nz me",
            "created_at": "2024-05-01T10:00:00.000Z", "expires_at": "2025-05-01T00:00:00.000Z",
            "usage_type": "auth_and_signing"
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v4/user/keys"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "message": {"key": ["has already been taken"], "fingerprint_sha256": ["has already been taken"]}
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/user/keys"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let c = client(&server);
    let k = c
        .add_ssh_key("laptop", "ssh-ed25519 AAAAC3Nz me")
        .await
        .unwrap();
    assert_eq!(k.id, "11");
    assert_eq!(k.expires_at, parse_timestamp("2025-05-01T00:00:00Z"));
    let e = c
        .add_ssh_key("laptop", "ssh-ed25519 AAAAC3Nz me")
        .await
        .unwrap_err();
    assert_eq!(e.code(), "alreadyExists");
    assert!(c.list_ssh_keys().await.unwrap().is_empty());
}

#[tokio::test]
async fn version_scope_and_rate_limit_errors() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/version"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"version": "16.11.2-ee", "revision": "abc123"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/user/keys"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header(
                    "www-authenticate",
                    "Bearer realm=\"\", error=\"insufficient_scope\", error_description=\"higher privileges\", scope=\"api\"",
                )
                .set_body_json(json!({"error": "insufficient_scope", "error_description": "higher privileges", "scope": "api"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "60")
                .insert_header("ratelimit-reset", "1999999999")
                .set_body_string("Retry later"),
        )
        .mount(&server)
        .await;
    let c = client(&server);
    let v = c.server_version().await.unwrap().unwrap();
    assert_eq!(
        (v.major, v.minor, v.revision.as_deref()),
        (16, 11, Some("abc123"))
    );
    assert!(v.at_least(15, 5));
    let e = c.list_ssh_keys().await.unwrap_err();
    assert_eq!(
        e.hint(),
        Some(&AccessHint::MissingScopes {
            required: vec!["api".into()],
            granted: None
        })
    );
    let e = c.list_repositories_page(1, 100).await.unwrap_err();
    assert_eq!(
        e,
        ProviderError::RateLimited {
            reset_at: Some(1_999_999_999),
            retry_after_secs: Some(60)
        }
    );
    let e = c.search_repositories("assets", 1, 20).await.unwrap_err();
    assert_eq!(e.code(), "rateLimited");
}
