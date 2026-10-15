//! GitHub pull requests (REST v3). Filtered listings use the issue search API because
//! `GET /repos/{o}/{r}/pulls` cannot filter by author / reviewer / label.

use std::collections::BTreeMap;

use reqwest::Method;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;

use super::*;
use crate::github::GithubClient;
use crate::timefmt::parse_timestamp;

/// GitHub lists at most 3000 files per pull request (30 pages of 100).
const MAX_FILE_PAGES: u32 = 30;

fn repo_segments(full_name: &str) -> Result<(&str, &str), ProviderError> {
    match full_name.trim_matches('/').split_once('/') {
        Some((owner, name)) if !owner.is_empty() && !name.is_empty() && !name.contains('/') => {
            Ok((owner, name.strip_suffix(".git").unwrap_or(name)))
        }
        _ => Err(ProviderError::InvalidInput {
            message: format!("`{full_name}` is not an owner/repo name"),
        }),
    }
}

fn ts(v: &Option<String>) -> Option<i64> {
    v.as_deref().and_then(parse_timestamp)
}

impl GithubClient {
    /// Every page of a list endpoint (at most `max_pages`); `true` when more pages remained.
    async fn get_all<T: DeserializeOwned>(
        &self,
        segments: &[&str],
        max_pages: u32,
    ) -> Result<(Vec<T>, bool), ProviderError> {
        let mut out = Vec::new();
        let mut page = 1u32;
        for _ in 0..max_pages {
            let page_s = page.to_string();
            let resp = self
                .api()
                .get_json::<Vec<T>>(segments, &[("per_page", "100"), ("page", &page_s)])
                .await?;
            let next = resp.next_page();
            out.extend(resp.body);
            match next {
                Some(n) if n > page => page = n,
                _ => return Ok((out, false)),
            }
        }
        Ok((out, true))
    }

    async fn raw_pull(&self, full_name: &str, number: u64) -> Result<RawPull, ProviderError> {
        let (o, r) = repo_segments(full_name)?;
        let n = number.to_string();
        Ok(self
            .api()
            .get_json::<RawPull>(&["repos", o, r, "pulls", &n], &[])
            .await?
            .body)
    }

    pub(crate) async fn pull_list(
        &self,
        full_name: &str,
        query: &PullListQuery,
    ) -> Result<PullPage, ProviderError> {
        let (o, r) = repo_segments(full_name)?;
        let page = query.page();
        let per_page = query.per_page();
        let use_pulls_api =
            !query.has_people_or_label_filter() && query.state != PullStateFilter::Merged;
        if use_pulls_api {
            let state = match query.state {
                PullStateFilter::Open => "open",
                PullStateFilter::Closed => "closed",
                _ => "all",
            };
            let resp = self
                .api()
                .get_json::<Vec<RawPull>>(
                    &["repos", o, r, "pulls"],
                    &[
                        ("state", state),
                        ("sort", "updated"),
                        ("direction", "desc"),
                        ("per_page", &per_page),
                        ("page", &page),
                    ],
                )
                .await?;
            let next_page = resp.next_page();
            return Ok(PullPage {
                items: resp.body.into_iter().map(RawPull::into_summary).collect(),
                next_page,
                total: None,
            });
        }
        let mut q = format!("repo:{o}/{r} is:pr");
        q.push_str(match query.state {
            PullStateFilter::Open => " is:open",
            PullStateFilter::Closed => " is:closed is:unmerged",
            PullStateFilter::Merged => " is:merged",
            PullStateFilter::All => "",
        });
        let quote = |v: &str| {
            if v.contains(char::is_whitespace) {
                format!("\"{}\"", v.replace('"', ""))
            } else {
                v.to_owned()
            }
        };
        for (key, value) in [
            ("author", &query.author),
            ("review-requested", &query.reviewer),
            ("assignee", &query.assignee),
            ("label", &query.label),
        ] {
            if let Some(v) = PullListQuery::clean(value) {
                q.push_str(&format!(" {key}:{}", quote(v)));
            }
        }
        let resp = self
            .api()
            .get_json::<RawSearch>(
                &["search", "issues"],
                &[
                    ("q", &q),
                    ("sort", "updated"),
                    ("order", "desc"),
                    ("per_page", &per_page),
                    ("page", &page),
                ],
            )
            .await?;
        let next_page = resp.next_page();
        Ok(PullPage {
            total: Some(resp.body.total_count),
            items: resp
                .body
                .items
                .into_iter()
                .map(RawIssue::into_summary)
                .collect(),
            next_page,
        })
    }

