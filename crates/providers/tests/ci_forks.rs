//! CI status and forks (Phase 4.3) against a wiremock server (no real network).

use std::time::Duration;

use nanomgg_providers::ci::{CiJobKind, CiJobState, CiState};
use nanomgg_providers::forks::{
    ForkNamespaceKind, ForkOptions, ForkProblem, ForkReadiness, fork_problem,
};
use nanomgg_providers::*;
use serde_json::json;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

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

fn check_run(name: &str, status: &str, conclusion: Option<&str>) -> serde_json::Value {
    json!({
        "name": name, "status": status, "conclusion": conclusion,
        "started_at": "2024-05-01T10:00:00Z", "completed_at": "2024-05-01T10:02:00Z",
        "html_url": format!("https://github.com/acme/api/runs/{name}"),
        "app": {"name": "GitHub Actions"}
    })
}

fn gh_paths(sha: &str) -> (String, String) {
    (
        format!("/api/v3/repos/acme/api/commits/{sha}/check-runs"),
        format!("/api/v3/repos/acme/api/commits/{sha}/status"),
    )
}

#[tokio::test]
async fn github_merges_check_runs_and_statuses() {
    let server = MockServer::start().await;
    let (checks, status) = gh_paths(SHA);
    Mock::given(method("GET"))
        .and(path(checks.as_str()))
        .and(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "link",
                    format!(
                        r#"<{}{checks}?per_page=100&page=2>; rel="next""#,
                        server.uri()
                    )
                    .as_str(),
                )
                .set_body_json(json!({
                    "total_count": 2,
                    "check_runs": [check_run("build", "completed", Some("success"))]
                })),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(checks.as_str()))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 2,
            "check_runs": [check_run("test", "in_progress", None)]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(status.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "state": "success",
            "statuses": [{
                "context": "ci/jenkins", "state": "success", "description": "Build passed",
                "target_url": "https://ci.acme.vn/1", "created_at": "2024-05-01T10:00:00Z",
                "updated_at": "2024-05-01T10:05:00Z"
            }]
        })))
        .mount(&server)
        .await;

    let ci = github(&server)
        .ci_status("acme/api", SHA, false)
        .await
        .unwrap();
    assert_eq!(ci.state, CiState::Pending);
    assert_eq!(ci.jobs.len(), 3);
    assert!(!ci.truncated);
    assert!(ci.checks_error.is_none());
    let build = &ci.jobs[0];
    assert_eq!(build.kind, CiJobKind::Check);
    assert_eq!(build.state, CiJobState::Success);
    assert_eq!(build.group.as_deref(), Some("GitHub Actions"));
    assert_eq!(build.finished_at.unwrap() - build.started_at.unwrap(), 120);
    assert_eq!(ci.jobs[1].state, CiJobState::Running);
    let jenkins = &ci.jobs[2];
    assert_eq!(jenkins.kind, CiJobKind::Status);
    assert_eq!(jenkins.description.as_deref(), Some("Build passed"));
    assert_eq!(jenkins.url.as_deref(), Some("https://ci.acme.vn/1"));
}

#[tokio::test]
async fn github_without_any_ci_is_none_not_pending() {
    let server = MockServer::start().await;
    let (checks, status) = gh_paths(SHA);
    Mock::given(method("GET"))
        .and(path(checks.as_str()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"total_count": 0, "check_runs": []})),
        )
        .mount(&server)
        .await;
    // GitHub reports "pending" for a commit without statuses.
    Mock::given(method("GET"))
        .and(path(status.as_str()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"state": "pending", "statuses": []})),
        )
        .mount(&server)
        .await;
    let ci = github(&server)
        .ci_status("acme/api", SHA, true)
        .await
        .unwrap();
    assert_eq!(ci.state, CiState::None);
}

