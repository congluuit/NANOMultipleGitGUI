//! Pull / merge request API (Phase 4.1) against wiremock servers (no real network).

use nanomgg_providers::pulls::*;
use nanomgg_providers::*;
use serde_json::json;
use wiremock::matchers::{body_json, body_partial_json, method, path, query_param};
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
    let endpoint = ProviderEndpoint::custom(
        ProviderKind::GitlabSelfHosted,
        &server.uri(),
        &format!("{}/api/v4", server.uri()),
    )
    .unwrap();
    ProviderClient::new(
        build_http_client(&HttpConfig::default()).unwrap(),
        endpoint,
        SecretToken::new("glpat-testtoken"),
    )
}

fn gh_user(login: &str) -> serde_json::Value {
    json!({"login": login, "avatar_url": format!("https://a/{login}.png"), "html_url": format!("https://github.com/{login}")})
}

fn gh_pull(number: u64, head_repo: &str) -> serde_json::Value {
    json!({
        "number": number, "title": format!("PR {number}"), "body": "Body <script>x</script>",
        "state": "open", "draft": false, "merged_at": null, "user": gh_user("alice"),
        "head": {"ref": "feature/x", "sha": "aaa111", "repo": {"full_name": head_repo}},
        "base": {"ref": "main", "sha": "bbb222", "repo": {"full_name": "acme/app"}},
        "labels": [{"name": "bug"}], "assignees": [gh_user("bob")],
        "comments": 2, "review_comments": 1, "commits": 3, "additions": 10, "deletions": 4,
        "changed_files": 2, "mergeable": true, "mergeable_state": "clean",
        "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-02T00:00:00Z",
        "html_url": format!("https://github.com/acme/app/pull/{number}")
    })
}

fn gl_user(name: &str) -> serde_json::Value {
    json!({"username": name, "name": name.to_uppercase(), "avatar_url": null, "web_url": format!("https://gl/{name}")})
}

fn gl_mr(iid: u64) -> serde_json::Value {
    json!({
        "iid": iid, "title": format!("MR {iid}"), "description": "desc", "state": "opened",
        "draft": false, "author": gl_user("carol"), "source_branch": "feat", "target_branch": "main",
        "source_project_id": 7, "target_project_id": 5, "labels": ["ui"], "assignees": [],
        "reviewers": [gl_user("dave"), gl_user("erin")], "user_notes_count": 4, "sha": "ccc333",
        "diff_refs": {"base_sha": "b1", "head_sha": "ccc333", "start_sha": "s1"},
        "merge_status": "can_be_merged", "detailed_merge_status": "not_approved",
        "has_conflicts": false, "changes_count": "1000+",
        "created_at": "2024-03-01T00:00:00Z", "updated_at": "2024-03-02T00:00:00Z",
        "web_url": format!("https://gl/grp/app/-/merge_requests/{iid}")
    })
}

