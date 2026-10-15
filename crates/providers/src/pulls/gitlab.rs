//! GitLab merge requests (REST v4). The project id is its full path (one URL-encoded segment)
//! and merge requests are addressed by `iid`.

use reqwest::Method;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;

use super::*;
use crate::gitlab::GitlabClient;
use crate::timefmt::parse_timestamp;

fn project_path(full_name: &str) -> Result<&str, ProviderError> {
    let path = full_name.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    if path.contains('/') {
        Ok(path)
    } else {
        Err(ProviderError::InvalidInput {
            message: format!("`{full_name}` is not a group/project path"),
        })
    }
}

fn ts(v: &Option<String>) -> Option<i64> {
    v.as_deref().and_then(parse_timestamp)
}

/// A missing endpoint on an older GitLab (or a feature of a higher tier).
fn is_missing(e: &ProviderError) -> bool {
    matches!(
        e,
        ProviderError::NotFound { .. } | ProviderError::Forbidden { .. }
    ) || matches!(e, ProviderError::UnexpectedStatus { status: 405, .. })
}

impl GitlabClient {
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

    async fn raw_mr(&self, full_name: &str, iid: u64) -> Result<RawMr, ProviderError> {
        let p = project_path(full_name)?;
        let n = iid.to_string();
        Ok(self
            .api()
            .get_json::<RawMr>(&["projects", p, "merge_requests", &n], &[])
            .await?
            .body)
    }

    async fn post_note(
        &self,
        full_name: &str,
        iid: u64,
        body: &str,
    ) -> Result<RawNote, ProviderError> {
        let p = project_path(full_name)?;
        let n = iid.to_string();
        Ok(self
            .api()
            .send_json::<RawNote, _>(
                Method::POST,
                &["projects", p, "merge_requests", &n, "notes"],
                &json!({ "body": body }),
            )
            .await?
            .body)
    }

    pub(crate) async fn pull_list(
        &self,
        full_name: &str,
        query: &PullListQuery,
    ) -> Result<PullPage, ProviderError> {
        let p = project_path(full_name)?;
        let page = query.page();
        let per_page = query.per_page();
        let state = match query.state {
            PullStateFilter::Open => "opened",
            PullStateFilter::Closed => "closed",
            PullStateFilter::Merged => "merged",
            PullStateFilter::All => "all",
        };
        let mut q: Vec<(&str, &str)> = vec![
            ("state", state),
            ("order_by", "updated_at"),
            ("sort", "desc"),
            ("per_page", &per_page),
            ("page", &page),
        ];
        for (key, value) in [
            ("author_username", &query.author),
            ("reviewer_username", &query.reviewer),
            ("assignee_username", &query.assignee),
            ("labels", &query.label),
        ] {
            if let Some(v) = PullListQuery::clean(value) {
                q.push((key, v.trim_start_matches('@')));
            }
        }
        let resp = self
            .api()
            .get_json::<Vec<RawMr>>(&["projects", p, "merge_requests"], &q)
            .await?;
        let next_page = resp.next_page();
        let total = resp.header("x-total").and_then(|t| t.parse().ok());
        Ok(PullPage {
            items: resp.body.into_iter().map(RawMr::into_summary).collect(),
            next_page,
            total,
        })
    }

    pub(crate) async fn pull_detail(
        &self,
        full_name: &str,
        iid: u64,
    ) -> Result<PullDetail, ProviderError> {
        let p = project_path(full_name)?;
        let project_segs = ["projects", p];
        let (mr, project) = tokio::join!(
            self.raw_mr(full_name, iid),
            self.api().get_json::<RawProject>(&project_segs, &[])
        );
        let mr = mr?;
        let project = project?.body;
        let source_repo = if mr.source_project_id == mr.target_project_id {
            Some(project.path_with_namespace.clone())
        } else {
            // A private or deleted fork: unknown.
            let id = mr.source_project_id.to_string();
            self.api()
                .get_json::<RawProject>(&["projects", &id], &[])
                .await
                .ok()
                .map(|r| r.body.path_with_namespace)
        };
        let methods = match project.squash_option.as_deref() {
            Some("never") => vec![PullMergeMethod::Merge],
            Some("always") => vec![PullMergeMethod::Squash],
            _ => vec![PullMergeMethod::Merge, PullMergeMethod::Squash],
        };
        let delete_default = mr
            .force_remove_source_branch
            .or(project.remove_source_branch_after_merge)
            .unwrap_or(false);
        Ok(mr.into_detail(
            project.path_with_namespace,
            source_repo,
            methods,
            delete_default,
        ))
    }