#[tokio::test]
async fn github_fine_grained_token_without_checks_keeps_statuses() {
    let server = MockServer::start().await;
    let (checks, status) = gh_paths(SHA);
    Mock::given(method("GET"))
        .and(path(checks.as_str()))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(
                json!({"message": "Resource not accessible by personal access token"}),
            ),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(status.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "state": "failure",
            "statuses": [{"context": "ci", "state": "error", "target_url": null}]
        })))
        .mount(&server)
        .await;
    let ci = github(&server)
        .ci_status("acme/api", SHA, true)
        .await
        .unwrap();
    assert_eq!(ci.state, CiState::Failure);
    match ci.checks_error {
        Some(ProviderError::Forbidden { hint, .. }) => {
            assert_eq!(hint, Some(AccessHint::TokenPermissionMissing));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn github_unknown_commit_is_none_and_hidden_repo_is_an_error() {
    let server = MockServer::start().await;
    let (checks, status) = gh_paths(SHA);
    Mock::given(method("GET"))
        .and(path(checks.as_str()))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(json!({"message": "No commit found for SHA: 0123"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(status.as_str()))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(json!({"message": "No commit found for SHA: 0123"})),
        )
        .mount(&server)
        .await;
    let ci = github(&server)
        .ci_status("acme/api", SHA, true)
        .await
        .unwrap();
    assert_eq!(ci.state, CiState::None);

    let other = "fedcba9876543210fedcba9876543210fedcba98";
    let (checks, status) = gh_paths(other);
    for p in [checks, status] {
        Mock::given(method("GET"))
            .and(path(p.as_str()))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})))
            .mount(&server)
            .await;
    }
    let err = github(&server)
        .ci_status("acme/api", other, true)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "notFound");
}

#[tokio::test]
async fn ci_errors_rate_limit_and_unauthorized() {
    let server = MockServer::start().await;
    let (checks, status) = gh_paths(SHA);
    for p in [checks, status] {
        Mock::given(method("GET"))
            .and(path(p.as_str()))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("x-ratelimit-remaining", "0")
                    .insert_header("x-ratelimit-reset", "1999999999")
                    .set_body_json(json!({"message": "API rate limit exceeded"})),
            )
            .mount(&server)
            .await;
    }
    let err = github(&server)
        .ci_status("acme/api", SHA, true)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        ProviderError::RateLimited {
            reset_at: Some(1_999_999_999),
            ..
        }
    ));

    let gl = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/acme%2Fapi/pipelines"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message": "401 Unauthorized"})),
        )
        .mount(&gl)
        .await;
    let err = gitlab(&gl)
        .ci_status("acme/api", SHA, true)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "unauthorized");
    assert_eq!(
        github(&server)
            .ci_status("acme/api", "not-a-sha", true)
            .await
            .unwrap_err()
            .code(),
        "invalidInput"
    );
}

#[tokio::test]
async fn gitlab_newest_pipeline_and_jobs() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/grp%2Fsub%2Fapi/pipelines"))
        .and(query_param("sha", SHA))
        .and(query_param("per_page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": 77, "status": "failed", "web_url": "https://gitlab.acme.vn/grp/sub/api/-/pipelines/77"
        }])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/grp%2Fsub%2Fapi/pipelines/77/jobs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name": "lint", "stage": "test", "status": "failed", "allow_failure": true,
             "started_at": "2024-05-01T10:00:00Z", "finished_at": "2024-05-01T10:01:00Z",
             "web_url": "https://gitlab.acme.vn/grp/sub/api/-/jobs/1"},
            {"name": "unit", "stage": "test", "status": "failed", "allow_failure": false,
             "web_url": "https://gitlab.acme.vn/grp/sub/api/-/jobs/2"},
            {"name": "deploy", "stage": "deploy", "status": "manual"}
        ])))
        .expect(1)
        .mount(&server)
        .await;
    let c = gitlab(&server);
    let ci = c.ci_status("grp/sub/api", SHA, true).await.unwrap();
    assert_eq!(ci.state, CiState::Failure);
    assert_eq!(
        ci.pipeline_url.as_deref(),
        Some("https://gitlab.acme.vn/grp/sub/api/-/pipelines/77")
    );
    assert_eq!(ci.jobs.len(), 3);
    assert!(ci.jobs[0].allow_failure);
    assert_eq!(ci.jobs[0].group.as_deref(), Some("test"));
    assert_eq!(ci.jobs[2].state, CiJobState::Manual);
    // Without jobs: one request (the jobs mock expects exactly one call overall).
    let badge = c.ci_status("grp/sub/api", SHA, false).await.unwrap();
    assert_eq!(badge.state, CiState::Failure);
    assert!(badge.jobs.is_empty());
}

#[tokio::test]
async fn gitlab_no_pipeline_is_none() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/acme%2Fapi/pipelines"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let ci = gitlab(&server)
        .ci_status("acme/api", SHA, true)
        .await
        .unwrap();
    assert_eq!(ci.state, CiState::None);
}