    pub(crate) async fn pull_detail(
        &self,
        full_name: &str,
        number: u64,
    ) -> Result<PullDetail, ProviderError> {
        let (o, r) = repo_segments(full_name)?;
        let repo_segs = ["repos", o, r];
        let (pull, repo) = tokio::join!(
            self.raw_pull(full_name, number),
            self.api().get_json::<RawRepoSettings>(&repo_segs, &[])
        );
        let pull = pull?;
        // The settings only come back for accounts with push access; offer everything otherwise
        // (GitHub refuses a disallowed method with a clear message).
        let settings = repo.map(|r| r.body).unwrap_or_default();
        let mut methods = Vec::new();
        if settings.allow_merge_commit.unwrap_or(true) {
            methods.push(PullMergeMethod::Merge);
        }
        if settings.allow_squash_merge.unwrap_or(true) {
            methods.push(PullMergeMethod::Squash);
        }
        if settings.allow_rebase_merge.unwrap_or(true) {
            methods.push(PullMergeMethod::Rebase);
        }
        Ok(pull.into_detail(
            &format!("{o}/{r}"),
            methods,
            settings.delete_branch_on_merge.unwrap_or(false),
        ))
    }

    pub(crate) async fn pull_reviews(
        &self,
        full_name: &str,
        number: u64,
    ) -> Result<PullReviews, ProviderError> {
        let (o, r) = repo_segments(full_name)?;
        let n = number.to_string();
        let reviews_segs = ["repos", o, r, "pulls", &n, "reviews"];
        let requested_segs = ["repos", o, r, "pulls", &n, "requested_reviewers"];
        let (reviews, requested) = tokio::join!(
            self.get_all::<RawReview>(&reviews_segs, MAX_PAGES),
            self.api().get_json::<RawRequested>(&requested_segs, &[])
        );
        let (reviews, _) = reviews?;
        let requested = requested?.body;
        // Latest verdict per reviewer; a plain comment never hides an earlier verdict.
        let mut by_user: BTreeMap<String, (PullUser, PullReviewerState)> = BTreeMap::new();
        for review in reviews {
            let (Some(user), Some(state)) = (review.user, review_state(&review.state)) else {
                continue;
            };
            let key = user.login.to_ascii_lowercase();
            match by_user.get_mut(&key) {
                Some(_) if state == PullReviewerState::Commented => {}
                Some(entry) => entry.1 = state,
                None => {
                    by_user.insert(key, (user.into(), state));
                }
            }
        }
        // Re-requested reviewers are pending again.
        for user in requested.users {
            by_user.insert(
                user.login.to_ascii_lowercase(),
                (user.into(), PullReviewerState::Requested),
            );
        }
        let reviewers: Vec<PullReviewer> = by_user
            .into_values()
            .map(|(user, state)| PullReviewer { user, state })
            .collect();
        Ok(PullReviews {
            decision: decision_of(&reviewers),
            reviewers,
            teams: requested
                .teams
                .into_iter()
                .map(|t| t.name.unwrap_or(t.slug))
                .collect(),
            approvals_left: None,
            approved_by_me: None,
            approvals_unavailable: false,
        })
    }