    pub(crate) async fn pull_reviews(
        &self,
        full_name: &str,
        iid: u64,
    ) -> Result<PullReviews, ProviderError> {
        let p = project_path(full_name)?;
        let n = iid.to_string();
        let approvals_segs = ["projects", p, "merge_requests", &n, "approvals"];
        let reviewers_segs = ["projects", p, "merge_requests", &n, "reviewers"];
        let (mr, approvals, states) = tokio::join!(
            self.raw_mr(full_name, iid),
            self.api().get_json::<RawApprovals>(&approvals_segs, &[]),
            self.api()
                .get_json::<Vec<RawReviewerState>>(&reviewers_segs, &[])
        );
        let mr = mr?;
        let (approvals, unavailable) = match approvals {
            Ok(a) => (Some(a.body), false),
            Err(e) if is_missing(&e) => (None, true),
            Err(e) => return Err(e),
        };
        // Per-reviewer states (GitLab 15.x+); older servers only list the reviewers.
        let states: Vec<RawReviewerState> = states.map(|r| r.body).unwrap_or_default();
        let approved: Vec<String> = approvals
            .as_ref()
            .map(|a| {
                a.approved_by
                    .iter()
                    .map(|x| x.user.username.to_ascii_lowercase())
                    .collect()
            })
            .unwrap_or_default();
        let mut reviewers: Vec<PullReviewer> = Vec::new();
        for user in mr.reviewers {
            let key = user.username.to_ascii_lowercase();
            let reported = states
                .iter()
                .find(|s| s.user.username.eq_ignore_ascii_case(&key))
                .map(|s| s.state.as_str());
            let state = if approved.contains(&key) {
                PullReviewerState::Approved
            } else {
                match reported {
                    Some("requested_changes") => PullReviewerState::ChangesRequested,
                    Some("reviewed") => PullReviewerState::Commented,
                    Some("approved") => PullReviewerState::Approved,
                    _ => PullReviewerState::Requested,
                }
            };
            reviewers.push(PullReviewer {
                user: user.into(),
                state,
            });
        }
        if let Some(a) = &approvals {
            for x in &a.approved_by {
                let known = reviewers
                    .iter()
                    .any(|r| r.user.username.eq_ignore_ascii_case(&x.user.username));
                if !known {
                    reviewers.push(PullReviewer {
                        user: x.user.clone().into(),
                        state: PullReviewerState::Approved,
                    });
                }
            }
        }
        let approvals_left = approvals.as_ref().and_then(|a| a.approvals_left);
        let mut decision = decision_of(&reviewers);
        if decision != PullReviewDecision::ChangesRequested {
            match approvals_left {
                Some(left) if left > 0 => decision = PullReviewDecision::ReviewRequired,
                Some(_) if !approved.is_empty() => decision = PullReviewDecision::Approved,
                _ => {}
            }
        }
        Ok(PullReviews {
            decision,
            reviewers,
            teams: Vec::new(),
            approvals_left,
            approved_by_me: approvals.as_ref().and_then(|a| a.user_has_approved),
            approvals_unavailable: unavailable,
        })
    }

    pub(crate) async fn pull_commits(
        &self,
        full_name: &str,
        iid: u64,
    ) -> Result<Vec<PullCommit>, ProviderError> {
        let p = project_path(full_name)?;
        let n = iid.to_string();
        let (commits, _) = self
            .get_all::<RawCommit>(&["projects", p, "merge_requests", &n, "commits"], MAX_PAGES)
            .await?;
        // GitLab lists newest first; GitHub (and the UI) oldest first.
        Ok(commits
            .into_iter()
            .rev()
            .map(|c| PullCommit {
                title: if c.title.is_empty() {
                    first_line(&c.message)
                } else {
                    c.title
                },
                message: c.message,
                author_name: c.author_name,
                author_login: None,
                authored_at: ts(&c.authored_date),
                sha: c.id,
            })
            .collect())
    }