// ----- forks ---------------------------------------------------------------------------------

#[tokio::test]
async fn github_namespaces_include_user_and_paged_orgs() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"id": 1, "login": "hoangnv", "name": "Hoàng", "avatar_url": "https://a/1"}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user/orgs"))
        .and(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "link",
                    format!(
                        r#"<{}/api/v3/user/orgs?per_page=100&page=2>; rel="next""#,
                        server.uri()
                    )
                    .as_str(),
                )
                .set_body_json(json!([{"login": "acme", "avatar_url": null}])),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user/orgs"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"login": "oss"}])))
        .mount(&server)
        .await;
    let ns = github(&server).fork_namespaces().await.unwrap();
    let paths: Vec<_> = ns.iter().map(|n| (n.kind, n.path.as_str())).collect();
    assert_eq!(
        paths,
        [
            (ForkNamespaceKind::User, "hoangnv"),
            (ForkNamespaceKind::Organization, "acme"),
            (ForkNamespaceKind::Organization, "oss"),
        ]
    );
    assert_eq!(ns[0].name.as_deref(), Some("Hoàng"));
}

fn gh_repo(full: &str, fork_of: Option<&str>) -> serde_json::Value {
    let (owner, name) = full.split_once('/').unwrap();
    json!({
        "id": 9, "name": name, "full_name": full, "owner": {"login": owner},
        "private": false, "fork": fork_of.is_some(),
        "parent": fork_of.map(|p| json!({"full_name": p})),
        "clone_url": format!("https://github.com/{full}.git"),
        "ssh_url": format!("git@github.com:{full}.git"),
        "html_url": format!("https://github.com/{full}"),
        "permissions": {"admin": true, "push": true, "pull": true}
    })
}

#[tokio::test]
async fn github_find_create_and_wait_for_fork() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"id": 1, "login": "hoangnv"})),
        )
        .mount(&server)
        .await;
    // Existing fork in the user namespace.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/hoangnv/lib"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(gh_repo("hoangnv/lib", Some("some-org/lib"))),
        )
        .mount(&server)
        .await;
    // Same name in the org, but not a fork of the source.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/lib"))
        .respond_with(ResponseTemplate::new(200).set_body_json(gh_repo("acme/lib", None)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/oss/lib"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})))
        .mount(&server)
        .await;
    let c = github(&server);
    let found = c.find_fork("some-org/lib", None).await.unwrap().unwrap();
    assert_eq!(found.full_name, "hoangnv/lib");
    assert!(
        c.find_fork("some-org/lib", Some("acme"))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        c.find_fork("some-org/lib", Some("oss"))
            .await
            .unwrap()
            .is_none()
    );

    // `organization` is sent for an org and omitted for the account itself.
    Mock::given(method("POST"))
        .and(path("/api/v3/repos/some-org/lib/forks"))
        .and(body_json(
            json!({"default_branch_only": true, "organization": "acme"}),
        ))
        .respond_with(
            ResponseTemplate::new(202).set_body_json(gh_repo("acme/lib-1", Some("some-org/lib"))),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v3/repos/some-org/lib/forks"))
        .and(body_json(json!({"default_branch_only": false})))
        .respond_with(
            ResponseTemplate::new(202).set_body_json(gh_repo("hoangnv/lib", Some("some-org/lib"))),
        )
        .expect(1)
        .mount(&server)
        .await;
    let name = c
        .create_fork(
            "some-org/lib",
            &ForkOptions {
                namespace: Some("acme".into()),
                name: None,
                default_branch_only: true,
            },
        )
        .await
        .unwrap();
    assert_eq!(name, "acme/lib-1");
    let name = c
        .create_fork(
            "some-org/lib",
            &ForkOptions {
                namespace: Some("HoangNV".into()),
                ..ForkOptions::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(name, "hoangnv/lib");

    // Readiness: 409 empty while copying, then 200.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/lib-1/commits"))
        .respond_with(
            ResponseTemplate::new(409)
                .set_body_json(json!({"message": "Git Repository is empty."})),
        )
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/lib-1/commits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"sha": SHA}])))
        .mount(&server)
        .await;
    assert_eq!(
        c.fork_readiness("acme/lib-1").await.unwrap(),
        ForkReadiness::Pending
    );
    let ready = c
        .wait_fork_ready(
            "acme/lib-1",
            Duration::from_secs(5),
            Duration::from_millis(20),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(ready);
}

