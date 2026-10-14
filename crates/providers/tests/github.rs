//! GitHub client against a wiremock server (no real network).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use nanomgg_providers::*;
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TOKEN: &str = "ghp_testtoken0123456789abcdefghijklmnop";

fn client(server: &MockServer) -> ProviderClient {
    client_with(server, HttpConfig::default())
}

fn client_with(server: &MockServer, cfg: HttpConfig) -> ProviderClient {
    let endpoint = ProviderEndpoint::custom(
        ProviderKind::GithubEnterprise,
        &server.uri(),
        &format!("{}/api/v3", server.uri()),
    )
    .unwrap();
    ProviderClient::new(
        build_http_client(&cfg).unwrap(),
        endpoint,
        SecretToken::new(TOKEN),
    )
}

fn repo(id: u64, full: &str, perms: serde_json::Value) -> serde_json::Value {
    let (owner, name) = full.split_once('/').unwrap();
    json!({
        "id": id, "name": name, "full_name": full, "owner": {"login": owner},
        "private": true, "visibility": "private", "description": "", "fork": false,
        "archived": false, "default_branch": "main",
        "clone_url": format!("https://github.com/{full}.git"),
        "ssh_url": format!("git@github.com:{full}.git"),
        "html_url": format!("https://github.com/{full}"),
        "updated_at": "2024-01-01T00:00:00Z", "pushed_at": "2024-02-01T00:00:00Z",
        "stargazers_count": 12, "size": 2, "permissions": perms
    })
}

#[tokio::test]
async fn current_user_and_emails_send_auth_and_user_agent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .and(header("user-agent", USER_AGENT))
        .and(header("x-github-api-version", "2022-11-28"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 42, "login": "hoangnv", "name": "Hoàng", "avatar_url": "https://a/1.png",
            "html_url": "https://github.com/hoangnv", "email": null
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user/emails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"email": "a@x.vn", "primary": true, "verified": true, "visibility": "private"},
            {"email": "b@x.vn", "primary": false, "verified": false, "visibility": null}
        ])))
        .mount(&server)
        .await;

    let c = client(&server);
    let me = c.current_user().await.unwrap();
    assert_eq!(me.id, "42");
    assert_eq!(me.username, "hoangnv");
    assert_eq!(me.display_name.as_deref(), Some("Hoàng"));
    assert_eq!(me.email, None);
    let emails = c.emails().await.unwrap();
    assert_eq!(emails.len(), 2);
    assert!(emails[0].primary && emails[0].verified);
    assert!(!emails[1].verified);
}

#[tokio::test]
async fn lists_all_pages_via_link_header() {
    let server = MockServer::start().await;
    let next = format!(
        "<{}/api/v3/user/repos?per_page=2&page=2>; rel=\"next\", <{}/api/v3/user/repos?per_page=2&page=2>; rel=\"last\"",
        server.uri(),
        server.uri()
    );
    Mock::given(method("GET"))
        .and(path("/api/v3/user/repos"))
        .and(query_param("page", "1"))
        .and(query_param("per_page", "2"))
        .and(query_param(
            "affiliation",
            "owner,collaborator,organization_member",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("link", next.as_str())
                .set_body_json(json!([
                    repo(
                        1,
                        "hoangnv/dotfiles",
                        json!({"admin": true, "push": true, "pull": true})
                    ),
                    repo(
                        2,
                        "acme/api",
                        json!({"admin": false, "push": true, "pull": true})
                    ),
                ])),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user/repos"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([repo(
            3,
            "oss/lib",
            json!({"admin": false, "push": false, "pull": true})
        )])))
        .expect(1)
        .mount(&server)
        .await;

    let pages = AtomicUsize::new(0);
    let listing = list_all_repositories(
        &client(&server),
        &ListOptions {
            per_page: 2,
            max_pages: 10,
        },
        &CancellationToken::new(),
        |page| {
            pages.fetch_add(1, Ordering::SeqCst);
            assert!(!page.is_empty());
        },
    )
    .await
    .unwrap();
    assert_eq!(pages.load(Ordering::SeqCst), 2);
    assert!(!listing.truncated);
    let names: Vec<_> = listing.repos.iter().map(|r| r.full_name.as_str()).collect();
    assert_eq!(names, ["hoangnv/dotfiles", "acme/api", "oss/lib"]);
    let r = &listing.repos[1];
    assert_eq!(r.owner, "acme");
    assert_eq!(r.permissions.level, AccessLevel::Write);
    assert!(r.permissions.can_push && !r.permissions.can_admin);
    assert_eq!(r.size_bytes, Some(2048));
    assert_eq!(r.updated_at, parse_timestamp("2024-02-01T00:00:00Z"));
    assert_eq!(r.clone_ssh, "git@github.com:acme/api.git");
    assert_eq!(listing.repos[0].permissions.level, AccessLevel::Admin);
    assert_eq!(listing.repos[2].permissions.level, AccessLevel::Read);
    assert!(!listing.repos[2].permissions.can_push);
}

