//! Issues and "My work" against a wiremock server (no real network) — Phase 4.2.

use nanomgg_providers::issues::*;
use nanomgg_providers::mywork::*;
use nanomgg_providers::*;
use serde_json::json;
use wiremock::matchers::{body_json, method, path, query_param, query_param_is_missing};
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
        SecretToken::new("ghp_test"),
    )
}

fn gitlab(server: &MockServer) -> ProviderClient {
    let endpoint = ProviderEndpoint::gitlab_self_hosted(&server.uri()).unwrap();
    ProviderClient::new(
        build_http_client(&HttpConfig::default()).unwrap(),
        endpoint,
        SecretToken::new("glpat-test"),
    )
}

fn gh_issue(number: u64, title: &str, pr: bool) -> serde_json::Value {
    let mut v = json!({
        "number": number, "title": title, "state": "open",
        "user": {"login": "alice", "avatar_url": "https://a/1.png"},
        "labels": [{"name": "bug", "color": "d73a4a", "description": "Something is wrong"}],
        "assignees": [{"login": "bob", "avatar_url": null}],
        "comments": 2, "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-02T00:00:00Z",
        "html_url": format!("https://github.com/acme/game/issues/{number}"),
        "body": "Steps\n<script>x</script>", "locked": false,
        "repository_url": "https://api.github.com/repos/acme/game"
    });
    if pr {
        v["pull_request"] = json!({"url": "x"});
        v["draft"] = json!(true);
    }
    v
}

fn gl_issue(iid: u64, title: &str) -> serde_json::Value {
    json!({
        "iid": iid, "title": title, "state": "opened",
        "author": {"id": 1, "username": "alice", "name": "Alice", "avatar_url": null},
        "labels": [{"name": "bug", "color": "#D9534F", "description": null}],
        "assignees": [], "user_notes_count": 4,
        "created_at": "2024-01-01T00:00:00.000Z", "updated_at": "2024-01-03T00:00:00.000Z",
        "web_url": format!("https://gl.acme.dev/grp/sub/proj/-/issues/{iid}"),
        "description": null, "discussion_locked": null,
        "references": {"full": format!("grp/sub/proj#{iid}")}
    })
}

#[tokio::test]
async fn github_list_drops_pull_requests_and_pages() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/game/issues"))
        .and(query_param("state", "open"))
        .and(query_param("labels", "bug,ui"))
        .and(query_param("assignee", "none"))
        .and(query_param("creator", "alice"))
        .and(query_param("page", "1"))
        .and(query_param("per_page", "50"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "link",
                    format!(
                        "<{}/api/v3/repos/acme/game/issues?page=2>; rel=\"next\"",
                        server.uri()
                    )
                    .as_str(),
                )
                .set_body_json(json!([
                    gh_issue(7, "Crash", false),
                    gh_issue(8, "PR", true)
                ])),
        )
        .expect(1)
        .mount(&server)
        .await;
    let c = github(&server);
    let page = c
        .issue_list(
            "acme/game",
            &IssueListQuery {
                labels: vec!["bug".into(), " ui ".into(), "bug".into()],
                assignee: Some(ASSIGNEE_NONE.into()),
                author: Some("alice".into()),
                page: 1,
                per_page: 50,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page.next_page, Some(2));
    assert_eq!(page.items.len(), 1);
    let i = &page.items[0];
    assert_eq!((i.number, i.title.as_str()), (7, "Crash"));
    assert_eq!(i.state, IssueState::Open);
    assert_eq!(i.labels[0].color.as_deref(), Some("#d73a4a"));
    assert_eq!(i.assignees[0].username, "bob");
    assert_eq!(i.author.as_ref().unwrap().username, "alice");
    assert!(!i.is_pull_request);
}

