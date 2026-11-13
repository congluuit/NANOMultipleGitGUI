//! Gitea / Forgejo compatibility: real answers of codeberg.org's public API (repository
//! `forgejo/forgejo`, saved on 2026-11-13 in `fixtures/gitea/`) served by wiremock and read
//! by the GitHub-flavoured client the app uses for Gitea accounts.

use nanomgg_providers::ci::*;
use nanomgg_providers::issues::*;
use nanomgg_providers::pulls::*;
use nanomgg_providers::*;
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const REPO: &str = include_str!("fixtures/gitea/repo.json");
const PULLS: &str = include_str!("fixtures/gitea/pulls.json");
const ISSUES: &str = include_str!("fixtures/gitea/issues.json");
const USER: &str = include_str!("fixtures/gitea/user.json");
const STATUS: &str = include_str!("fixtures/gitea/status.json");

fn gitea(server: &MockServer) -> ProviderClient {
    let endpoint = ProviderEndpoint::gitea(&server.uri()).unwrap();
    assert_eq!(endpoint.kind(), ProviderKind::Gitea);
    assert!(!endpoint.kind().git_only());
    ProviderClient::new(
        build_http_client(&HttpConfig::default()).unwrap(),
        endpoint,
        SecretToken::new("gitea-token"),
    )
}

