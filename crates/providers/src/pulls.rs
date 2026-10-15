//! Pull requests (GitHub) and merge requests (GitLab) behind one set of provider-neutral types
//! (Phase 4.1, PLAN §5.8).
//!
//! The API lives in extra `impl` blocks of [`GithubClient`](crate::GithubClient) /
//! [`GitlabClient`](crate::GitlabClient) (`pulls/github.rs`, `pulls/gitlab.rs`) and is reached
//! through the `pull_*` methods of [`ProviderClient`]. A pull request is identified by its
//! project-scoped number (GitLab: the merge request `iid`).
//!
//! Every type is prefixed with `Pull` (specta exports Rust names; other provider features share
//! the crate).

mod github;
mod gitlab;

use serde::{Deserialize, Serialize};

use crate::ProviderClient;
use crate::error::ProviderError;

/// Upper bound of pages read by the "everything" helpers (commits, files, comments).
pub(crate) const MAX_PAGES: u32 = 10;

/// State of a pull request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum PullState {
    Open,
    Closed,
    Merged,
}

/// Which pull requests a listing returns.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum PullStateFilter {
    #[default]
    Open,
    Closed,
    Merged,
    All,
}

/// Filters and page of [`ProviderClient::pull_list`]. User names are provider logins.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PullListQuery {
    #[serde(default)]
    pub state: PullStateFilter,
    #[serde(default)]
    #[cfg_attr(feature = "specta", specta(optional))]
    pub author: Option<String>,
    /// Review requested from (GitHub) / reviewer (GitLab).
    #[serde(default)]
    #[cfg_attr(feature = "specta", specta(optional))]
    pub reviewer: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "specta", specta(optional))]
    pub assignee: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "specta", specta(optional))]
    pub label: Option<String>,
    /// 1-based.
    #[serde(default)]
    pub page: u32,
    /// 1..=100 (0 = 30).
    #[serde(default)]
    pub per_page: u8,
}

impl PullListQuery {
    pub(crate) fn page(&self) -> String {
        self.page.max(1).to_string()
    }

    pub(crate) fn per_page(&self) -> String {
        if self.per_page == 0 {
            "30".into()
        } else {
            self.per_page.clamp(1, 100).to_string()
        }
    }

    /// Trimmed, non-empty filter values.
    pub(crate) fn clean(v: &Option<String>) -> Option<&str> {
        v.as_deref().map(str::trim).filter(|s| !s.is_empty())
    }

    pub(crate) fn has_people_or_label_filter(&self) -> bool {
        Self::clean(&self.author).is_some()
            || Self::clean(&self.reviewer).is_some()
            || Self::clean(&self.assignee).is_some()
            || Self::clean(&self.label).is_some()
    }
}

/// A user as shown next to a pull request, comment or review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PullUser {
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub web_url: Option<String>,
}

/// One row of a pull request listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PullSummary {
    pub number: u64,
    pub title: String,
    pub state: PullState,
    pub draft: bool,
    pub author: Option<PullUser>,
    /// `None` when the listing came from GitHub search (which has no branch data).
    pub source_branch: Option<String>,
    pub target_branch: Option<String>,
    pub labels: Vec<String>,
    /// Comment count when the listing carries it.
    pub comments: Option<u32>,
    /// Unix seconds.
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub web_url: String,
}

/// One page of [`PullSummary`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PullPage {
    pub items: Vec<PullSummary>,
    pub next_page: Option<u32>,
    /// Total match count when the provider reports it.
    pub total: Option<u64>,
}

/// Can the pull request be merged as it stands?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum PullMergeState {
    /// No conflict and nothing blocking.
    Clean,
    /// Conflicts with the target branch.
    Conflicts,
    /// Blocked by branch protection / required approvals / failing required checks /
    /// unresolved discussions.
    Blocked,
    /// The source branch is behind the target and the project requires it up to date (or a
    /// rebase, GitLab fast-forward projects).
    Behind,
    /// Mergeable, but some (non-required) checks are failing.
    Unstable,
    /// Draft pull requests cannot be merged.
    Draft,
    /// The provider is still computing it — ask again in a moment.
    Checking,
    /// Already merged / closed, or not reported.
    Unknown,
}

/// A merge method of [`ProviderClient::pull_merge`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum PullMergeMethod {
    Merge,
    Squash,
    /// GitHub "Rebase and merge". GitLab rebases through the project setting instead.
    Rebase,
}