    pub(crate) async fn pull_commits(
        &self,
        full_name: &str,
        number: u64,
    ) -> Result<Vec<PullCommit>, ProviderError> {
        let (o, r) = repo_segments(full_name)?;
        let n = number.to_string();
        let (commits, _) = self
            .get_all::<RawCommit>(&["repos", o, r, "pulls", &n, "commits"], MAX_PAGES)
            .await?;
        Ok(commits
            .into_iter()
            .map(|c| PullCommit {
                title: first_line(&c.commit.message),
                author_name: c
                    .commit
                    .author
                    .as_ref()
                    .map(|a| a.name.clone())
                    .unwrap_or_default(),
                authored_at: c.commit.author.as_ref().and_then(|a| ts(&a.date)),
                author_login: c.author.map(|a| a.login),
                message: c.commit.message,
                sha: c.sha,
            })
            .collect())
    }

    pub(crate) async fn pull_files(
        &self,
        full_name: &str,
        number: u64,
    ) -> Result<PullFiles, ProviderError> {
        let (o, r) = repo_segments(full_name)?;
        let n = number.to_string();
        let (files, more) = self
            .get_all::<RawFile>(&["repos", o, r, "pulls", &n, "files"], MAX_FILE_PAGES)
            .await?;
        let truncated = more || files.len() >= 3000;
        Ok(PullFiles {
            files: files
                .into_iter()
                .map(|f| PullFile {
                    status: match f.status.as_str() {
                        "added" => PullFileStatus::Added,
                        "removed" => PullFileStatus::Deleted,
                        "renamed" => PullFileStatus::Renamed,
                        "copied" => PullFileStatus::Copied,
                        "modified" => PullFileStatus::Modified,
                        _ => PullFileStatus::Changed,
                    },
                    path: f.filename,
                    old_path: f.previous_filename,
                    additions: f.additions,
                    deletions: f.deletions,
                    patch: f.patch.filter(|p| !p.is_empty()),
                })
                .collect(),
            truncated,
        })
    }

    pub(crate) async fn pull_comments(
        &self,
        full_name: &str,
        number: u64,
    ) -> Result<Vec<PullComment>, ProviderError> {
        let (o, r) = repo_segments(full_name)?;
        let n = number.to_string();
        let issue_segs = ["repos", o, r, "issues", &n, "comments"];
        let review_segs = ["repos", o, r, "pulls", &n, "reviews"];
        let line_segs = ["repos", o, r, "pulls", &n, "comments"];
        let (general, reviews, lines) = tokio::join!(
            self.get_all::<RawIssueComment>(&issue_segs, MAX_PAGES),
            self.get_all::<RawReview>(&review_segs, MAX_PAGES),
            self.get_all::<RawLineComment>(&line_segs, MAX_PAGES)
        );
        let mut out: Vec<PullComment> = general?.0.into_iter().map(Into::into).collect();
        for review in reviews?.0 {
            let state = review_state(&review.state);
            let body = review.body.unwrap_or_default();
            // Reviews that only carry line comments have no body: the line comments show them.
            let verdict = matches!(
                state,
                Some(PullReviewerState::Approved | PullReviewerState::ChangesRequested)
            );
            if state.is_none() || (body.trim().is_empty() && !verdict) {
                continue;
            }
            out.push(PullComment {
                id: format!("review-{}", review.id),
                kind: PullCommentKind::Review,
                author: review.user.map(Into::into),
                body,
                created_at: ts(&review.submitted_at),
                updated_at: None,
                thread_id: None,
                anchor: None,
                review_state: state,
                resolved: None,
                web_url: review.html_url,
            });
        }
        out.extend(lines?.0.into_iter().map(Into::into));
        out.sort_by_key(|c| c.created_at.unwrap_or(i64::MAX));
        Ok(out)
    }