    pub(crate) async fn pull_files(
        &self,
        full_name: &str,
        iid: u64,
    ) -> Result<PullFiles, ProviderError> {
        let p = project_path(full_name)?;
        let n = iid.to_string();
        let (diffs, truncated) = match self
            .get_all::<RawDiff>(&["projects", p, "merge_requests", &n, "diffs"], MAX_PAGES)
            .await
        {
            Ok(d) => d,
            // `/diffs` is GitLab 15.7+; `/changes` is the older (deprecated) form.
            Err(e) if is_missing(&e) => {
                let changes = self
                    .api()
                    .get_json::<RawChanges>(&["projects", p, "merge_requests", &n, "changes"], &[])
                    .await?
                    .body;
                (changes.changes, changes.overflow)
            }
            Err(e) => return Err(e),
        };
        Ok(PullFiles {
            files: diffs.into_iter().map(RawDiff::into_file).collect(),
            truncated,
        })
    }

    pub(crate) async fn pull_comments(
        &self,
        full_name: &str,
        iid: u64,
    ) -> Result<Vec<PullComment>, ProviderError> {
        let p = project_path(full_name)?;
        let n = iid.to_string();
        let (discussions, _) = self
            .get_all::<RawDiscussion>(
                &["projects", p, "merge_requests", &n, "discussions"],
                MAX_PAGES,
            )
            .await?;
        let mut out = Vec::new();
        for d in discussions {
            let threaded = !d.individual_note;
            for note in d.notes {
                let thread = (threaded || note.position.is_some()).then(|| d.id.clone());
                out.push(note.into_comment(thread));
            }
        }
        out.sort_by_key(|c| c.created_at.unwrap_or(i64::MAX));
        Ok(out)
    }