#[tokio::test]
async fn github_list_paginates_and_filters_through_search() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/app/pulls"))
        .and(query_param("state", "open"))
        .and(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "link",
                    format!(
                        "<{}/api/v3/repos/acme/app/pulls?page=2>; rel=\"next\"",
                        server.uri()
                    )
                    .as_str(),
                )
                .set_body_json(json!([gh_pull(1, "acme/app"), gh_pull(2, "fork/app")])),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .and(query_param(
            "q",
            "repo:acme/app is:pr is:merged author:alice label:\"good first\"",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 1,
            "items": [{
                "number": 9, "title": "Merged", "state": "closed", "draft": false,
                "user": gh_user("alice"), "labels": [], "comments": 0,
                "html_url": "https://github.com/acme/app/pull/9",
                "pull_request": {"merged_at": "2024-01-05T00:00:00Z"}
            }]
        })))
        .mount(&server)
        .await;
    let c = github(&server);
    let page = c
        .pull_list("acme/app", &PullListQuery::default())
        .await
        .unwrap();
    assert_eq!(page.next_page, Some(2));
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].source_branch.as_deref(), Some("feature/x"));
    assert_eq!(page.items[0].labels, vec!["bug"]);
    let page = c
        .pull_list(
            "acme/app",
            &PullListQuery {
                state: PullStateFilter::Merged,
                author: Some("alice".into()),
                label: Some("good first".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page.total, Some(1));
    assert_eq!(page.items[0].state, PullState::Merged);
    assert_eq!(page.items[0].source_branch, None);
}

#[tokio::test]
async fn github_detail_reviews_and_fork_detection() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/app/pulls/2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(gh_pull(2, "fork/app")))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/app"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "allow_merge_commit": false, "allow_squash_merge": true, "allow_rebase_merge": true,
            "delete_branch_on_merge": true
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/app/pulls/2/reviews"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 1, "user": gh_user("bob"), "body": "", "state": "CHANGES_REQUESTED", "submitted_at": "2024-01-03T00:00:00Z"},
            {"id": 2, "user": gh_user("bob"), "body": "nit", "state": "COMMENTED", "submitted_at": "2024-01-04T00:00:00Z"},
            {"id": 3, "user": gh_user("bob"), "body": "", "state": "APPROVED", "submitted_at": "2024-01-05T00:00:00Z"},
            {"id": 4, "user": gh_user("zed"), "body": "", "state": "PENDING"}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/app/pulls/2/requested_reviewers"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "users": [gh_user("carol")], "teams": [{"slug": "core", "name": "Core"}]
        })))
        .mount(&server)
        .await;
    let c = github(&server);
    let d = c.pull_detail("acme/app", 2).await.unwrap();
    assert!(d.from_fork);
    assert_eq!(d.source_repo.as_deref(), Some("fork/app"));
    assert_eq!(d.target_repo, "acme/app");
    assert_eq!(d.merge_state, PullMergeState::Clean);
    assert_eq!(
        d.merge_methods,
        vec![PullMergeMethod::Squash, PullMergeMethod::Rebase]
    );
    assert!(d.delete_source_default);
    assert_eq!(d.body, "Body <script>x</script>");

    let r = c.pull_reviews("acme/app", 2).await.unwrap();
    let bob = r
        .reviewers
        .iter()
        .find(|x| x.user.username == "bob")
        .unwrap();
    assert_eq!(bob.state, PullReviewerState::Approved);
    assert!(r.reviewers.iter().all(|x| x.user.username != "zed"));
    assert_eq!(r.decision, PullReviewDecision::ReviewRequired);
    assert_eq!(r.teams, vec!["Core"]);
}

