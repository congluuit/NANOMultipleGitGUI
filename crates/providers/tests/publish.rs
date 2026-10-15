//! Creating an empty repository to publish a local one, against a wiremock server.

use nanomgg_providers::forks::{ForkProblem, fork_problem};
use nanomgg_providers::publish::NewRepoOptions;
use nanomgg_providers::*;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn github(server: &MockServer) -> ProviderClient {
    let endpoint = ProviderEndpoint::custom(
        ProviderKind::GithubEnterprise,
        &server.uri(),
        &format!("{}/api/v3", server.uri()),
    )
    .unwrap();
    ProviderClient::new(
        build_http_client(&HttpConfig::default()).unwrap(),
        endpoint,
        SecretToken::new("ghp_testtoken"),
    )
}

fn gitlab(server: &MockServer) -> ProviderClient {
    ProviderClient::new(
        build_http_client(&HttpConfig::default()).unwrap(),
        ProviderEndpoint::gitlab_self_hosted(&server.uri()).unwrap(),
        SecretToken::new("glpat-testtoken"),
    )
}

fn opts(name: &str, namespace: Option<&str>, private: bool) -> NewRepoOptions {
    NewRepoOptions {
        name: name.into(),
        namespace: namespace.map(Into::into),
        description: Some("  My app ".into()),
        private,
    }
}

async fn github_user(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"id": 1, "login": "hoangnv"})),
        )
        .mount(server)
        .await;
}

#[tokio::test]
async fn github_creates_in_the_user_or_an_organization() {
    let server = MockServer::start().await;
    github_user(&server).await;
    Mock::given(method("POST"))
        .and(path("/api/v3/user/repos"))
        .and(body_json(json!({
            "name": "app", "private": true, "auto_init": false, "description": "My app"
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"full_name": "hoangnv/app"})))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v3/orgs/acme/repos"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"full_name": "acme/app"})))
        .expect(1)
        .mount(&server)
        .await;
    let c = github(&server);
    assert_eq!(
        c.create_repository(&opts("app", None, true)).await.unwrap(),
        "hoangnv/app"
    );
    // The account's own login is the user namespace, not an organization.
    assert_eq!(
        c.create_repository(&opts("app", Some("HoangNV"), true))
            .await
            .unwrap(),
        "hoangnv/app"
    );
    assert_eq!(
        c.create_repository(&opts("app", Some("acme"), false))
            .await
            .unwrap(),
        "acme/app"
    );
}

#[tokio::test]
async fn github_name_taken_is_recognised() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v3/user/repos"))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({
            "message": "Repository creation failed.",
            "errors": [{"resource": "Repository", "code": "custom", "field": "name",
                        "message": "name already exists on this account"}]
        })))
        .mount(&server)
        .await;
    let err = github(&server)
        .create_repository(&opts("app", None, false))
        .await
        .unwrap_err();
    assert_eq!(fork_problem(&err), Some(ForkProblem::NameTaken));
}

#[tokio::test]
async fn invalid_names_are_refused_without_a_request() {
    let server = MockServer::start().await;
    let err = github(&server)
        .create_repository(&opts("my app", None, false))
        .await
        .unwrap_err();
    assert!(matches!(err, ProviderError::InvalidInput { .. }));
}

#[tokio::test]
async fn gitlab_creates_in_the_user_or_a_group() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v4/projects"))
        .and(body_json(json!({
            "name": "app", "path": "app", "visibility": "private",
            "initialize_with_readme": false, "description": "My app"
        })))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(json!({"path_with_namespace": "hoang/app"})),
        )
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/namespaces/hoang"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": 5, "kind": "user"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/namespaces/acme%2Fteam"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": 42, "kind": "group"})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v4/projects"))
        .and(body_json(json!({
            "name": "app", "path": "app", "visibility": "public",
            "initialize_with_readme": false, "description": "My app", "namespace_id": 42
        })))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_json(json!({"path_with_namespace": "acme/team/app"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let c = gitlab(&server);
    assert_eq!(
        c.create_repository(&opts("app", None, true)).await.unwrap(),
        "hoang/app"
    );
    assert_eq!(
        c.create_repository(&opts("app", Some("hoang"), true))
            .await
            .unwrap(),
        "hoang/app"
    );
    assert_eq!(
        c.create_repository(&opts("app", Some("acme/team"), false))
            .await
            .unwrap(),
        "acme/team/app"
    );
}