/// Full view of one pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PullDetail {
    pub number: u64,
    pub title: String,
    /// Markdown as written by the author (untrusted; the UI renders it as plain text).
    pub body: String,
    pub state: PullState,
    pub draft: bool,
    pub author: Option<PullUser>,
    pub source_branch: String,
    pub target_branch: String,
    /// Project the source branch lives in (`None`: the fork was deleted).
    pub source_repo: Option<String>,
    pub target_repo: String,
    /// The source branch lives in another project (a fork).
    pub from_fork: bool,
    pub head_sha: String,
    pub base_sha: Option<String>,
    /// GitLab `diff_refs.start_sha` (needed for new line comments).
    pub start_sha: Option<String>,
    pub merge_state: PullMergeState,
    /// Provider's own wording (`mergeable_state` / `detailed_merge_status`) for details.
    pub merge_state_raw: Option<String>,
    /// Methods the project allows (all three when the provider does not tell).
    pub merge_methods: Vec<PullMergeMethod>,
    /// The project deletes the source branch on merge by default.
    pub delete_source_default: bool,
    pub labels: Vec<String>,
    pub assignees: Vec<PullUser>,
    /// General comments (GitHub issue comments / GitLab user notes).
    pub comments: u32,
    /// Line comments (GitHub only; GitLab counts them in `comments`).
    pub review_comments: u32,
    pub commits: Option<u32>,
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
    pub changed_files: Option<u32>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub merged_at: Option<i64>,
    pub closed_at: Option<i64>,
    pub merged_by: Option<PullUser>,
    pub web_url: String,
}

/// Review state of one reviewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum PullReviewerState {
    /// Asked to review, nothing submitted yet.
    Requested,
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
}

/// One reviewer (requested or having reviewed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PullReviewer {
    pub user: PullUser,
    pub state: PullReviewerState,
}

/// Overall review outcome (computed from the latest review of each reviewer; branch protection
/// rules may still require more).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum PullReviewDecision {
    Approved,
    ChangesRequested,
    /// Reviews were requested and are pending (GitLab: approvals still missing).
    ReviewRequired,
    /// No reviewer, no review.
    None,
}

/// Reviewers of a pull request and the overall decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PullReviews {
    pub decision: PullReviewDecision,
    pub reviewers: Vec<PullReviewer>,
    /// Requested teams (GitHub), shown as names.
    pub teams: Vec<String>,
    /// GitLab: approvals still needed (`None` when unknown / GitHub).
    pub approvals_left: Option<u32>,
    /// GitLab: the token's user already approved.
    pub approved_by_me: Option<bool>,
    /// GitLab: the approvals API is missing on this server (old GitLab Free).
    pub approvals_unavailable: bool,
}

pub(crate) fn decision_of(reviewers: &[PullReviewer]) -> PullReviewDecision {
    let has = |s: PullReviewerState| reviewers.iter().any(|r| r.state == s);
    if has(PullReviewerState::ChangesRequested) {
        PullReviewDecision::ChangesRequested
    } else if has(PullReviewerState::Requested) {
        PullReviewDecision::ReviewRequired
    } else if has(PullReviewerState::Approved) {
        PullReviewDecision::Approved
    } else {
        PullReviewDecision::None
    }
}

/// A commit of a pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PullCommit {
    pub sha: String,
    /// First line of the message.
    pub title: String,
    pub message: String,
    pub author_name: String,
    /// Provider login of the author when linked.
    pub author_login: Option<String>,
    pub authored_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum PullFileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Changed,
}

/// A file changed by a pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PullFile {
    pub path: String,
    /// Previous path of a renamed / copied file.
    pub old_path: Option<String>,
    pub status: PullFileStatus,
    pub additions: u32,
    pub deletions: u32,
    /// Unified diff hunks (from the first `@@`). `None` when the provider left it out (large or
    /// binary file): the app then diffs locally after fetching the pull request ref.
    pub patch: Option<String>,
}

/// Files of a pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PullFiles {
    pub files: Vec<PullFile>,
    /// More files than the provider lists (GitHub stops at 3000) or than we read.
    pub truncated: bool,
}

/// Side of a diff a line comment is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum PullSide {
    /// The old file (deleted / context line numbers of the target).
    Old,
    /// The new file.
    New,
}

/// Where a line comment is anchored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PullLineAnchor {
    pub path: String,
    /// `None` when the line no longer exists in the current diff (outdated).
    pub line: Option<u32>,
    pub side: PullSide,
    /// The line in the diff the comment was written on (may differ when outdated).
    pub original_line: Option<u32>,
    /// GitHub `diff_hunk` (context around the line).
    pub diff_hunk: Option<String>,
}