#[tokio::test]
async fn github_files_comments_and_writes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/app/pulls/5/files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"filename": "a.rs", "status": "modified", "additions": 1, "deletions": 1, "patch": "@@ -1 +1 @@\n-a\n+b"},
            {"filename": "big.bin", "status": "added", "additions": 0, "deletions": 0},
            {"filename": "new.rs", "previous_filename": "old.rs", "status": "renamed", "additions": 0, "deletions": 0}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/app/issues/5/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 10, "user": gh_user("a"), "body": "second", "created_at": "2024-01-02T00:00:00Z"}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/app/pulls/5/reviews"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 20, "user": gh_user("b"), "body": "", "state": "COMMENTED", "submitted_at": "2024-01-01T00:00:00Z"},
            {"id": 21, "user": gh_user("b"), "body": "", "state": "APPROVED", "submitted_at": "2024-01-04T00:00:00Z"}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/app/pulls/5/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 30, "user": gh_user("b"), "body": "first", "created_at": "2024-01-01T00:00:00Z",
             "path": "a.rs", "line": 1, "original_line": 1, "side": "RIGHT", "diff_hunk": "@@"},
            {"id": 31, "user": gh_user("a"), "body": "reply", "created_at": "2024-01-03T00:00:00Z",
             "path": "a.rs", "line": null, "original_line": 1, "side": "LEFT", "in_reply_to_id": 30}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v3/repos/acme/app/pulls/5/comments/30/replies"))
        .and(body_json(json!({"body": "ok"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 32, "user": gh_user("me"), "body": "ok", "path": "a.rs", "line": 1, "in_reply_to_id": 30
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/app/pulls/5"))
        .respond_with(ResponseTemplate::new(200).set_body_json(gh_pull(5, "acme/app")))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v3/repos/acme/app/pulls/5/comments"))
        .and(body_json(json!({
            "body": "why?", "commit_id": "aaa111", "path": "a.rs", "line": 1, "side": "LEFT"
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 33, "user": gh_user("me"), "body": "why?", "path": "a.rs", "line": 1, "side": "LEFT"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v3/repos/acme/app/pulls/5/reviews"))
        .and(body_json(json!({"event": "APPROVE"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": 1})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api/v3/repos/acme/app/pulls/5/merge"))
        .and(body_json(
            json!({"merge_method": "squash", "sha": "aaa111"}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"sha": "m1", "merged": true})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v3/repos/acme/app/git/refs/heads/feature/x"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/api/v3/repos/acme/app/pulls/5"))
        .and(body_json(json!({"state": "closed"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(gh_pull(5, "acme/app")))
        .expect(1)
        .mount(&server)
        .await;

    let c = github(&server);
    let files = c.pull_files("acme/app", 5).await.unwrap();
    assert!(!files.truncated);
    assert_eq!(files.files[1].patch, None);
    assert_eq!(files.files[2].status, PullFileStatus::Renamed);
    assert_eq!(files.files[2].old_path.as_deref(), Some("old.rs"));

    let comments = c.pull_comments("acme/app", 5).await.unwrap();
    let ids: Vec<&str> = comments.iter().map(|c| c.id.as_str()).collect();
    // Body-less COMMENTED review skipped; sorted by time.
    assert_eq!(ids, vec!["line-30", "issue-10", "line-31", "review-21"]);
    assert_eq!(comments[2].thread_id.as_deref(), Some("30"));
    let outdated = comments[2].anchor.as_ref().unwrap();
    assert_eq!((outdated.line, outdated.side), (None, PullSide::Old));
    assert_eq!(comments[3].review_state, Some(PullReviewerState::Approved));

    let reply = c
        .pull_comment(
            "acme/app",
            5,
            &PullCommentTarget::Reply {
                thread_id: "30".into(),
            },
            "ok",
        )
        .await
        .unwrap();
    assert_eq!(reply.thread_id.as_deref(), Some("30"));
    c.pull_comment(
        "acme/app",
        5,
        &PullCommentTarget::Line {
            path: "a.rs".into(),
            old_path: None,
            line: 1,
            side: PullSide::Old,
            paired_line: None,
        },
        "why?",
    )
    .await
    .unwrap();
    let bad = c
        .pull_comment(
            "acme/app",
            5,
            &PullCommentTarget::Reply {
                thread_id: "../x".into(),
            },
            "x",
        )
        .await
        .unwrap_err();
    assert_eq!(bad.code(), "invalidInput");
    assert_eq!(
        c.pull_comment("acme/app", 5, &PullCommentTarget::General, "  ")
            .await
            .unwrap_err()
            .code(),
        "invalidInput"
    );
    c.pull_review("acme/app", 5, PullReviewEvent::Approve, "")
        .await
        .unwrap();
    let merged = c
        .pull_merge(
            "acme/app",
            5,
            &PullMergeRequest {
                method: PullMergeMethod::Squash,
                delete_source_branch: true,
                expected_head_sha: Some("aaa111".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(merged.sha.as_deref(), Some("m1"));
    assert!(merged.branch_deleted, "{:?}", merged.delete_error);
    c.pull_set_open("acme/app", 5, false).await.unwrap();
}

#[tokio::test]
async fn github_create_reports_follow_up_failures() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v3/repos/acme/app/pulls"))
        .and(body_json(json!({
            "title": "Add x", "head": "feature/x", "base": "main", "body": "b", "draft": true
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(gh_pull(7, "acme/app")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v3/repos/acme/app/pulls/7/requested_reviewers"))
        .and(body_json(json!({"reviewers": ["bob"]})))
        .respond_with(
            ResponseTemplate::new(422).set_body_json(
                json!({"message": "Reviews may only be requested from collaborators."}),
            ),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v3/repos/acme/app/issues/7/labels"))
        .and(body_json(json!({"labels": ["bug"]})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"name": "bug"}])))
        .expect(1)
        .mount(&server)
        .await;
    let c = github(&server);
    let created = c
        .pull_create(
            "acme/app",
            &PullCreate {
                source_branch: "feature/x".into(),
                target_branch: "main".into(),
                title: " Add x ".into(),
                body: "b".into(),
                draft: true,
                reviewers: vec!["@bob".into(), " ".into()],
                labels: vec!["bug".into()],
                remove_source_branch: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(created.number, 7);
    assert_eq!(created.warnings.len(), 1);
    assert!(created.warnings[0].contains("collaborators"));
}

#[tokio::test]
async fn github_errors_are_typed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/a401/pulls"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message": "Bad credentials"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/a404/pulls"))
        .respond_with(
            ResponseTemplate::new(404)
                .insert_header("x-accepted-oauth-scopes", "repo")
                .insert_header("x-oauth-scopes", "read:user")
                .set_body_json(json!({"message": "Not Found"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/a403/pulls"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "1999999999")
                .set_body_json(json!({"message": "API rate limit exceeded"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api/v3/repos/acme/app/pulls/1/merge"))
        .respond_with(
            ResponseTemplate::new(405)
                .set_body_json(json!({"message": "Pull Request is not mergeable"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/app/pulls/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(gh_pull(1, "acme/app")))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v3/repos/acme/app/pulls"))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({
            "message": "Validation Failed",
            "errors": [{"message": "No commits between main and feature"}]
        })))
        .mount(&server)
        .await;
    let c = github(&server);
    let q = PullListQuery::default();
    assert_eq!(
        c.pull_list("acme/a401", &q).await.unwrap_err().code(),
        "unauthorized"
    );
    let e = c.pull_list("acme/a404", &q).await.unwrap_err();
    assert_eq!(e.code(), "notFound");
    assert!(matches!(e.hint(), Some(AccessHint::MissingScopes { .. })));
    assert!(matches!(
        c.pull_list("acme/a403", &q).await.unwrap_err(),
        ProviderError::RateLimited {
            reset_at: Some(1_999_999_999),
            ..
        }
    ));
    let e = c
        .pull_merge(
            "acme/app",
            1,
            &PullMergeRequest {
                method: PullMergeMethod::Merge,
                delete_source_branch: false,
                expected_head_sha: None,
            },
        )
        .await
        .unwrap_err();
    assert!(e.to_string().contains("not mergeable"), "{e}");
    let e = c
        .pull_create(
            "acme/app",
            &PullCreate {
                source_branch: "feature".into(),
                target_branch: "main".into(),
                title: "t".into(),
                body: String::new(),
                draft: false,
                reviewers: vec![],
                labels: vec![],
                remove_source_branch: false,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(e.code(), "rejected");
    assert!(
        c.pull_list("not-a-repo", &q).await.unwrap_err().code() == "invalidInput",
        "bad full name"
    );
}

#[tokio::test]
async fn gitlab_list_detail_and_reviews() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/grp%2Fapp/merge_requests"))
        .and(query_param("state", "opened"))
        .and(query_param("author_username", "carol"))
        .and(query_param("reviewer_username", "dave"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-next-page", "2")
                .insert_header("x-total", "31")
                .set_body_json(json!([gl_mr(3)])),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/grp%2Fapp/merge_requests/3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(gl_mr(3)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/grp%2Fapp"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "path_with_namespace": "grp/app", "squash_option": "default_off",
            "remove_source_branch_after_merge": true
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "path_with_namespace": "carol/app"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/v4/projects/grp%2Fapp/merge_requests/3/approvals",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "approvals_required": 2, "approvals_left": 1,
            "approved_by": [{"user": gl_user("dave")}, {"user": gl_user("frank")}],
            "user_has_approved": false
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/v4/projects/grp%2Fapp/merge_requests/3/reviewers",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"user": gl_user("erin"), "state": "requested_changes"}
        ])))
        .mount(&server)
        .await;
    let c = gitlab(&server);
    let page = c
        .pull_list(
            "grp/app",
            &PullListQuery {
                author: Some("carol".into()),
                reviewer: Some("@dave".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!((page.next_page, page.total), (Some(2), Some(31)));
    assert_eq!(page.items[0].number, 3);
    assert_eq!(page.items[0].comments, Some(4));

    let d = c.pull_detail("grp/app", 3).await.unwrap();
    assert!(d.from_fork);
    assert_eq!(d.source_repo.as_deref(), Some("carol/app"));
    assert_eq!(d.merge_state, PullMergeState::Blocked);
    assert_eq!(d.merge_state_raw.as_deref(), Some("not_approved"));
    assert_eq!(
        d.merge_methods,
        vec![PullMergeMethod::Merge, PullMergeMethod::Squash]
    );
    assert!(d.delete_source_default);
    assert_eq!(d.changed_files, Some(1000));
    assert_eq!(d.start_sha.as_deref(), Some("s1"));

    let r = c.pull_reviews("grp/app", 3).await.unwrap();
    let state = |u: &str| {
        r.reviewers
            .iter()
            .find(|x| x.user.username == u)
            .map(|x| x.state)
    };
    assert_eq!(state("dave"), Some(PullReviewerState::Approved));
    assert_eq!(state("erin"), Some(PullReviewerState::ChangesRequested));
    assert_eq!(state("frank"), Some(PullReviewerState::Approved));
    assert_eq!(r.decision, PullReviewDecision::ChangesRequested);
    assert_eq!(r.approvals_left, Some(1));
    assert_eq!(r.approved_by_me, Some(false));
}

#[tokio::test]
async fn gitlab_old_server_fallbacks() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/grp%2Fapp/merge_requests/3"))
        .respond_with(ResponseTemplate::new(200).set_body_json({
            let mut mr = gl_mr(3);
            mr["reviewers"] = json!([]);
            mr
        }))
        .mount(&server)
        .await;
    for missing in ["approvals", "reviewers", "diffs"] {
        Mock::given(method("GET"))
            .and(path(format!(
                "/api/v4/projects/grp%2Fapp/merge_requests/3/{missing}"
            )))
            .respond_with(
                ResponseTemplate::new(404).set_body_json(json!({"message": "404 Not found"})),
            )
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/grp%2Fapp/merge_requests/3/changes"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "overflow": true,
            "changes": [
                {"old_path": "a", "new_path": "b", "renamed_file": true, "diff": "@@ -1 +1 @@\n-x\n+y\n+z\n"},
                {"old_path": "big", "new_path": "big", "new_file": true, "diff": ""}
            ]
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v4/projects/grp%2Fapp/merge_requests/3/approve"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "404"})))
        .mount(&server)
        .await;
    let c = gitlab(&server);
    let r = c.pull_reviews("grp/app", 3).await.unwrap();
    assert!(r.approvals_unavailable);
    assert_eq!(r.decision, PullReviewDecision::None);
    let f = c.pull_files("grp/app", 3).await.unwrap();
    assert!(f.truncated);
    assert_eq!(f.files[0].status, PullFileStatus::Renamed);
    assert_eq!(f.files[0].old_path.as_deref(), Some("a"));
    assert_eq!((f.files[0].additions, f.files[0].deletions), (2, 1));
    assert_eq!(f.files[1].patch, None);
    let e = c
        .pull_review("grp/app", 3, PullReviewEvent::Approve, "")
        .await
        .unwrap_err();
    assert_eq!(e.code(), "unsupported");
}

#[tokio::test]
async fn gitlab_comments_and_writes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/grp%2Fapp/merge_requests/3/discussions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": "d1", "individual_note": true, "notes": [
                {"id": 1, "body": "hello", "author": gl_user("a"), "created_at": "2024-03-01T00:00:00Z", "system": false}
            ]},
            {"id": "d2", "individual_note": false, "notes": [
                {"id": 2, "body": "line", "author": gl_user("b"), "created_at": "2024-03-02T00:00:00Z",
                 "system": false, "resolvable": true, "resolved": false,
                 "position": {"new_path": "x.rs", "old_path": "x.rs", "new_line": null, "old_line": 4}},
                {"id": 3, "body": "re", "author": gl_user("a"), "created_at": "2024-03-03T00:00:00Z",
                 "system": false, "resolvable": true, "resolved": false}
            ]},
            {"id": "d3", "individual_note": true, "notes": [
                {"id": 4, "body": "added 1 commit", "created_at": "2024-02-28T00:00:00Z", "system": true}
            ]}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/grp%2Fapp/merge_requests/3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(gl_mr(3)))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/api/v4/projects/grp%2Fapp/merge_requests/3/discussions",
        ))
        .and(body_partial_json(json!({
            "body": "ctx",
            "position": {"position_type": "text", "base_sha": "b1", "start_sha": "s1",
                         "head_sha": "ccc333", "new_path": "x.rs", "old_path": "x.rs",
                         "new_line": 5, "old_line": 4}
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": "d9", "individual_note": false, "notes": [
                {"id": 9, "body": "ctx", "author": gl_user("me"), "system": false,
                 "position": {"new_path": "x.rs", "new_line": 5, "old_line": 4}}
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/api/v4/projects/grp%2Fapp/merge_requests/3/discussions/d2/notes",
        ))
        .and(body_json(json!({"body": "thanks"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 10, "body": "thanks", "author": gl_user("me"), "system": false
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v4/projects/grp%2Fapp/merge_requests/3/notes"))
        .and(body_json(json!({"body": "please fix"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 11, "body": "please fix", "system": false
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/users"))
        .and(query_param("username", "dave"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id": 42}])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/users"))
        .and(query_param("username", "ghost"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v4/projects/grp%2Fapp/merge_requests"))
        .and(body_json(json!({
            "source_branch": "feat", "target_branch": "main", "title": "Draft: Add",
            "description": "", "remove_source_branch": true, "labels": "ui,bug",
            "reviewer_ids": [42]
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(gl_mr(12)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api/v4/projects/grp%2Fapp/merge_requests/3/merge"))
        .and(body_json(
            json!({"squash": true, "should_remove_source_branch": true}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json({
            let mut mr = gl_mr(3);
            mr["state"] = json!("merged");
            mr["squash_commit_sha"] = json!("sq1");
            mr
        }))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api/v4/projects/grp%2Fapp/merge_requests/3"))
        .and(body_json(json!({"state_event": "reopen"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(gl_mr(3)))
        .expect(1)
        .mount(&server)
        .await;

    let c = gitlab(&server);
    let comments = c.pull_comments("grp/app", 3).await.unwrap();
    let kinds: Vec<PullCommentKind> = comments.iter().map(|c| c.kind).collect();
    assert_eq!(
        kinds,
        vec![
            PullCommentKind::System,
            PullCommentKind::General,
            PullCommentKind::Line,
            PullCommentKind::General
        ]
    );
    assert_eq!(comments[1].thread_id, None);
    assert_eq!(comments[2].thread_id.as_deref(), Some("d2"));
    assert_eq!(comments[3].thread_id.as_deref(), Some("d2"));
    let anchor = comments[2].anchor.as_ref().unwrap();
    assert_eq!((anchor.line, anchor.side), (Some(4), PullSide::Old));
    assert_eq!(comments[2].resolved, Some(false));

    let created = c
        .pull_comment(
            "grp/app",
            3,
            &PullCommentTarget::Line {
                path: "x.rs".into(),
                old_path: None,
                line: 5,
                side: PullSide::New,
                paired_line: Some(4),
            },
            "ctx",
        )
        .await
        .unwrap();
    assert_eq!(created.thread_id.as_deref(), Some("d9"));
    c.pull_comment(
        "grp/app",
        3,
        &PullCommentTarget::Reply {
            thread_id: "d2".into(),
        },
        "thanks",
    )
    .await
    .unwrap();
    // No "request changes" API on GitLab: posted as a note.
    c.pull_review("grp/app", 3, PullReviewEvent::RequestChanges, "please fix")
        .await
        .unwrap();
    assert_eq!(
        c.pull_review("grp/app", 3, PullReviewEvent::Comment, "")
            .await
            .unwrap_err()
            .code(),
        "invalidInput"
    );
    let created = c
        .pull_create(
            "grp/app",
            &PullCreate {
                source_branch: "feat".into(),
                target_branch: "main".into(),
                title: "Add".into(),
                body: String::new(),
                draft: true,
                reviewers: vec!["dave".into(), "ghost".into()],
                labels: vec!["ui".into(), "bug".into()],
                remove_source_branch: true,
            },
        )
        .await
        .unwrap();
    assert_eq!(created.number, 12);
    assert_eq!(created.warnings, vec!["reviewer `ghost` not found"]);
    let merged = c
        .pull_merge(
            "grp/app",
            3,
            &PullMergeRequest {
                method: PullMergeMethod::Squash,
                delete_source_branch: true,
                expected_head_sha: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(merged.sha.as_deref(), Some("sq1"));
    assert_eq!(
        c.pull_merge(
            "grp/app",
            3,
            &PullMergeRequest {
                method: PullMergeMethod::Rebase,
                delete_source_branch: false,
                expected_head_sha: None,
            },
        )
        .await
        .unwrap_err()
        .code(),
        "unsupported"
    );
    c.pull_set_open("grp/app", 3, true).await.unwrap();
}

#[tokio::test]
async fn gitlab_commits_oldest_first_and_rate_limit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/grp%2Fapp/merge_requests/3/commits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": "c2", "title": "second", "message": "second\n\nbody", "author_name": "A", "authored_date": "2024-03-02T00:00:00Z"},
            {"id": "c1", "title": "", "message": "first line\nmore", "author_name": "B"}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/grp%2Fapp/merge_requests"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "30")
                .set_body_string("Retry later"),
        )
        .mount(&server)
        .await;
    let c = gitlab(&server);
    let commits = c.pull_commits("grp/app", 3).await.unwrap();
    assert_eq!(commits[0].sha, "c1");
    assert_eq!(commits[0].title, "first line");
    assert_eq!(commits[1].title, "second");
    let e = c
        .pull_list("grp/app", &PullListQuery::default())
        .await
        .unwrap_err();
    assert!(matches!(
        e,
        ProviderError::RateLimited {
            retry_after_secs: Some(30),
            ..
        }
    ));
    assert_eq!(
        c.pull_list("app", &PullListQuery::default())
            .await
            .unwrap_err()
            .code(),
        "invalidInput"
    );
}