async fn serve(server: &MockServer, at: &str, body: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/{at}")))
        .and(header("authorization", "Bearer gitea-token"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
        .mount(server)
        .await;
}

#[tokio::test]
async fn reads_a_forgejo_repository_and_user() {
    let server = MockServer::start().await;
    serve(&server, "repos/forgejo/forgejo", REPO).await;
    serve(&server, "user", USER).await;
    let c = gitea(&server);

    let repo = c.repository("forgejo/forgejo").await.unwrap();
    assert_eq!(repo.provider, ProviderKind::Gitea);
    assert_eq!(repo.full_name, "forgejo/forgejo");
    assert_eq!(repo.owner, "forgejo");
    assert_eq!(repo.name, "forgejo");
    assert!(!repo.private);
    assert_eq!(repo.default_branch.as_deref(), Some("forgejo"));
    assert_eq!(repo.clone_https, "https://codeberg.org/forgejo/forgejo.git");
    assert!(
        repo.clone_ssh.contains("codeberg.org"),
        "{}",
        repo.clone_ssh
    );
    assert!(repo.updated_at.is_some());

    let user = c.current_user().await.unwrap();
    assert_eq!(user.username, "forgejo");
    assert!(user.avatar_url.is_some());
}

#[tokio::test]
async fn reads_forgejo_pull_requests_and_issues() {
    let server = MockServer::start().await;
    serve(&server, "repos/forgejo/forgejo/pulls", PULLS).await;
    serve(&server, "repos/forgejo/forgejo/issues", ISSUES).await;
    let c = gitea(&server);

    let pulls = c
        .pull_list("forgejo/forgejo", &PullListQuery::default())
        .await
        .unwrap();
    assert_eq!(pulls.items.len(), 2);
    for p in &pulls.items {
        assert!(p.number > 0);
        assert!(!p.title.is_empty());
        assert_eq!(p.state, PullState::Open);
        assert!(p.author.is_some());
        assert!(p.source_branch.is_some(), "{p:?}");
        assert!(p.target_branch.is_some(), "{p:?}");
        assert!(p.created_at.is_some());
    }

    let issues = c
        .issue_list("forgejo/forgejo", &IssueListQuery::default())
        .await
        .unwrap();
    assert_eq!(issues.items.len(), 2);
    for i in &issues.items {
        assert!(i.number > 0);
        assert!(!i.title.is_empty());
        assert_eq!(i.state, IssueState::Open);
        assert!(i.author.is_some());
        assert!(!i.is_pull_request);
    }
}

#[tokio::test]
async fn reads_forgejo_commit_statuses_as_ci_jobs() {
    let server = MockServer::start().await;
    let sha = "4a763714317f6064ed10b457ce2b301b66418faa";
    serve(
        &server,
        &format!("repos/forgejo/forgejo/commits/{sha}/status"),
        STATUS,
    )
    .await;
    let c = gitea(&server);
    let ci = c.ci_status("forgejo/forgejo", sha, true).await.unwrap();
    // 20 statuses in the saved answer; no check-runs request was made (there is no such API).
    assert_eq!(ci.jobs.len(), 20);
    assert!(ci.checks_error.is_none());
    assert!(ci.jobs.iter().all(|j| j.kind == CiJobKind::Status));
    assert!(ci.jobs.iter().any(|j| j.state == CiJobState::Skipped));
    assert!(ci.jobs.iter().any(|j| j.state == CiJobState::Success));
    // Links are relative on the server and made absolute for the UI.
    let url = ci.jobs[0].url.as_deref().unwrap();
    assert!(url.starts_with(&server.uri()), "{url}");
    assert!(url.contains("/forgejo/forgejo/actions/runs/"), "{url}");
    assert!(ci.jobs[0].started_at.is_some());
    let requests = server.received_requests().await.unwrap();
    assert!(
        requests
            .iter()
            .all(|r| !r.url.path().contains("check-runs"))
    );
}

fn pull_json(number: u64, merged: bool) -> serde_json::Value {
    json!({
        "number": number, "title": "WIP: Add x", "body": "b", "state": if merged { "closed" } else { "open" },
        "draft": !merged, "merged": merged,
        "merged_at": if merged { json!("2026-11-13T10:00:00+01:00") } else { json!(null) },
        "merge_commit_sha": if merged { json!("feedc0de") } else { json!(null) },
        "user": {"id": 1, "login": "alice", "full_name": "", "avatar_url": "https://c/a.png"},
        "labels": [], "assignees": null, "requested_reviewers": null,
        "head": {"label": "feature/x", "ref": "feature/x", "sha": "aaa111", "repo": {"full_name": "acme/app"}},
        "base": {"label": "main", "ref": "main", "sha": "bbb222", "repo": {"full_name": "acme/app"}},
        "comments": 0, "created_at": "2026-11-13T09:00:00+01:00", "updated_at": "2026-11-13T09:00:00+01:00",
        "html_url": format!("https://codeberg.org/acme/app/pulls/{number}")
    })
}

#[tokio::test]
async fn creates_and_merges_a_pull_request_the_gitea_way() {
    let server = MockServer::start().await;
    // A draft is a "WIP:" title; there is no `draft` field to send.
    Mock::given(method("POST"))
        .and(path("/api/v1/repos/acme/app/pulls"))
        .and(body_json(json!({
            "title": "WIP: Add x", "head": "feature/x", "base": "main", "body": "b"
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(pull_json(7, false)))
        .expect(1)
        .mount(&server)
        .await;
    let c = gitea(&server);
    let created = c
        .pull_create(
            "acme/app",
            &PullCreate {
                source_branch: "feature/x".into(),
                target_branch: "main".into(),
                title: "Add x".into(),
                body: "b".into(),
                draft: true,
                reviewers: vec![],
                labels: vec![],
                remove_source_branch: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(created.number, 7);
    assert!(created.warnings.is_empty(), "{:?}", created.warnings);

    // Merge: POST with `Do`, an empty answer, then the merge commit from the pull itself.
    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/repos/acme/app/pulls/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pull_json(7, false)))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/repos/acme/app/pulls/7/merge"))
        .and(body_json(
            json!({"Do": "squash", "head_commit_id": "aaa111"}),
        ))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/repos/acme/app/pulls/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pull_json(7, true)))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/repos/acme/app/branches/feature%2Fx"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let merged = c
        .pull_merge(
            "acme/app",
            7,
            &PullMergeRequest {
                method: PullMergeMethod::Squash,
                expected_head_sha: Some("aaa111".into()),
                delete_source_branch: true,
            },
        )
        .await
        .unwrap();
    assert_eq!(merged.sha.as_deref(), Some("feedc0de"));
    assert!(merged.branch_deleted, "{:?}", merged.delete_error);
    let requests = server.received_requests().await.unwrap();
    assert!(requests.iter().all(|r| r.method.as_str() != "PUT"));
}

#[tokio::test]
async fn reviews_comments_and_empty_ci_follow_gitea_shapes() {
    let server = MockServer::start().await;
    // Requested reviewers come from the pull request itself; states have Gitea's names.
    let mut pull = pull_json(7, false);
    // The server keeps reviewers in this list after they reviewed (dave approved).
    pull["requested_reviewers"] = json!([{"id": 3, "login": "carol"}, {"id": 4, "login": "dave"}]);
    pull["requested_reviewers_teams"] = json!([{"id": 9, "name": "core"}]);
    Mock::given(method("GET"))
        .and(path("/api/v1/repos/acme/app/pulls/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pull))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/repos/acme/app/pulls/7/reviews"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 1, "user": {"id": 2, "login": "bob"}, "body": "Please fix", "state": "REQUEST_CHANGES",
             "submitted_at": "2026-11-13T09:10:00+01:00", "html_url": "https://c/r/1"},
            {"id": 2, "user": {"id": 4, "login": "dave"}, "body": "", "state": "APPROVED",
             "submitted_at": "2026-11-13T09:20:00+01:00", "html_url": "https://c/r/2"}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/repos/acme/app/issues/7/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 11, "user": {"id": 1, "login": "alice"}, "body": "Thanks",
             "created_at": "2026-11-13T09:05:00+01:00", "updated_at": "2026-11-13T09:05:00+01:00",
             "html_url": "https://c/c/11"}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/repos/acme/app/commits/aaa1111/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"state": "", "sha": "aaa1111", "total_count": 0, "statuses": null}),
        ))
        .mount(&server)
        .await;
    let c = gitea(&server);

    let reviews = c.pull_reviews("acme/app", 7).await.unwrap();
    let state = |name: &str| {
        reviews
            .reviewers
            .iter()
            .find(|r| r.user.username == name)
            .map(|r| r.state)
    };
    assert_eq!(state("bob"), Some(PullReviewerState::ChangesRequested));
    assert_eq!(state("dave"), Some(PullReviewerState::Approved));
    assert_eq!(state("carol"), Some(PullReviewerState::Requested));
    assert_eq!(reviews.teams, ["core"]);

    let comments = c.pull_comments("acme/app", 7).await.unwrap();
    let bodies: Vec<&str> = comments.iter().map(|m| m.body.as_str()).collect();
    assert_eq!(bodies, ["Thanks", "Please fix", ""]);

    let ci = c.ci_status("acme/app", "aaa1111", true).await.unwrap();
    assert!(ci.jobs.is_empty());

    // Neither of the GitHub-only calls was made.
    let requests = server.received_requests().await.unwrap();
    assert!(requests.iter().all(|r| {
        let p = r.url.path();
        !p.ends_with("/requested_reviewers") && !p.ends_with("/pulls/7/comments")
    }));
}