    pub(crate) async fn pull_comment(
        &self,
        full_name: &str,
        iid: u64,
        target: &PullCommentTarget,
        body: &str,
    ) -> Result<PullComment, ProviderError> {
        let p = project_path(full_name)?;
        let n = iid.to_string();
        match target {
            PullCommentTarget::General => Ok(self
                .post_note(full_name, iid, body)
                .await?
                .into_comment(None)),
            PullCommentTarget::Reply { thread_id } => {
                let id = thread_id.trim();
                if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
                    return Err(ProviderError::InvalidInput {
                        message: format!("invalid discussion id `{thread_id}`"),
                    });
                }
                let note = self
                    .api()
                    .send_json::<RawNote, _>(
                        Method::POST,
                        &[
                            "projects",
                            p,
                            "merge_requests",
                            &n,
                            "discussions",
                            id,
                            "notes",
                        ],
                        &json!({ "body": body }),
                    )
                    .await?
                    .body;
                Ok(note.into_comment(Some(id.to_owned())))
            }
            PullCommentTarget::Line {
                path,
                old_path,
                line,
                side,
                paired_line,
            } => {
                let mr = self.raw_mr(full_name, iid).await?;
                let refs = mr.diff_refs.ok_or_else(|| ProviderError::Unsupported {
                    message: "this merge request has no diff yet".into(),
                })?;
                let mut position = json!({
                    "position_type": "text",
                    "base_sha": refs.base_sha,
                    "start_sha": refs.start_sha,
                    "head_sha": refs.head_sha,
                    "new_path": path,
                    "old_path": old_path.as_deref().unwrap_or(path),
                });
                let (this, other) = match side {
                    PullSide::New => ("new_line", "old_line"),
                    PullSide::Old => ("old_line", "new_line"),
                };
                position[this] = json!(line);
                if let Some(paired) = paired_line {
                    position[other] = json!(paired);
                }
                let d = self
                    .api()
                    .send_json::<RawDiscussion, _>(
                        Method::POST,
                        &["projects", p, "merge_requests", &n, "discussions"],
                        &json!({ "body": body, "position": position }),
                    )
                    .await?
                    .body;
                let id = d.id.clone();
                d.notes
                    .into_iter()
                    .next()
                    .map(|note| note.into_comment(Some(id)))
                    .ok_or_else(|| ProviderError::InvalidResponse {
                        message: "discussion without notes".into(),
                    })
            }
        }
    }

    pub(crate) async fn pull_review(
        &self,
        full_name: &str,
        iid: u64,
        event: PullReviewEvent,
        body: &str,
    ) -> Result<(), ProviderError> {
        let p = project_path(full_name)?;
        let n = iid.to_string();
        let action = match event {
            PullReviewEvent::Approve => Some("approve"),
            PullReviewEvent::Unapprove => Some("unapprove"),
            PullReviewEvent::RequestChanges | PullReviewEvent::Comment => None,
        };
        if let Some(action) = action {
            self.api()
                .send_json::<serde_json::Value, _>(
                    Method::POST,
                    &["projects", p, "merge_requests", &n, action],
                    &json!({}),
                )
                .await
                .map_err(|e| match e {
                    ProviderError::NotFound { .. } => ProviderError::Unsupported {
                        message: "approvals are not available on this GitLab".into(),
                    },
                    e => e,
                })?;
        } else if body.trim().is_empty() {
            return Err(ProviderError::InvalidInput {
                message: "a review comment needs a text".into(),
            });
        }
        if !body.trim().is_empty() {
            self.post_note(full_name, iid, body).await?;
        }
        Ok(())
    }

    async fn user_id(&self, username: &str) -> Result<Option<u64>, ProviderError> {
        let users = self
            .api()
            .get_json::<Vec<RawUserId>>(&["users"], &[("username", username)])
            .await?
            .body;
        Ok(users.first().map(|u| u.id))
    }

    pub(crate) async fn pull_create(
        &self,
        full_name: &str,
        create: &PullCreate,
    ) -> Result<PullCreated, ProviderError> {
        let p = project_path(full_name)?;
        let mut warnings = Vec::new();
        let mut reviewer_ids = Vec::new();
        for name in create
            .reviewers
            .iter()
            .map(|s| s.trim().trim_start_matches('@'))
            .filter(|s| !s.is_empty())
        {
            match self.user_id(name).await {
                Ok(Some(id)) => reviewer_ids.push(id),
                Ok(None) => warnings.push(format!("reviewer `{name}` not found")),
                Err(e) => warnings.push(format!("reviewer `{name}`: {e}")),
            }
        }
        let title = create.title.trim();
        let title = if create.draft && !title.to_ascii_lowercase().starts_with("draft:") {
            format!("Draft: {title}")
        } else {
            title.to_owned()
        };
        let labels: Vec<&str> = create
            .labels
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        let mut payload = json!({
            "source_branch": create.source_branch,
            "target_branch": create.target_branch,
            "title": title,
            "description": create.body,
            "remove_source_branch": create.remove_source_branch,
        });
        if !labels.is_empty() {
            payload["labels"] = json!(labels.join(","));
        }
        if !reviewer_ids.is_empty() {
            payload["reviewer_ids"] = json!(reviewer_ids);
        }
        let mr = self
            .api()
            .send_json::<RawMr, _>(Method::POST, &["projects", p, "merge_requests"], &payload)
            .await?
            .body;
        Ok(PullCreated {
            number: mr.iid,
            web_url: mr.web_url,
            warnings,
        })
    }

    pub(crate) async fn pull_merge(
        &self,
        full_name: &str,
        iid: u64,
        request: &PullMergeRequest,
    ) -> Result<PullMerged, ProviderError> {
        let p = project_path(full_name)?;
        let n = iid.to_string();
        if request.method == PullMergeMethod::Rebase {
            return Err(ProviderError::Unsupported {
                message: "GitLab rebases through the project's merge method setting".into(),
            });
        }
        let mut payload = json!({
            "squash": request.method == PullMergeMethod::Squash,
            "should_remove_source_branch": request.delete_source_branch,
        });
        if let Some(sha) = request
            .expected_head_sha
            .as_deref()
            .filter(|s| !s.is_empty())
        {
            payload["sha"] = json!(sha);
        }
        let mr = self
            .api()
            .send_json::<RawMr, _>(
                Method::PUT,
                &["projects", p, "merge_requests", &n, "merge"],
                &payload,
            )
            .await?
            .body;
        Ok(PullMerged {
            sha: mr.merge_commit_sha.or(mr.squash_commit_sha),
            // GitLab deletes the branch itself after the merge.
            branch_deleted: request.delete_source_branch,
            delete_error: None,
        })
    }

    pub(crate) async fn pull_set_open(
        &self,
        full_name: &str,
        iid: u64,
        open: bool,
    ) -> Result<(), ProviderError> {
        let p = project_path(full_name)?;
        let n = iid.to_string();
        self.api()
            .send_json::<serde_json::Value, _>(
                Method::PUT,
                &["projects", p, "merge_requests", &n],
                &json!({ "state_event": if open { "reopen" } else { "close" } }),
            )
            .await?;
        Ok(())
    }
}

#[derive(Deserialize, Clone)]
struct RawUser {
    username: String,
    name: Option<String>,
    avatar_url: Option<String>,
    web_url: Option<String>,
}

