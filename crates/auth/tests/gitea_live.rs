//! Round trip against a real Gitea / Forgejo server: sign in with an access token, then
//! everything the app does with a hosted repository (read it, open a pull request, comment,
//! review, read its CI, merge it, issues).
//!
//! Ignored by default. Point it at a throw-away server and a token with full access:
//!
//! ```text
//! NANOMGG_TEST_GITEA_URL=http://127.0.0.1:3999 NANOMGG_TEST_GITEA_TOKEN=… \
//!   cargo test -p nanomgg-auth --test gitea_live -- --ignored --nocapture
//! ```
//!
//! It creates a repository named `nanomgg-live-<time>` for the token's user.

use base64::Engine;
use nanomgg_auth::*;
use nanomgg_providers::issues::*;
use nanomgg_providers::pulls::*;
use nanomgg_providers::{
    HttpConfig, Provider, ProviderClient, ProviderEndpoint, ProviderKind, SecretToken,
    build_http_client,
};
use reqwest::Method;
use serde_json::{Value, json};

struct Steps(Vec<String>);

impl Steps {
    /// Records a failed step and carries on, so one run shows every incompatibility.
    fn check<T, E: std::fmt::Display>(&mut self, what: &str, result: Result<T, E>) -> Option<T> {
        match result {
            Ok(value) => {
                println!("ok    {what}");
                Some(value)
            }
            Err(e) => {
                println!("FAIL  {what}: {e}");
                self.0.push(format!("{what}: {e}"));
                None
            }
        }
    }

    fn expect(&mut self, what: &str, ok: bool) {
        self.check(what, if ok { Ok(()) } else { Err("unexpected value") });
    }
}