    pub(crate) async fn pull_comment(
        &self,
        full_name: &str,
        number: u64,
        target: &PullCommentTarget,
        body: &str,
    ) -> Result<PullComment, ProviderError> {
        let (o, r) = repo_segments(full_name)?;
        let n = number.to_string();
        match target {
            PullCommentTarget::General => Ok(self
                .api()
                .send_json::<RawIssueComment, _>(
                    Method::POST,
                    &["repos", o, r, "issues", &n, "comments"],
                    &json!({ "body": body }),
                )
                .await?
                .body
                .into()),
            PullCommentTarget::Reply { thread_id } => {
                let id = thread_id.trim();
                if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(ProviderError::InvalidInput {
                        message: format!("invalid comment id `{thread_id}`"),
                    });
                }
                Ok(self
                    .api()
                    .send_json::<RawLineComment, _>(
                        Method::POST,
                        &["repos", o, r, "pulls", &n, "comments", id, "replies"],
                        &json!({ "body": body }),
                    )
                    .await?
                    .body
                    .into())
            }
            PullCommentTarget::Line {
                path, line, side, ..
            } => {
                let head = self.raw_pull(full_name, number).await?.head.sha;
                let side = match side {
                    PullSide::Old => "LEFT",
                    PullSide::New => "RIGHT",
                };
                Ok(self
                    .api()
                    .send_json::<RawLineComment, _>(
                        Method::POST,
                        &["repos", o, r, "pulls", &n, "comments"],
                        &json!({
                            "body": body, "commit_id": head, "path": path,
                            "line": line, "side": side
                        }),
                    )
                    .await?
                    .body
                    .into())
            }
        }
    }

    pub(crate) async fn pull_review(
        &self,
        full_name: &str,
        number: u64,
        event: PullReviewEvent,
        body: &str,
    ) -> Result<(), ProviderError> {
        let (o, r) = repo_segments(full_name)?;
        let n = number.to_string();
        let event = match event {
            PullReviewEvent::Approve => "APPROVE",
            PullReviewEvent::RequestChanges => "REQUEST_CHANGES",
            PullReviewEvent::Comment => "COMMENT",
            PullReviewEvent::Unapprove => {
                return Err(ProviderError::Unsupported {
                    message: "GitHub reviews cannot be withdrawn; submit a new review".into(),
                });
            }
        };
        let mut payload = json!({ "event": event });
        if !body.trim().is_empty() {
            payload["body"] = json!(body);
        }
        self.api()
            .send_json::<serde_json::Value, _>(
                Method::POST,
                &["repos", o, r, "pulls", &n, "reviews"],
                &payload,
            )
            .await?;
        Ok(())
    }

    pub(crate) async fn pull_create(
        &self,
        full_name: &str,
        create: &PullCreate,
    ) -> Result<PullCreated, ProviderError> {
        let (o, r) = repo_segments(full_name)?;
        let pull = self
            .api()
            .send_json::<RawPull, _>(
                Method::POST,
                &["repos", o, r, "pulls"],
                &json!({
                    "title": create.title.trim(),
                    "head": create.source_branch,
                    "base": create.target_branch,
                    "body": create.body,
                    "draft": create.draft,
                }),
            )
            .await?
            .body;
        let n = pull.number.to_string();
        let mut warnings = Vec::new();
        let reviewers: Vec<&str> = clean_list(&create.reviewers);
        if !reviewers.is_empty()
            && let Err(e) = self
                .api()
                .send_json::<serde_json::Value, _>(
                    Method::POST,
                    &["repos", o, r, "pulls", &n, "requested_reviewers"],
                    &json!({ "reviewers": reviewers }),
                )
                .await
        {
            warnings.push(format!("reviewers: {e}"));
        }
        let labels: Vec<&str> = clean_list(&create.labels);
        if !labels.is_empty()
            && let Err(e) = self
                .api()
                .send_json::<serde_json::Value, _>(
                    Method::POST,
                    &["repos", o, r, "issues", &n, "labels"],
                    &json!({ "labels": labels }),
                )
                .await
        {
            warnings.push(format!("labels: {e}"));
        }
        Ok(PullCreated {
            number: pull.number,
            web_url: pull.html_url,
            warnings,
        })
    }

    pub(crate) async fn pull_merge(
        &self,
        full_name: &str,
        number: u64,
        request: &PullMergeRequest,
    ) -> Result<PullMerged, ProviderError> {
        let (o, r) = repo_segments(full_name)?;
        let n = number.to_string();
        let pull = self.raw_pull(full_name, number).await?;
        let mut payload = json!({
            "merge_method": match request.method {
                PullMergeMethod::Merge => "merge",
                PullMergeMethod::Squash => "squash",
                PullMergeMethod::Rebase => "rebase",
            }
        });
        if let Some(sha) = request
            .expected_head_sha
            .as_deref()
            .filter(|s| !s.is_empty())
        {
            payload["sha"] = json!(sha);
        }
        let merged = self
            .api()
            .send_json::<RawMergeResult, _>(
                Method::PUT,
                &["repos", o, r, "pulls", &n, "merge"],
                &payload,
            )
            .await?
            .body;
        let mut result = PullMerged {
            sha: merged.sha,
            branch_deleted: false,
            delete_error: None,
        };
        if request.delete_source_branch {
            match pull.head.repo.as_ref().map(|r| r.full_name.as_str()) {
                Some(head_repo) => match self.delete_branch(head_repo, &pull.head.r#ref).await {
                    Ok(()) => result.branch_deleted = true,
                    Err(e) => result.delete_error = Some(e.to_string()),
                },
                None => result.delete_error = Some("the source repository was deleted".into()),
            }
        }
        Ok(result)
    }

    async fn delete_branch(&self, full_name: &str, branch: &str) -> Result<(), ProviderError> {
        let (o, r) = repo_segments(full_name)?;
        let mut segs = vec!["repos", o, r, "git", "refs", "heads"];
        segs.extend(branch.split('/').filter(|s| !s.is_empty()));
        let url = self.api().url(&segs, &[]);
        self.api()
            .send::<serde_json::Value, ()>(Method::DELETE, url, None)
            .await?;
        Ok(())
    }

    pub(crate) async fn pull_set_open(
        &self,
        full_name: &str,
        number: u64,
        open: bool,
    ) -> Result<(), ProviderError> {
        let (o, r) = repo_segments(full_name)?;
        let n = number.to_string();
        self.api()
            .send_json::<serde_json::Value, _>(
                Method::PATCH,
                &["repos", o, r, "pulls", &n],
                &json!({ "state": if open { "open" } else { "closed" } }),
            )
            .await?;
        Ok(())
    }
}