impl From<RawUser> for PullUser {
    fn from(u: RawUser) -> Self {
        Self {
            username: u.username,
            display_name: u.name.filter(|n| !n.trim().is_empty()),
            avatar_url: u.avatar_url,
            web_url: u.web_url,
        }
    }
}

#[derive(Deserialize)]
struct RawUserId {
    id: u64,
}

#[derive(Deserialize)]
struct RawDiffRefs {
    base_sha: Option<String>,
    head_sha: Option<String>,
    start_sha: Option<String>,
}

#[derive(Deserialize)]
struct RawMr {
    iid: u64,
    title: String,
    description: Option<String>,
    state: String,
    draft: Option<bool>,
    work_in_progress: Option<bool>,
    author: Option<RawUser>,
    #[serde(default)]
    source_branch: String,
    #[serde(default)]
    target_branch: String,
    #[serde(default)]
    source_project_id: u64,
    #[serde(default)]
    target_project_id: u64,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    assignees: Vec<RawUser>,
    #[serde(default)]
    reviewers: Vec<RawUser>,
    user_notes_count: Option<u32>,
    sha: Option<String>,
    merge_commit_sha: Option<String>,
    squash_commit_sha: Option<String>,
    diff_refs: Option<RawDiffRefs>,
    merge_status: Option<String>,
    detailed_merge_status: Option<String>,
    has_conflicts: Option<bool>,
    changes_count: Option<String>,
    force_remove_source_branch: Option<bool>,
    merged_by: Option<RawUser>,
    merge_user: Option<RawUser>,
    created_at: Option<String>,
    updated_at: Option<String>,
    merged_at: Option<String>,
    closed_at: Option<String>,
    #[serde(default)]
    web_url: String,
}

impl RawMr {
    fn state(&self) -> PullState {
        match self.state.as_str() {
            "opened" | "locked" => PullState::Open,
            "merged" => PullState::Merged,
            _ => PullState::Closed,
        }
    }

    fn is_draft(&self) -> bool {
        self.draft.or(self.work_in_progress).unwrap_or(false)
    }

    fn into_summary(self) -> PullSummary {
        PullSummary {
            state: self.state(),
            draft: self.is_draft(),
            number: self.iid,
            title: self.title,
            author: self.author.map(Into::into),
            source_branch: Some(self.source_branch),
            target_branch: Some(self.target_branch),
            labels: self.labels,
            comments: self.user_notes_count,
            created_at: ts(&self.created_at),
            updated_at: ts(&self.updated_at),
            web_url: self.web_url,
        }
    }

    fn merge_state(&self) -> PullMergeState {
        if self.state() != PullState::Open {
            return PullMergeState::Unknown;
        }
        if let Some(detailed) = self.detailed_merge_status.as_deref() {
            return match detailed {
                "mergeable" => PullMergeState::Clean,
                "conflict" | "broken_status" => PullMergeState::Conflicts,
                "need_rebase" => PullMergeState::Behind,
                "draft_status" => PullMergeState::Draft,
                "checking" | "unchecked" | "approvals_syncing" | "preparing" => {
                    PullMergeState::Checking
                }
                _ => PullMergeState::Blocked,
            };
        }
        if self.has_conflicts == Some(true) {
            return PullMergeState::Conflicts;
        }
        if self.is_draft() {
            return PullMergeState::Draft;
        }
        match self.merge_status.as_deref() {
            Some("can_be_merged") => PullMergeState::Clean,
            Some("cannot_be_merged") | Some("cannot_be_merged_recheck") => {
                PullMergeState::Conflicts
            }
            Some("unchecked") | Some("checking") => PullMergeState::Checking,
            _ => PullMergeState::Unknown,
        }
    }

    fn into_detail(
        self,
        target_repo: String,
        source_repo: Option<String>,
        merge_methods: Vec<PullMergeMethod>,
        delete_source_default: bool,
    ) -> PullDetail {
        let state = self.state();
        let merge_state = self.merge_state();
        let draft = self.is_draft();
        let changed_files = self.changes_count.as_deref().and_then(|c| {
            let digits: String = c.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        });
        let (base_sha, start_sha, refs_head) = match self.diff_refs {
            Some(r) => (r.base_sha, r.start_sha, r.head_sha),
            None => (None, None, None),
        };
        PullDetail {
            number: self.iid,
            title: self.title,
            body: self.description.unwrap_or_default(),
            state,
            draft,
            author: self.author.map(Into::into),
            source_branch: self.source_branch,
            target_branch: self.target_branch,
            from_fork: self.source_project_id != self.target_project_id,
            source_repo,
            target_repo,
            head_sha: self.sha.or(refs_head).unwrap_or_default(),
            base_sha,
            start_sha,
            merge_state,
            merge_state_raw: self.detailed_merge_status.or(self.merge_status),
            merge_methods,
            delete_source_default,
            labels: self.labels,
            assignees: self.assignees.into_iter().map(Into::into).collect(),
            comments: self.user_notes_count.unwrap_or(0),
            review_comments: 0,
            commits: None,
            additions: None,
            deletions: None,
            changed_files,
            created_at: ts(&self.created_at),
            updated_at: ts(&self.updated_at),
            merged_at: ts(&self.merged_at),
            closed_at: ts(&self.closed_at),
            merged_by: self.merge_user.or(self.merged_by).map(Into::into),
            web_url: self.web_url,
        }
    }
}