#[tokio::test]
async fn max_pages_truncates() {
    let server = MockServer::start().await;
    let next = format!("<{}/api/v3/user/repos?page=2>; rel=\"next\"", server.uri());
    Mock::given(method("GET"))
        .and(path("/api/v3/user/repos"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("link", next.as_str())
                .set_body_json(json!([repo(1, "a/b", json!({"pull": true}))])),
        )
        .mount(&server)
        .await;
    let listing = list_all_repositories(
        &client(&server),
        &ListOptions {
            per_page: 100,
            max_pages: 1,
        },
        &CancellationToken::new(),
        |_| {},
    )
    .await
    .unwrap();
    assert!(listing.truncated);
    assert_eq!(listing.repos.len(), 1);
}

#[tokio::test]
async fn listing_is_cancellable() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user/repos"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(10))
                .set_body_json(json!([])),
        )
        .mount(&server)
        .await;
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        trigger.cancel();
    });
    let started = std::time::Instant::now();
    let err = list_all_repositories(&client(&server), &ListOptions::default(), &cancel, |_| {})
        .await
        .unwrap_err();
    assert_eq!(err, ProviderError::Cancelled);
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn search_reports_total_and_next_page() {
    let server = MockServer::start().await;
    let next = format!(
        "<{}/api/v3/search/repositories?q=lib&page=2>; rel=\"next\"",
        server.uri()
    );
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .and(query_param("q", "lib language:rust"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("link", next.as_str())
                .set_body_json(json!({
                    "total_count": 321, "incomplete_results": false,
                    "items": [repo(9, "rust-lang/lib", serde_json::Value::Null)]
                })),
        )
        .mount(&server)
        .await;
    let page = client(&server)
        .search_repositories("lib language:rust", 1, 30)
        .await
        .unwrap();
    assert_eq!(page.total, Some(321));
    assert_eq!(page.next_page, Some(2));
    // No `permissions` in search results → at least readable.
    assert_eq!(page.repos[0].permissions.level, AccessLevel::Read);
    assert!(
        client(&server)
            .search_repositories("  ", 1, 30)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn permission_for_remote_maps_flags_and_checks_host() {
    let server = MockServer::start().await;
    let mut parent = repo(
        5,
        "me/fork",
        json!({"admin": false, "maintain": true, "push": true}),
    );
    parent["fork"] = json!(true);
    parent["parent"] = json!({"full_name": "upstream/fork"});
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/me/fork"))
        .respond_with(ResponseTemplate::new(200).set_body_json(parent))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/secret"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})))
        .mount(&server)
        .await;

    let c = client(&server);
    let host = server.address().ip().to_string();
    let p = c
        .repo_permission_for_remote(&format!("git@{host}:me/fork.git"))
        .await
        .unwrap();
    assert_eq!(p.level, AccessLevel::Maintain);
    assert!(p.can_push && p.can_admin);
    let r = c.repository("me/fork").await.unwrap();
    assert!(r.fork);
    assert_eq!(r.parent_full_name.as_deref(), Some("upstream/fork"));

    let e = c
        .repo_permission_for_remote(&format!("{}/acme/secret.git", server.uri()))
        .await
        .unwrap_err();
    assert!(matches!(e, ProviderError::NotFound { .. }), "{e:?}");

    let e = c
        .repo_permission_for_remote("https://gitlab.com/acme/api.git")
        .await
        .unwrap_err();
    assert!(matches!(e, ProviderError::HostMismatch { .. }), "{e:?}");
    assert!(c.repo_permission_for_remote("not a url").await.is_err());
}

#[tokio::test]
async fn ssh_keys_upload_list_and_duplicate() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v3/user/keys"))
        .and(body_json(
            json!({"title": "NANO laptop", "key": "ssh-ed25519 AAAAC3Nz me@pc"}),
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 7, "key": "ssh-ed25519 AAAAC3Nz", "title": "NANO laptop",
            "created_at": "2024-05-01T10:00:00Z", "verified": true, "read_only": false
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v3/user/keys"))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({
            "message": "Validation Failed",
            "errors": [{"resource": "PublicKey", "code": "custom", "field": "key", "message": "key is already in use"}]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user/keys"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 7, "key": "ssh-ed25519 AAAAC3Nz", "title": "NANO laptop", "created_at": "2024-05-01T10:00:00Z"}
        ])))
        .mount(&server)
        .await;

    let c = client(&server);
    let key = c
        .add_ssh_key("NANO laptop", " ssh-ed25519 AAAAC3Nz me@pc\n")
        .await
        .unwrap();
    assert_eq!(key.id, "7");
    assert_eq!(key.created_at, parse_timestamp("2024-05-01T10:00:00Z"));
    let err = c
        .add_ssh_key("NANO laptop", "ssh-ed25519 AAAAC3Nz me@pc")
        .await
        .unwrap_err();
    assert!(
        matches!(err, ProviderError::AlreadyExists { ref message } if message.contains("already in use")),
        "{err:?}"
    );
    let keys = c.list_ssh_keys().await.unwrap();
    assert!(keys[0].matches("ssh-ed25519 AAAAC3Nz other-comment"));
}