/// What a timeline entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum PullCommentKind {
    /// Conversation comment.
    General,
    /// Comment on a line of the diff.
    Line,
    /// A submitted review (its summary body and verdict).
    Review,
    /// Provider event ("added 2 commits", "approved this merge request"…; GitLab system notes).
    System,
}

/// A comment, review or event of a pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PullComment {
    pub id: String,
    pub kind: PullCommentKind,
    pub author: Option<PullUser>,
    /// Markdown (untrusted).
    pub body: String,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    /// Comments of one line thread share this id (GitHub: root comment id; GitLab: discussion
    /// id). Pass it to [`PullCommentTarget::Reply`].
    pub thread_id: Option<String>,
    pub anchor: Option<PullLineAnchor>,
    /// Review verdict for [`PullCommentKind::Review`].
    pub review_state: Option<PullReviewerState>,
    /// GitLab resolvable discussions.
    pub resolved: Option<bool>,
    pub web_url: Option<String>,
}

/// Where [`ProviderClient::pull_comment`] posts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PullCommentTarget {
    /// Conversation comment.
    General,
    /// Answer in an existing line thread (`thread_id` of a comment).
    #[serde(rename_all = "camelCase")]
    Reply { thread_id: String },
    /// New comment on a line of the current diff.
    #[serde(rename_all = "camelCase")]
    Line {
        path: String,
        /// Previous path (renamed files; GitLab needs both).
        #[serde(default)]
        #[cfg_attr(feature = "specta", specta(optional))]
        old_path: Option<String>,
        line: u32,
        side: PullSide,
        /// Line number on the other side when the line is unchanged context (GitLab needs
        /// both for context lines; GitHub ignores it).
        #[serde(default)]
        #[cfg_attr(feature = "specta", specta(optional))]
        paired_line: Option<u32>,
    },
}

/// Review verdicts. GitLab has no "request changes" API: it is posted as a comment (the UI says
/// so); `Unapprove` is GitLab only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum PullReviewEvent {
    Approve,
    RequestChanges,
    Comment,
    Unapprove,
}

/// New pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PullCreate {
    pub source_branch: String,
    pub target_branch: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub draft: bool,
    /// Logins.
    #[serde(default)]
    pub reviewers: Vec<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    /// GitLab: delete the source branch when merged.
    #[serde(default)]
    pub remove_source_branch: bool,
}

/// Result of [`ProviderClient::pull_create`]: the pull request plus follow-up steps that failed
/// (reviewers / labels that could not be set), as short English messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PullCreated {
    pub number: u64,
    pub web_url: String,
    pub warnings: Vec<String>,
}

/// Merge options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PullMergeRequest {
    pub method: PullMergeMethod,
    #[serde(default)]
    pub delete_source_branch: bool,
    /// Only merge if the head is still this commit (refuses when someone pushed meanwhile).
    #[serde(default)]
    #[cfg_attr(feature = "specta", specta(optional))]
    pub expected_head_sha: Option<String>,
}

/// Result of a merge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PullMerged {
    pub sha: Option<String>,
    /// The source branch was deleted (requested and allowed).
    pub branch_deleted: bool,
    /// Why the requested branch deletion did not happen.
    pub delete_error: Option<String>,
}

macro_rules! dispatch {
    ($self:ident, $c:ident => $e:expr) => {
        match $self {
            ProviderClient::Github($c) => $e,
            ProviderClient::Gitlab($c) => $e,
        }
    };
}

impl ProviderClient {
    /// One page of pull requests of `full_name`.
    pub async fn pull_list(
        &self,
        full_name: &str,
        query: &PullListQuery,
    ) -> Result<PullPage, ProviderError> {
        dispatch!(self, c => c.pull_list(full_name, query).await)
    }

    pub async fn pull_detail(
        &self,
        full_name: &str,
        number: u64,
    ) -> Result<PullDetail, ProviderError> {
        dispatch!(self, c => c.pull_detail(full_name, number).await)
    }

    pub async fn pull_reviews(
        &self,
        full_name: &str,
        number: u64,
    ) -> Result<PullReviews, ProviderError> {
        dispatch!(self, c => c.pull_reviews(full_name, number).await)
    }

    pub async fn pull_commits(
        &self,
        full_name: &str,
        number: u64,
    ) -> Result<Vec<PullCommit>, ProviderError> {
        dispatch!(self, c => c.pull_commits(full_name, number).await)
    }