fn clean_list(v: &[String]) -> Vec<&str> {
    v.iter()
        .map(|s| s.trim().trim_start_matches('@'))
        .filter(|s| !s.is_empty())
        .collect()
}

fn review_state(state: &str) -> Option<PullReviewerState> {
    Some(match state {
        "APPROVED" => PullReviewerState::Approved,
        "CHANGES_REQUESTED" => PullReviewerState::ChangesRequested,
        "COMMENTED" => PullReviewerState::Commented,
        "DISMISSED" => PullReviewerState::Dismissed,
        // PENDING: the token owner's own unsubmitted draft review.
        _ => return None,
    })
}

#[derive(Deserialize)]
struct RawUser {
    login: String,
    avatar_url: Option<String>,
    html_url: Option<String>,
}

impl From<RawUser> for PullUser {
    fn from(u: RawUser) -> Self {
        Self {
            username: u.login,
            display_name: None,
            avatar_url: u.avatar_url,
            web_url: u.html_url,
        }
    }
}

#[derive(Deserialize)]
struct RawLabel {
    name: String,
}

#[derive(Deserialize)]
struct RawRefRepo {
    full_name: String,
}

#[derive(Deserialize)]
struct RawRef {
    r#ref: String,
    sha: String,
    repo: Option<RawRefRepo>,
}

#[derive(Deserialize)]
struct RawPull {
    number: u64,
    title: String,
    body: Option<String>,
    state: String,
    #[serde(default)]
    draft: bool,
    merged_at: Option<String>,
    user: Option<RawUser>,
    head: RawRef,
    base: RawRef,
    #[serde(default)]
    labels: Vec<RawLabel>,
    #[serde(default)]
    assignees: Vec<RawUser>,
    comments: Option<u32>,
    review_comments: Option<u32>,
    commits: Option<u32>,
    additions: Option<u32>,
    deletions: Option<u32>,
    changed_files: Option<u32>,
    mergeable: Option<bool>,
    mergeable_state: Option<String>,
    merged_by: Option<RawUser>,
    created_at: Option<String>,
    updated_at: Option<String>,
    closed_at: Option<String>,
    #[serde(default)]
    html_url: String,
}