#[tokio::test]
#[ignore = "needs a real Gitea / Forgejo server (NANOMGG_TEST_GITEA_URL / _TOKEN)"]
async fn real_gitea_account_round_trip() {
    let url = std::env::var("NANOMGG_TEST_GITEA_URL").expect("NANOMGG_TEST_GITEA_URL");
    let token = std::env::var("NANOMGG_TEST_GITEA_TOKEN").expect("NANOMGG_TEST_GITEA_TOKEN");
    let http = build_http_client(&HttpConfig::default()).unwrap();
    let endpoint = ProviderEndpoint::gitea(&url).unwrap();
    assert_eq!(endpoint.kind(), ProviderKind::Gitea);
    let token = SecretToken::new(token.trim());
    let mut steps = Steps(Vec::new());

    // Sign-in: the same check "Add account" runs.
    let info = validate_token(&http, &endpoint, &token)
        .await
        .expect("the token is accepted");
    let user = info.user.username.clone();
    println!("signed in as {user}");
    let wrong = validate_token(&http, &endpoint, &SecretToken::new("wrong-token")).await;
    steps.expect("a wrong token is refused", wrong.is_err());

    let c = ProviderClient::new(http.clone(), endpoint.clone(), token.clone());
    let stamp = nanomgg_providers::now_unix();
    let name = format!("nanomgg-live-{stamp}");
    let full = format!("{user}/{name}");

    // A repository with one commit, then a branch with a change (server-side, no git needed).
    let created: Value = c
        .api()
        .send_json(
            Method::POST,
            &["user", "repos"],
            &json!({"name": name, "auto_init": true, "default_branch": "main"}),
        )
        .await
        .expect("create the test repository")
        .body;
    assert_eq!(created["full_name"], json!(full));
    let content = base64::engine::general_purpose::STANDARD.encode("hello from the live test\n");
    let file: Value = c
        .api()
        .send_json(
            Method::POST,
            &["repos", &user, &name, "contents", "feature.txt"],
            &json!({
                "content": content, "message": "Add feature.txt",
                "branch": "main", "new_branch": "feature/live-test"
            }),
        )
        .await
        .expect("commit a file on a new branch")
        .body;
    let head_sha = file["commit"]["sha"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(head_sha.len() >= 40, "{file}");

    if let Some(repo) = steps.check("read the repository", c.repository(&full).await) {
        steps.expect(
            "repository: default branch",
            repo.default_branch.as_deref() == Some("main"),
        );
        steps.expect(
            "repository: clone url",
            repo.clone_https.ends_with(&format!("{full}.git")),
        );
    }
    steps.check("list repositories", c.list_repositories_page(1, 20).await);
    steps.check("default branch", c.pull_default_branch(&full).await);

    // Pull request: create, list, read every tab of its page.
    let created = steps.check(
        "create a pull request",
        c.pull_create(
            &full,
            &PullCreate {
                source_branch: "feature/live-test".into(),
                target_branch: "main".into(),
                title: "Live test".into(),
                body: "Opened by the live test.".into(),
                draft: false,
                reviewers: vec![],
                labels: vec![],
                remove_source_branch: false,
            },
        )
        .await,
    );
    let number = created.map_or(1, |p| p.number);
    if let Some(page) = steps.check(
        "list pull requests",
        c.pull_list(&full, &PullListQuery::default()).await,
    ) {
        steps.expect(
            "list: the new pull request is there",
            page.items.iter().any(|p| p.number == number),
        );
    }
    if let Some(detail) = steps.check("pull request detail", c.pull_detail(&full, number).await) {
        steps.expect(
            "detail: branches",
            detail.source_branch == "feature/live-test" && detail.target_branch == "main",
        );
        steps.expect("detail: open", detail.state == PullState::Open);
    }
    if let Some(commits) = steps.check("pull request commits", c.pull_commits(&full, number).await)
    {
        steps.expect("commits: one commit", commits.len() == 1);
    }
    if let Some(files) = steps.check("pull request files", c.pull_files(&full, number).await) {
        steps.expect(
            "files: feature.txt",
            files.files.iter().any(|f| f.path == "feature.txt"),
        );
    }
    steps.check("pull request reviews", c.pull_reviews(&full, number).await);
    steps.check(
        "comment on the pull request",
        c.pull_comment(
            &full,
            number,
            &PullCommentTarget::General,
            "A comment from the live test.",
        )
        .await,
    );
    if let Some(comments) = steps.check(
        "pull request comments",
        c.pull_comments(&full, number).await,
    ) {
        steps.expect(
            "comments: the new comment is there",
            comments.iter().any(|m| m.body.contains("live test")),
        );
    }
    steps.check(
        "review the pull request (comment)",
        c.pull_review(
            &full,
            number,
            PullReviewEvent::Comment,
            "Reviewed by the live test.",
        )
        .await,
    );

    // A second user (needs an admin token): comments on a line and approves.
    let reviewer = format!("rev{stamp}");
    let password = format!("Rv-{stamp}-{}x", std::process::id());
    let made: Result<_, _> = c
        .api()
        .send_json::<Value, _>(
            Method::POST,
            &["admin", "users"],
            &json!({"username": reviewer, "email": format!("{reviewer}@example.test"),
                    "password": password, "must_change_password": false}),
        )
        .await;
    if steps.check("create a second user", made).is_some() {
        let added: Result<_, _> = c
            .api()
            .send_json::<Value, _>(
                Method::PUT,
                &["repos", &user, &name, "collaborators", &reviewer],
                &json!({"permission": "write"}),
            )
            .await;
        steps.check("add the second user to the repository", added);
        let token_url = endpoint.api(&["users", &reviewer, "tokens"]);
        let answer: Value = http
            .post(token_url)
            .basic_auth(&reviewer, Some(&password))
            .json(&json!({"name": "live-test", "scopes": ["all"]}))
            .send()
            .await
            .expect("ask for the second user's token")
            .json()
            .await
            .expect("token answer");
        let second = answer["sha1"].as_str().unwrap_or_default().to_owned();
        steps.expect("the second user got a token", !second.is_empty());
        let info2 = validate_token(&http, &endpoint, &SecretToken::new(second.clone())).await;
        steps.expect(
            "the second user signs in",
            info2.is_ok_and(|i| i.user.username == reviewer),
        );
        let c2 = ProviderClient::new(http.clone(), endpoint.clone(), SecretToken::new(second));
        let line = steps.check(
            "comment on a line of the diff",
            c2.pull_comment(
                &full,
                number,
                &PullCommentTarget::Line {
                    path: "feature.txt".into(),
                    old_path: None,
                    line: 1,
                    side: PullSide::New,
                    paired_line: None,
                },
                "This line looks good.",
            )
            .await,
        );
        if let Some(line) = line {
            steps.expect(
                "line comment: kind and place",
                line.kind == PullCommentKind::Line
                    && line
                        .anchor
                        .as_ref()
                        .is_some_and(|a| a.path == "feature.txt" && a.line == Some(1)),
            );
        }
        steps.check(
            "approve the pull request",
            c2.pull_review(
                &full,
                number,
                PullReviewEvent::Approve,
                "Approved by the live test.",
            )
            .await,
        );
        if let Some(comments) = steps.check(
            "comments with the line comment",
            c.pull_comments(&full, number).await,
        ) {
            steps.expect(
                "comments: the line comment is listed with its place",
                comments.iter().any(|m| {
                    m.kind == PullCommentKind::Line
                        && m.body.contains("looks good")
                        && m.anchor
                            .as_ref()
                            .is_some_and(|a| a.path == "feature.txt" && a.side == PullSide::New)
                }),
            );
            steps.expect(
                "comments: the approval is listed",
                comments
                    .iter()
                    .any(|m| m.review_state == Some(PullReviewerState::Approved)),
            );
        }
        if let Some(reviews) = steps.check(
            "reviews after the approval",
            c.pull_reviews(&full, number).await,
        ) {
            steps.expect(
                "reviews: approved by the second user",
                reviews
                    .reviewers
                    .iter()
                    .any(|r| r.user.username == reviewer && r.state == PullReviewerState::Approved),
            );
            println!(
                "      decision: {:?}, reviewers: {:?}",
                reviews.decision, reviews.reviewers
            );
        }
        let reply = c
            .pull_comment(
                &full,
                number,
                &PullCommentTarget::Reply {
                    thread_id: "1".into(),
                },
                "reply",
            )
            .await;
        steps.expect(
            "a reply in a line thread is refused with an explanation",
            reply.is_err(),
        );
    }

    // CI: nothing yet, then one commit status.
    if let Some(ci) = steps.check(
        "CI without statuses",
        c.ci_status(&full, &head_sha, true).await,
    ) {
        steps.expect("CI: no jobs yet", ci.jobs.is_empty());
    }
    let posted: Result<_, _> = c
        .api()
        .send_json::<Value, _>(
            Method::POST,
            &["repos", &user, &name, "statuses", &head_sha],
            &json!({"state": "success", "context": "ci/live-test", "description": "ok",
                    "target_url": "https://example.test/build/1"}),
        )
        .await;
    steps.check("post a commit status", posted);
    if let Some(ci) = steps.check(
        "CI with a status",
        c.ci_status(&full, &head_sha, true).await,
    ) {
        steps.expect(
            "CI: one successful job",
            ci.jobs.len() == 1 && ci.jobs[0].name == "ci/live-test",
        );
        println!(
            "      CI state: {:?}, job: {:?}",
            ci.state,
            ci.jobs.first().map(|j| j.state)
        );
    }

    // Issues.
    let issue = steps.check(
        "create an issue",
        c.issue_create(
            &full,
            &IssueCreate {
                title: "Live test issue".into(),
                body: "Body".into(),
                labels: vec![],
                assignees: vec![],
            },
        )
        .await,
    );
    if let Some(issue) = issue {
        steps.check("read the issue", c.issue(&full, issue.issue.number).await);
        steps.check(
            "comment on the issue",
            c.issue_comment_create(&full, issue.issue.number, "Noted.")
                .await,
        );
        steps.check(
            "issue comments",
            c.issue_comments(&full, issue.issue.number).await,
        );
    }
    if let Some(page) = steps.check(
        "list issues",
        c.issue_list(&full, &IssueListQuery::default()).await,
    ) {
        steps.expect(
            "issues: only the issue, not the pull request",
            page.items.len() == 1,
        );
    }
    steps.check("issue labels", c.issue_labels(&full).await);
    steps.check("assignable users", c.issue_assignable_users(&full).await);

    // Merge and delete the branch.
    if let Some(merged) = steps.check(
        "merge the pull request",
        c.pull_merge(
            &full,
            number,
            &PullMergeRequest {
                method: PullMergeMethod::Merge,
                delete_source_branch: true,
                expected_head_sha: Some(head_sha.clone()),
            },
        )
        .await,
    ) {
        steps.expect("merge: commit id returned", merged.sha.is_some());
        steps.expect("merge: source branch deleted", merged.branch_deleted);
        if let Some(e) = &merged.delete_error {
            println!("      delete error: {e}");
        }
    }
    if let Some(detail) = steps.check("detail after the merge", c.pull_detail(&full, number).await)
    {
        steps.expect("detail: merged", detail.state == PullState::Merged);
    }

    assert!(
        steps.0.is_empty(),
        "{} step(s) failed:\n{}",
        steps.0.len(),
        steps.0.join("\n")
    );
}