#[tokio::test]
async fn github_text_search_uses_search_api() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .and(query_param(
            "q",
            "repo:acme/game is:issue is:closed label:\"good first issue\" assignee:bob crash login",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 2, "incomplete_results": false,
            "items": [gh_issue(3, "Crash on login", false), gh_issue(4, "pr", true)]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let page = github(&server)
        .issue_list(
            "acme/game",
            &IssueListQuery {
                state: IssueStateFilter::Closed,
                labels: vec!["good first issue".into()],
                assignee: Some("bob".into()),
                search: Some("crash login".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page.total, Some(2));
    assert_eq!(page.items.len(), 1);
}

#[tokio::test]
async fn github_detail_comments_and_writes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/game/issues/8"))
        .respond_with(ResponseTemplate::new(200).set_body_json(gh_issue(8, "A PR", true)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/game/issues/7/comments"))
        .and(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "link",
                    format!("<{}/api/v3/x?page=2>; rel=\"next\"", server.uri()).as_str(),
                )
                .set_body_json(json!([{
                    "id": 1, "user": {"login": "bob", "avatar_url": null}, "body": "first",
                    "created_at": "2024-01-01T00:00:00Z", "updated_at": null,
                    "html_url": "https://github.com/acme/game/issues/7#issuecomment-1"
                }])),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/game/issues/7/comments"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": 2, "user": null, "body": null, "created_at": null, "updated_at": null,
            "html_url": null
        }])))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v3/repos/acme/game/issues/7/comments"))
        .and(body_json(json!({"body": "Thanks!"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 3, "user": {"login": "me", "avatar_url": null}, "body": "Thanks!",
            "created_at": "2024-01-05T00:00:00Z", "updated_at": null, "html_url": null
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v3/repos/acme/game/issues"))
        .and(body_json(json!({
            "title": "New bug", "body": "Body", "labels": ["bug"], "assignees": ["bob"]
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(gh_issue(9, "New bug", false)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/api/v3/repos/acme/game/issues/9"))
        .and(body_json(json!({"state": "closed", "labels": []})))
        .respond_with(ResponseTemplate::new(200).set_body_json({
            let mut v = gh_issue(9, "New bug", false);
            v["state"] = json!("closed");
            v["closed_at"] = json!("2024-01-06T00:00:00Z");
            v["closed_by"] = json!({"login": "me", "avatar_url": null});
            v
        }))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/game/labels"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name": "bug", "color": "zzzzzz", "description": ""}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/game/assignees"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"login": "bob", "avatar_url": "https://a/b.png"}
        ])))
        .mount(&server)
        .await;

    let c = github(&server);
    let d = c.issue("acme/game", 8).await.unwrap();
    assert!(d.issue.is_pull_request);
    assert_eq!(d.body, "Steps\n<script>x</script>");
    let comments = c.issue_comments("acme/game", 7).await.unwrap();
    assert_eq!(comments.comments.len(), 2);
    assert!(!comments.truncated);
    assert_eq!(comments.comments[1].body, "");
    assert!(c.issue_comment_create("acme/game", 7, "  ").await.is_err());
    let made = c
        .issue_comment_create("acme/game", 7, "Thanks!")
        .await
        .unwrap();
    assert_eq!(made.id, "3");
    assert!(
        c.issue_create("acme/game", &IssueCreate::default())
            .await
            .is_err()
    );
    let created = c
        .issue_create(
            "acme/game",
            &IssueCreate {
                title: " New bug ".into(),
                body: "Body".into(),
                labels: vec!["bug".into()],
                assignees: vec!["bob".into()],
            },
        )
        .await
        .unwrap();
    assert_eq!(created.issue.number, 9);
    let closed = c
        .issue_update(
            "acme/game",
            9,
            &IssueUpdate {
                state: Some(IssueState::Closed),
                labels: Some(vec![]),
                assignees: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(closed.issue.state, IssueState::Closed);
    assert_eq!(closed.closed_by.unwrap().username, "me");
    let labels = c.issue_labels("acme/game").await.unwrap();
    assert_eq!(labels[0].color, None);
    assert_eq!(labels[0].description, None);
    let users = c.issue_assignable_users("acme/game").await.unwrap();
    assert_eq!(users[0].username, "bob");
}

#[tokio::test]
async fn github_errors_are_typed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/secret/issues"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/game/issues/1"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message": "Bad credentials"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v3/repos/acme/game/issues"))
        .respond_with(
            ResponseTemplate::new(422).set_body_json(json!({"message": "Validation Failed"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/game/labels"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "1999999999")
                .set_body_json(json!({"message": "API rate limit exceeded"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/game/assignees"))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(
                json!({"message": "Resource not accessible by personal access token"}),
            ),
        )
        .mount(&server)
        .await;
    let c = github(&server);
    let e = c
        .issue_list("acme/secret", &IssueListQuery::default())
        .await
        .unwrap_err();
    assert_eq!(e.code(), "notFound");
    assert_eq!(
        c.issue("acme/game", 1).await.unwrap_err().code(),
        "unauthorized"
    );
    let e = c
        .issue_create(
            "acme/game",
            &IssueCreate {
                title: "x".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(e, ProviderError::Rejected { status: 422, .. }));
    let e = c.issue_labels("acme/game").await.unwrap_err();
    assert!(matches!(
        e,
        ProviderError::RateLimited {
            reset_at: Some(1_999_999_999),
            ..
        }
    ));
    let e = c.issue_assignable_users("acme/game").await.unwrap_err();
    assert_eq!(e.code(), "forbidden");
    assert!(c.issue("acme/game", 0).await.is_err());
    assert!(
        c.issue_list("not-a-repo", &IssueListQuery::default())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn gitlab_issues_use_encoded_path_and_iids() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/grp%2Fsub%2Fproj/issues"))
        .and(query_param("state", "opened"))
        .and(query_param("assignee_username", "bob"))
        .and(query_param("author_username", "alice"))
        .and(query_param("search", "crash"))
        .and(query_param("labels", "bug"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-next-page", "2")
                .insert_header("x-total", "31")
                .set_body_json(json!([gl_issue(3, "Crash")])),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/grp%2Fsub%2Fproj/issues"))
        .and(query_param("assignee_id", "None"))
        .and(query_param_is_missing("state"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "iid": 5, "title": "Plain labels", "state": "closed", "labels": ["a", "b"],
            "web_url": "https://gl.acme.dev/grp/sub/proj/-/issues/5"
        }])))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/grp%2Fsub%2Fproj/issues/3/notes"))
        .and(query_param("sort", "asc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 10, "author": {"id": 2, "username": "bob", "name": "Bob", "avatar_url": null},
             "body": "added ~bug label", "created_at": "2024-01-01T00:00:00Z", "system": true},
            {"id": 11, "author": null, "body": "hello", "created_at": null, "system": false}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/users"))
        .and(query_param("username", "bob"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 2, "username": "bob", "name": "Bob", "avatar_url": null}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/users"))
        .and(query_param("username", "ghost"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v4/projects/grp%2Fsub%2Fproj/issues"))
        .and(body_json(json!({
            "title": "New", "description": "", "labels": "bug,ui", "assignee_ids": [2]
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(gl_issue(6, "New")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api/v4/projects/grp%2Fsub%2Fproj/issues/6"))
        .and(body_json(
            json!({"state_event": "reopen", "assignee_ids": [0]}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(gl_issue(6, "New")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v4/projects/grp%2Fsub%2Fproj/issues/6/notes"))
        .and(body_json(json!({"body": "ok"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 12, "author": null, "body": "ok", "system": false
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/grp%2Fsub%2Fproj/members/all"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 2, "username": "bob", "name": "Bob", "state": "active"},
            {"id": 3, "username": "old", "name": "Old", "state": "blocked"},
            {"id": 2, "username": "bob", "name": "Bob", "state": "active"}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/grp%2Fsub%2Fproj/labels"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name": "bug", "color": "#FF0000", "description": "Bug"}
        ])))
        .mount(&server)
        .await;

    let c = gitlab(&server);
    let page = c
        .issue_list(
            "grp/sub/proj",
            &IssueListQuery {
                labels: vec!["bug".into()],
                assignee: Some("bob".into()),
                author: Some("alice".into()),
                search: Some("crash".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!((page.next_page, page.total), (Some(2), Some(31)));
    let i = &page.items[0];
    assert_eq!(i.number, 3);
    assert_eq!(i.comments, 4);
    assert_eq!(i.labels[0].color.as_deref(), Some("#d9534f"));
    let page = c
        .issue_list(
            "grp/sub/proj",
            &IssueListQuery {
                state: IssueStateFilter::All,
                assignee: Some(ASSIGNEE_NONE.into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items[0].state, IssueState::Closed);
    assert_eq!(page.items[0].labels.len(), 2);
    let notes = c.issue_comments("grp/sub/proj", 3).await.unwrap();
    assert!(notes.comments[0].system);
    assert_eq!(notes.comments[1].body, "hello");
    let e = c
        .issue_create(
            "grp/sub/proj",
            &IssueCreate {
                title: "New".into(),
                assignees: vec!["ghost".into()],
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(e, ProviderError::Rejected { status: 404, .. }));
    let made = c
        .issue_create(
            "grp/sub/proj",
            &IssueCreate {
                title: "New".into(),
                body: String::new(),
                labels: vec!["bug".into(), "ui".into()],
                assignees: vec!["bob".into()],
            },
        )
        .await
        .unwrap();
    assert_eq!(made.issue.number, 6);
    assert_eq!(made.body, "");
    c.issue_update(
        "grp/sub/proj",
        6,
        &IssueUpdate {
            state: Some(IssueState::Open),
            labels: None,
            assignees: Some(vec![]),
        },
    )
    .await
    .unwrap();
    c.issue_comment_create("grp/sub/proj", 6, "ok")
        .await
        .unwrap();
    let users = c.issue_assignable_users("grp/sub/proj").await.unwrap();
    assert_eq!(users.len(), 1);
    let labels = c.issue_labels("grp/sub/proj").await.unwrap();
    assert_eq!(labels[0].color.as_deref(), Some("#ff0000"));
}

#[tokio::test]
async fn github_my_work_runs_three_searches() {
    let server = MockServer::start().await;
    for (q, body) in [
        (
            "is:pr is:open review-requested:@me archived:false",
            json!({"total_count": 12, "items": [gh_issue(8, "Review me", true)]}),
        ),
        (
            "is:pr is:open author:@me archived:false",
            json!({"total_count": 0, "items": []}),
        ),
        (
            "is:issue is:open assignee:@me archived:false",
            json!({"total_count": 1, "items": [gh_issue(3, "Mine", false), {
                "number": 4, "title": "bad url", "repository_url": "https://x/nothing"
            }]}),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path("/api/v3/search/issues"))
            .and(query_param("q", q))
            .and(query_param("per_page", "30"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .expect(1)
            .mount(&server)
            .await;
    }
    let w = github(&server).my_work(30).await.unwrap();
    assert_eq!(w.review_requested.total, Some(12));
    let r = &w.review_requested.items[0];
    assert_eq!(r.kind, WorkItemKind::PullRequest);
    assert_eq!((r.full_name.as_str(), r.number), ("acme/game", 8));
    assert!(r.draft);
    assert_eq!(r.author.as_deref(), Some("alice"));
    assert!(w.authored.items.is_empty());
    assert_eq!(w.assigned.items.len(), 1);
    assert_eq!(w.assigned.items[0].kind, WorkItemKind::Issue);
    assert_eq!(w.assigned.items[0].labels, ["bug"]);
}

#[tokio::test]
async fn github_my_work_fails_as_a_whole_on_401() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message": "Bad credentials"})),
        )
        .mount(&server)
        .await;
    let e = github(&server).my_work(10).await.unwrap_err();
    assert_eq!(e.code(), "unauthorized");
}

#[tokio::test]
async fn gitlab_my_work_uses_reviewer_username() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 9, "username": "hoang", "name": "Hoàng"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/merge_requests"))
        .and(query_param("scope", "all"))
        .and(query_param("reviewer_username", "hoang"))
        .and(query_param("state", "opened"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-total", "1")
                .set_body_json(json!([{
                    "iid": 12, "title": "Draft: speed up", "author": {"username": "bob"},
                    "updated_at": "2024-02-01T00:00:00Z", "draft": true, "labels": ["perf"],
                    "user_notes_count": 3,
                    "web_url": "https://gl.acme.dev/grp/sub/proj/-/merge_requests/12",
                    "references": {"full": "grp/sub/proj!12"}
                }])),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/merge_requests"))
        .and(query_param("scope", "created_by_me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "iid": 2, "title": "Mine", "work_in_progress": false,
            "web_url": "https://gl.acme.dev/grp/proj/-/merge_requests/2"
        }])))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v4/issues"))
        .and(query_param("scope", "assigned_to_me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([gl_issue(3, "Fix")])))
        .expect(1)
        .mount(&server)
        .await;
    let w = gitlab(&server).my_work(20).await.unwrap();
    let r = &w.review_requested.items[0];
    assert_eq!(w.review_requested.total, Some(1));
    assert_eq!(
        (r.full_name.as_str(), r.number, r.draft),
        ("grp/sub/proj", 12, true)
    );
    assert_eq!(r.comments, Some(3));
    assert_eq!(w.authored.items[0].full_name, "grp/proj");
    assert_eq!(w.assigned.items[0].kind, WorkItemKind::Issue);
    assert_eq!(w.assigned.items[0].full_name, "grp/sub/proj");
}

#[tokio::test]
async fn gitlab_my_work_rate_limit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/user"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "30")
                .set_body_string("Retry later"),
        )
        .mount(&server)
        .await;
    let e = gitlab(&server).my_work(20).await.unwrap_err();
    assert!(matches!(
        e,
        ProviderError::RateLimited {
            retry_after_secs: Some(30),
            ..
        }
    ));
}