#[tokio::test]
async fn http_errors_are_typed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message": "Bad credentials"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user/emails"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "1999999999")
                .set_body_json(json!({"message": "API rate limit exceeded for user."})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme-corp/api"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "message": "Although you appear to have the correct authorization credentials, the `acme-corp` organization has enabled OAuth App access restrictions, meaning that data access to third-parties is limited. For more information on these restrictions, including how to enable this app, visit https://docs.github.com/articles/restricting-access-to-your-organization-s-data/"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user/keys"))
        .respond_with(ResponseTemplate::new(503).set_body_string("upstream down"))
        .mount(&server)
        .await;

    let c = client(&server);
    let e = c.current_user().await.unwrap_err();
    assert_eq!(
        e,
        ProviderError::Unauthorized {
            message: "Bad credentials".into()
        }
    );
    let e = c.emails().await.unwrap_err();
    assert_eq!(
        e,
        ProviderError::RateLimited {
            reset_at: Some(1_999_999_999),
            retry_after_secs: None
        }
    );
    let e = c.repo_permission("acme-corp/api").await.unwrap_err();
    assert!(e.suggests_fine_grained_pat(), "{e:?}");
    assert_eq!(
        e.hint(),
        Some(&AccessHint::OauthAppRestricted {
            organization: Some("acme-corp".into())
        })
    );
    let e = c.list_ssh_keys().await.unwrap_err();
    assert!(
        matches!(e, ProviderError::Server { status: 503, .. }),
        "{e:?}"
    );
    assert!(e.is_transient());
    // The token never shows up in errors.
    assert!(!format!("{e:?} {e}").contains(TOKEN));
}

#[tokio::test]
async fn timeouts_and_network_errors() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(5))
                .set_body_json(json!({"id": 1, "login": "x"})),
        )
        .mount(&server)
        .await;
    let c = client_with(
        &server,
        HttpConfig {
            timeout: Duration::from_millis(200),
            ..HttpConfig::default()
        },
    );
    assert_eq!(c.current_user().await.unwrap_err(), ProviderError::Timeout);

    // A port nobody listens on any more → connection refused. (wiremock pools its servers, so
    // a dropped MockServer would still answer.)
    let dead = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", l.local_addr().unwrap())
    };
    let endpoint = ProviderEndpoint::gitlab_self_hosted(&dead).unwrap();
    let c = ProviderClient::new(
        build_http_client(&HttpConfig::default()).unwrap(),
        endpoint,
        SecretToken::new("glpat-x"),
    );
    let e = c.current_user().await.unwrap_err();
    assert!(matches!(e, ProviderError::Network { .. }), "{e:?}");
}

#[tokio::test]
async fn enterprise_version_from_meta() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/meta"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"installed_version": "3.14.2"})),
        )
        .mount(&server)
        .await;
    let v = client(&server).server_version().await.unwrap().unwrap();
    assert_eq!((v.major, v.minor), (3, 14));
    let gh = ProviderClient::new(
        build_http_client(&HttpConfig::default()).unwrap(),
        ProviderEndpoint::github(),
        SecretToken::new(TOKEN),
    );
    // github.com: no request at all.
    assert_eq!(gh.server_version().await.unwrap(), None);
}