#[tokio::test]
async fn wait_gives_up_after_timeout_and_honours_cancel() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/slow/commits"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})))
        .mount(&server)
        .await;
    let c = github(&server);
    let ready = c
        .wait_fork_ready(
            "acme/slow",
            Duration::from_millis(120),
            Duration::from_millis(30),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(!ready);
    let cancel = CancellationToken::new();
    cancel.cancel();
    let err = c
        .wait_fork_ready(
            "acme/slow",
            Duration::from_secs(5),
            Duration::from_millis(30),
            &cancel,
        )
        .await
        .unwrap_err();
    assert_eq!(err, ProviderError::Cancelled);
}

#[tokio::test]
async fn github_fork_refused_by_org_is_recognised() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v3/repos/acme/secret/forks"))
        .respond_with(ResponseTemplate::new(403).set_body_json(
            json!({"message": "Repository forking is disabled for this organization."}),
        ))
        .mount(&server)
        .await;
    let err = github(&server)
        .create_fork("acme/secret", &ForkOptions::default())
        .await
        .unwrap_err();
    assert_eq!(fork_problem(&err), Some(ForkProblem::ForkingDisabled));
}

#[tokio::test]
async fn gitlab_namespaces_find_create_and_import_status() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"id": 5, "username": "hoang", "name": "Hoàng", "avatar_url": null}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/groups"))
        .and(query_param("min_access_level", "30"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"full_path": "acme/team", "full_name": "Acme / Team"}
        ])))
        .mount(&server)
        .await;
    let c = gitlab(&server);
    let ns = c.fork_namespaces().await.unwrap();
    assert_eq!(ns.len(), 2);
    assert_eq!(ns[1].kind, ForkNamespaceKind::Group);
    assert_eq!(ns[1].path, "acme/team");

    Mock::given(method("GET"))
        .and(path("/api/v4/projects/up%2Flib/forks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"path_with_namespace": "acme/team/lib", "namespace": {"full_path": "acme/team"}}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/acme%2Fteam%2Flib"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 3, "name": "lib", "path": "lib", "path_with_namespace": "acme/team/lib",
            "namespace": {"full_path": "acme/team"}, "visibility": "private",
            "forked_from_project": {"path_with_namespace": "up/lib"},
            "http_url_to_repo": "https://gl/acme/team/lib.git",
            "ssh_url_to_repo": "git@gl:acme/team/lib.git", "web_url": "https://gl/acme/team/lib",
            "import_status": "finished"
        })))
        .mount(&server)
        .await;
    let found = c
        .find_fork("up/lib", Some("acme/team"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.full_name, "acme/team/lib");
    assert_eq!(found.parent_full_name.as_deref(), Some("up/lib"));
    // The user namespace has no fork.
    assert!(c.find_fork("up/lib", None).await.unwrap().is_none());

    Mock::given(method("POST"))
        .and(path("/api/v4/projects/up%2Flib/fork"))
        .and(body_json(json!({"namespace_path": "hoang"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "path_with_namespace": "hoang/lib", "import_status": "scheduled"
        })))
        .mount(&server)
        .await;
    let name = c
        .create_fork(
            "up/lib",
            &ForkOptions {
                namespace: Some("hoang".into()),
                ..ForkOptions::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(name, "hoang/lib");
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/hoang%2Flib"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "path_with_namespace": "hoang/lib", "import_status": "failed",
            "import_error": "storage quota"
        })))
        .mount(&server)
        .await;
    assert_eq!(
        c.fork_readiness("hoang/lib").await.unwrap(),
        ForkReadiness::Failed {
            message: "storage quota".into()
        }
    );
    let err = c
        .wait_fork_ready(
            "hoang/lib",
            Duration::from_secs(1),
            Duration::from_millis(10),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("storage quota"));

    Mock::given(method("POST"))
        .and(path("/api/v4/projects/up%2Fdup/fork"))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({
            "message": {"name": ["has already been taken"], "path": ["has already been taken"]}
        })))
        .mount(&server)
        .await;
    let err = c
        .create_fork("up/dup", &ForkOptions::default())
        .await
        .unwrap_err();
    assert_eq!(fork_problem(&err), Some(ForkProblem::NameTaken));
}