    pub async fn pull_files(
        &self,
        full_name: &str,
        number: u64,
    ) -> Result<PullFiles, ProviderError> {
        dispatch!(self, c => c.pull_files(full_name, number).await)
    }

    /// Conversation, reviews and line comments, oldest first.
    pub async fn pull_comments(
        &self,
        full_name: &str,
        number: u64,
    ) -> Result<Vec<PullComment>, ProviderError> {
        dispatch!(self, c => c.pull_comments(full_name, number).await)
    }

    pub async fn pull_comment(
        &self,
        full_name: &str,
        number: u64,
        target: &PullCommentTarget,
        body: &str,
    ) -> Result<PullComment, ProviderError> {
        if body.trim().is_empty() {
            return Err(ProviderError::InvalidInput {
                message: "empty comment".into(),
            });
        }
        dispatch!(self, c => c.pull_comment(full_name, number, target, body).await)
    }

    pub async fn pull_review(
        &self,
        full_name: &str,
        number: u64,
        event: PullReviewEvent,
        body: &str,
    ) -> Result<(), ProviderError> {
        dispatch!(self, c => c.pull_review(full_name, number, event, body).await)
    }

    pub async fn pull_create(
        &self,
        full_name: &str,
        create: &PullCreate,
    ) -> Result<PullCreated, ProviderError> {
        if create.title.trim().is_empty() {
            return Err(ProviderError::InvalidInput {
                message: "empty title".into(),
            });
        }
        dispatch!(self, c => c.pull_create(full_name, create).await)
    }

    pub async fn pull_merge(
        &self,
        full_name: &str,
        number: u64,
        request: &PullMergeRequest,
    ) -> Result<PullMerged, ProviderError> {
        dispatch!(self, c => c.pull_merge(full_name, number, request).await)
    }

    /// Closes (`open = false`) or reopens a pull request.
    pub async fn pull_set_open(
        &self,
        full_name: &str,
        number: u64,
        open: bool,
    ) -> Result<(), ProviderError> {
        dispatch!(self, c => c.pull_set_open(full_name, number, open).await)
    }

    /// Default branch of the project (target of new pull requests).
    pub async fn pull_default_branch(&self, full_name: &str) -> Result<String, ProviderError> {
        use crate::Provider as _;
        Ok(self
            .repository(full_name)
            .await?
            .default_branch
            .unwrap_or_else(|| "main".into()))
    }
}

/// Counts `+` / `-` lines of a unified diff (GitLab diffs carry no counts).
pub(crate) fn count_changes(patch: &str) -> (u32, u32) {
    let mut add = 0;
    let mut del = 0;
    for line in patch.lines() {
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        match line.as_bytes().first() {
            Some(b'+') => add += 1,
            Some(b'-') => del += 1,
            _ => {}
        }
    }
    (add, del)
}

/// First line of a commit message.
pub(crate) fn first_line(message: &str) -> String {
    message.lines().next().unwrap_or_default().trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rev(state: PullReviewerState) -> PullReviewer {
        PullReviewer {
            user: PullUser {
                username: "u".into(),
                display_name: None,
                avatar_url: None,
                web_url: None,
            },
            state,
        }
    }

    #[test]
    fn decision_prefers_changes_then_pending_then_approved() {
        use PullReviewerState::*;
        assert_eq!(decision_of(&[]), PullReviewDecision::None);
        assert_eq!(
            decision_of(&[rev(Approved), rev(ChangesRequested)]),
            PullReviewDecision::ChangesRequested
        );
        assert_eq!(
            decision_of(&[rev(Approved), rev(Requested)]),
            PullReviewDecision::ReviewRequired
        );
        assert_eq!(
            decision_of(&[rev(Approved), rev(Commented)]),
            PullReviewDecision::Approved
        );
        assert_eq!(decision_of(&[rev(Commented)]), PullReviewDecision::None);
    }

    #[test]
    fn counts_diff_lines() {
        let patch = "@@ -1,3 +1,3 @@\n a\n-b\n+c\n+d\n";
        assert_eq!(count_changes(patch), (2, 1));
        let with_headers = "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-x\n+y\n";
        assert_eq!(count_changes(with_headers), (1, 1));
    }

    #[test]
    fn query_defaults() {
        let q = PullListQuery::default();
        assert_eq!((q.page(), q.per_page()), ("1".into(), "30".into()));
        assert!(!q.has_people_or_label_filter());
        let q = PullListQuery {
            label: Some("  ".into()),
            per_page: 200,
            ..Default::default()
        };
        assert!(!q.has_people_or_label_filter());
        assert_eq!(q.per_page(), "100");
    }
}