impl RawPull {
    fn state(&self) -> PullState {
        if self.merged_at.is_some() {
            PullState::Merged
        } else if self.state == "open" {
            PullState::Open
        } else {
            PullState::Closed
        }
    }

    fn into_summary(self) -> PullSummary {
        PullSummary {
            state: self.state(),
            number: self.number,
            title: self.title,
            draft: self.draft,
            author: self.user.map(Into::into),
            source_branch: Some(self.head.r#ref),
            target_branch: Some(self.base.r#ref),
            labels: self.labels.into_iter().map(|l| l.name).collect(),
            comments: self.comments,
            created_at: ts(&self.created_at),
            updated_at: ts(&self.updated_at),
            web_url: self.html_url,
        }
    }

    fn merge_state(&self) -> PullMergeState {
        if self.state() != PullState::Open {
            return PullMergeState::Unknown;
        }
        if self.mergeable == Some(false) {
            return PullMergeState::Conflicts;
        }
        match self.mergeable_state.as_deref() {
            Some("clean") | Some("has_hooks") => PullMergeState::Clean,
            Some("dirty") => PullMergeState::Conflicts,
            Some("blocked") => PullMergeState::Blocked,
            Some("behind") => PullMergeState::Behind,
            Some("unstable") => PullMergeState::Unstable,
            Some("draft") => PullMergeState::Draft,
            _ if self.draft => PullMergeState::Draft,
            _ => PullMergeState::Checking,
        }
    }

    fn into_detail(
        self,
        target_repo: &str,
        merge_methods: Vec<PullMergeMethod>,
        delete_source_default: bool,
    ) -> PullDetail {
        let state = self.state();
        let merge_state = self.merge_state();
        let source_repo = self.head.repo.map(|r| r.full_name);
        let base_repo = self
            .base
            .repo
            .map(|r| r.full_name)
            .unwrap_or_else(|| target_repo.to_owned());
        PullDetail {
            number: self.number,
            title: self.title,
            body: self.body.unwrap_or_default(),
            state,
            draft: self.draft,
            author: self.user.map(Into::into),
            source_branch: self.head.r#ref,
            target_branch: self.base.r#ref,
            from_fork: source_repo
                .as_deref()
                .is_none_or(|s| !s.eq_ignore_ascii_case(&base_repo)),
            source_repo,
            target_repo: base_repo,
            head_sha: self.head.sha,
            base_sha: Some(self.base.sha),
            start_sha: None,
            merge_state,
            merge_state_raw: self.mergeable_state,
            merge_methods,
            delete_source_default,
            labels: self.labels.into_iter().map(|l| l.name).collect(),
            assignees: self.assignees.into_iter().map(Into::into).collect(),
            comments: self.comments.unwrap_or(0),
            review_comments: self.review_comments.unwrap_or(0),
            commits: self.commits,
            additions: self.additions,
            deletions: self.deletions,
            changed_files: self.changed_files,
            created_at: ts(&self.created_at),
            updated_at: ts(&self.updated_at),
            merged_at: ts(&self.merged_at),
            closed_at: ts(&self.closed_at),
            merged_by: self.merged_by.map(Into::into),
            web_url: self.html_url,
        }
    }
}

#[derive(Deserialize, Default)]
struct RawRepoSettings {
    allow_merge_commit: Option<bool>,
    allow_squash_merge: Option<bool>,
    allow_rebase_merge: Option<bool>,
    delete_branch_on_merge: Option<bool>,
}

#[derive(Deserialize)]
struct RawSearch {
    total_count: u64,
    items: Vec<RawIssue>,
}

#[derive(Deserialize)]
struct RawIssuePull {
    merged_at: Option<String>,
}

#[derive(Deserialize)]
struct RawIssue {
    number: u64,
    title: String,
    state: String,
    #[serde(default)]
    draft: bool,
    user: Option<RawUser>,
    #[serde(default)]
    labels: Vec<RawLabel>,
    comments: Option<u32>,
    created_at: Option<String>,
    updated_at: Option<String>,
    #[serde(default)]
    html_url: String,
    pull_request: Option<RawIssuePull>,
}

impl RawIssue {
    fn into_summary(self) -> PullSummary {
        let merged = self
            .pull_request
            .as_ref()
            .is_some_and(|p| p.merged_at.is_some());
        PullSummary {
            number: self.number,
            title: self.title,
            state: if merged {
                PullState::Merged
            } else if self.state == "open" {
                PullState::Open
            } else {
                PullState::Closed
            },
            draft: self.draft,
            author: self.user.map(Into::into),
            source_branch: None,
            target_branch: None,
            labels: self.labels.into_iter().map(|l| l.name).collect(),
            comments: self.comments,
            created_at: ts(&self.created_at),
            updated_at: ts(&self.updated_at),
            web_url: self.html_url,
        }
    }
}

#[derive(Deserialize)]
struct RawReview {
    id: u64,
    user: Option<RawUser>,
    body: Option<String>,
    state: String,
    submitted_at: Option<String>,
    html_url: Option<String>,
}

#[derive(Deserialize)]
struct RawTeam {
    slug: String,
    name: Option<String>,
}

#[derive(Deserialize)]
struct RawRequested {
    #[serde(default)]
    users: Vec<RawUser>,
    #[serde(default)]
    teams: Vec<RawTeam>,
}

#[derive(Deserialize)]
struct RawCommitAuthor {
    name: String,
    date: Option<String>,
}

#[derive(Deserialize)]
struct RawCommitInner {
    message: String,
    author: Option<RawCommitAuthor>,
}

#[derive(Deserialize)]
struct RawCommit {
    sha: String,
    commit: RawCommitInner,
    author: Option<RawUser>,
}

#[derive(Deserialize)]
struct RawFile {
    filename: String,
    previous_filename: Option<String>,
    status: String,
    #[serde(default)]
    additions: u32,
    #[serde(default)]
    deletions: u32,
    patch: Option<String>,
}

#[derive(Deserialize)]
struct RawIssueComment {
    id: u64,
    user: Option<RawUser>,
    #[serde(default)]
    body: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    html_url: Option<String>,
}

impl From<RawIssueComment> for PullComment {
    fn from(c: RawIssueComment) -> Self {
        Self {
            id: format!("issue-{}", c.id),
            kind: PullCommentKind::General,
            author: c.user.map(Into::into),
            body: c.body.unwrap_or_default(),
            created_at: ts(&c.created_at),
            updated_at: ts(&c.updated_at),
            thread_id: None,
            anchor: None,
            review_state: None,
            resolved: None,
            web_url: c.html_url,
        }
    }
}

#[derive(Deserialize)]
struct RawLineComment {
    id: u64,
    user: Option<RawUser>,
    #[serde(default)]
    body: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    html_url: Option<String>,
    path: String,
    line: Option<u32>,
    original_line: Option<u32>,
    side: Option<String>,
    in_reply_to_id: Option<u64>,
    diff_hunk: Option<String>,
}

impl From<RawLineComment> for PullComment {
    fn from(c: RawLineComment) -> Self {
        Self {
            id: format!("line-{}", c.id),
            kind: PullCommentKind::Line,
            author: c.user.map(Into::into),
            body: c.body.unwrap_or_default(),
            created_at: ts(&c.created_at),
            updated_at: ts(&c.updated_at),
            // Replies always point at the thread's first comment.
            thread_id: Some(c.in_reply_to_id.unwrap_or(c.id).to_string()),
            anchor: Some(PullLineAnchor {
                path: c.path,
                line: c.line,
                side: if c.side.as_deref() == Some("LEFT") {
                    PullSide::Old
                } else {
                    PullSide::New
                },
                original_line: c.original_line,
                diff_hunk: c.diff_hunk,
            }),
            review_state: None,
            resolved: None,
            web_url: c.html_url,
        }
    }
}

#[derive(Deserialize)]
struct RawMergeResult {
    sha: Option<String>,
}