#[derive(Deserialize)]
struct RawProject {
    path_with_namespace: String,
    squash_option: Option<String>,
    remove_source_branch_after_merge: Option<bool>,
}

#[derive(Deserialize)]
struct RawApprover {
    user: RawUser,
}

#[derive(Deserialize)]
struct RawApprovals {
    approvals_left: Option<u32>,
    #[serde(default)]
    approved_by: Vec<RawApprover>,
    user_has_approved: Option<bool>,
}

#[derive(Deserialize)]
struct RawReviewerState {
    user: RawUser,
    #[serde(default)]
    state: String,
}

#[derive(Deserialize)]
struct RawCommit {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    author_name: String,
    authored_date: Option<String>,
}

#[derive(Deserialize)]
struct RawDiff {
    old_path: String,
    new_path: String,
    #[serde(default)]
    new_file: bool,
    #[serde(default)]
    renamed_file: bool,
    #[serde(default)]
    deleted_file: bool,
    #[serde(default)]
    diff: String,
}

impl RawDiff {
    fn into_file(self) -> PullFile {
        let (additions, deletions) = count_changes(&self.diff);
        let status = if self.new_file {
            PullFileStatus::Added
        } else if self.deleted_file {
            PullFileStatus::Deleted
        } else if self.renamed_file {
            PullFileStatus::Renamed
        } else {
            PullFileStatus::Modified
        };
        PullFile {
            old_path: (self.renamed_file && self.old_path != self.new_path)
                .then_some(self.old_path),
            path: self.new_path,
            status,
            additions,
            deletions,
            patch: (!self.diff.is_empty()).then_some(self.diff),
        }
    }
}

#[derive(Deserialize)]
struct RawChanges {
    #[serde(default)]
    changes: Vec<RawDiff>,
    #[serde(default)]
    overflow: bool,
}

#[derive(Deserialize)]
struct RawPosition {
    new_path: Option<String>,
    old_path: Option<String>,
    new_line: Option<u32>,
    old_line: Option<u32>,
}

#[derive(Deserialize)]
struct RawNote {
    id: u64,
    #[serde(default)]
    body: String,
    author: Option<RawUser>,
    created_at: Option<String>,
    updated_at: Option<String>,
    #[serde(default)]
    system: bool,
    #[serde(default)]
    resolvable: bool,
    resolved: Option<bool>,
    position: Option<RawPosition>,
}

impl RawNote {
    fn into_comment(self, thread_id: Option<String>) -> PullComment {
        let anchor = self.position.map(|p| {
            let side = if p.new_line.is_some() {
                PullSide::New
            } else {
                PullSide::Old
            };
            let line = p.new_line.or(p.old_line);
            PullLineAnchor {
                path: p.new_path.or(p.old_path).unwrap_or_default(),
                line,
                side,
                original_line: line,
                diff_hunk: None,
            }
        });
        let kind = if self.system {
            PullCommentKind::System
        } else if anchor.is_some() {
            PullCommentKind::Line
        } else {
            PullCommentKind::General
        };
        PullComment {
            id: format!("note-{}", self.id),
            kind,
            author: self.author.map(Into::into),
            body: self.body,
            created_at: ts(&self.created_at),
            updated_at: ts(&self.updated_at),
            thread_id,
            anchor,
            review_state: None,
            resolved: self.resolvable.then_some(self.resolved.unwrap_or(false)),
            web_url: None,
        }
    }
}

#[derive(Deserialize)]
struct RawDiscussion {
    id: String,
    #[serde(default)]
    individual_note: bool,
    #[serde(default)]
    notes: Vec<RawNote>,
}
