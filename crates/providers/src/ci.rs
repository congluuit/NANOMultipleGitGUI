//! CI status of a commit (PLAN §5.6, Phase 4.3): GitHub check runs + commit statuses, GitLab
//! pipelines, folded into one overall [`CiState`] plus the list of jobs.
//!
//! - GitHub: `GET /repos/{o}/{r}/commits/{sha}/check-runs` (latest run of each check; needs
//!   `checks:read` for fine-grained / GitHub App tokens) and `GET …/commits/{sha}/status`
//!   (combined legacy statuses). When only the check runs are refused the statuses are still
//!   used and [`CiStatus::checks_error`] says why the list may be incomplete.
//! - GitLab: the newest pipeline of the sha (`/projects/:id/pipelines?sha=`) and, when asked,
//!   its jobs (latest attempt of each).
//!
//! A commit the provider does not know (not pushed yet) is [`CiState::None`], not an error.

use serde::{Deserialize, Serialize};

use crate::ProviderClient;
use crate::error::ProviderError;
use crate::github::GithubClient;
use crate::gitlab::GitlabClient;
use crate::timefmt::parse_timestamp;

/// Check runs read per commit (two pages of 100); more are reported as `truncated`.
const MAX_CHECK_PAGES: u32 = 2;

/// Overall CI result of a commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum CiState {
    Success,
    /// At least one required job failed, errored, timed out or was cancelled.
    Failure,
    /// Something is queued or running.
    Pending,
    /// Finished without a verdict (skipped / neutral / manual-only).
    Neutral,
    /// No CI ran for this commit (or the provider does not know the commit).
    None,
}

impl CiState {
    /// Final states do not change on their own (a re-run is a new event).
    pub fn is_final(self) -> bool {
        matches!(self, Self::Success | Self::Failure | Self::Neutral)
    }
}

/// State of one job / check / status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum CiJobState {
    Queued,
    Running,
    Success,
    Failure,
    Cancelled,
    Skipped,
    Neutral,
    /// GitLab manual job / GitHub `action_required`: waits for someone to act.
    Manual,
}

/// Where a job comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum CiJobKind {
    /// GitHub check run (GitHub Actions and other apps).
    Check,
    /// GitHub commit status (external CI posting statuses).
    Status,
    /// GitLab pipeline job.
    Job,
}

/// One job of the commit's CI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CiJob {
    pub name: String,
    pub kind: CiJobKind,
    pub state: CiJobState,
    /// GitLab stage, GitHub app name (`GitHub Actions`).
    pub group: Option<String>,
    /// Status text of the provider (status description, raw conclusion), when useful.
    pub description: Option<String>,
    /// Unix seconds.
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    /// Page with the log / details (https).
    pub url: Option<String>,
    /// GitLab `allow_failure`: a failure does not fail the pipeline.
    pub allow_failure: bool,
}

/// CI of one commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CiStatus {
    pub sha: String,
    pub state: CiState,
    /// Empty when jobs were not requested (GitLab) or none ran.
    pub jobs: Vec<CiJob>,
    /// GitLab pipeline page; GitHub has no single page for all checks.
    pub pipeline_url: Option<String>,
    /// More check runs than were read.
    pub truncated: bool,
    /// GitHub: the check runs could not be read (typically a fine-grained token without
    /// `checks:read`); the state then only reflects commit statuses.
    pub checks_error: Option<ProviderError>,
}

impl CiStatus {
    fn none(sha: &str) -> Self {
        Self {
            sha: sha.to_owned(),
            state: CiState::None,
            jobs: Vec::new(),
            pipeline_url: None,
            truncated: false,
            checks_error: None,
        }
    }
}

/// Overall state of a set of jobs: failure > pending > success > neutral > none. Jobs allowed
/// to fail never make the result a failure.
pub fn fold_state(jobs: &[CiJob]) -> CiState {
    let mut state = CiState::None;
    let rank = |s: CiState| match s {
        CiState::None => 0,
        CiState::Neutral => 1,
        CiState::Success => 2,
        CiState::Pending => 3,
        CiState::Failure => 4,
    };
    for job in jobs {
        let s = match job.state {
            CiJobState::Failure | CiJobState::Cancelled if job.allow_failure => CiState::Neutral,
            CiJobState::Failure | CiJobState::Cancelled => CiState::Failure,
            CiJobState::Queued | CiJobState::Running => CiState::Pending,
            CiJobState::Success => CiState::Success,
            CiJobState::Skipped | CiJobState::Neutral | CiJobState::Manual => CiState::Neutral,
        };
        if rank(s) > rank(state) {
            state = s;
        }
    }
    state
}

/// The provider answered "this commit does not exist here" (not pushed yet).
fn unknown_commit(e: &ProviderError) -> bool {
    match e {
        ProviderError::Rejected { status: 422, .. } => true,
        ProviderError::NotFound { message, hint } => {
            // "No commit found for SHA" (GitHub) / "404 Commit Not Found"; a hidden repository
            // answers a plain "Not Found".
            hint.is_none() && message.to_ascii_lowercase().contains("commit")
        }
        _ => false,
    }
}

impl ProviderClient {
    /// CI of `sha` in `full_name`. GitLab only reads the jobs when `with_jobs` is set (one
    /// request less per commit, enough for a badge); GitHub always lists them.
    pub async fn ci_status(
        &self,
        full_name: &str,
        sha: &str,
        with_jobs: bool,
    ) -> Result<CiStatus, ProviderError> {
        let sha = sha.trim();
        if sha.len() < 7 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ProviderError::InvalidInput {
                message: format!("`{sha}` is not a commit id"),
            });
        }
        match self {
            Self::Github(c) => c.ci_status(full_name, sha).await,
            Self::Gitlab(c) => c.ci_status(full_name, sha, with_jobs).await,
        }
    }
}

// ----- GitHub --------------------------------------------------------------------------------

#[derive(Deserialize)]
struct RawCheckRuns {
    #[serde(default)]
    total_count: u64,
    #[serde(default)]
    check_runs: Vec<RawCheckRun>,
}

#[derive(Deserialize)]
struct RawCheckApp {
    name: Option<String>,
}

#[derive(Deserialize)]
struct RawCheckRun {
    name: String,
    status: String,
    conclusion: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
    html_url: Option<String>,
    details_url: Option<String>,
    app: Option<RawCheckApp>,
}

#[derive(Deserialize)]
struct RawCombined {
    #[serde(default)]
    statuses: Vec<RawStatus>,
}

#[derive(Deserialize)]
struct RawStatus {
    context: Option<String>,
    state: String,
    description: Option<String>,
    target_url: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

fn https_only(url: Option<String>) -> Option<String> {
    url.filter(|u| u.starts_with("https://") || u.starts_with("http://"))
}

fn check_state(status: &str, conclusion: Option<&str>) -> CiJobState {
    match status {
        "completed" => match conclusion.unwrap_or("") {
            "success" => CiJobState::Success,
            "failure" | "timed_out" | "startup_failure" | "stale" => CiJobState::Failure,
            "cancelled" => CiJobState::Cancelled,
            "skipped" => CiJobState::Skipped,
            "action_required" => CiJobState::Manual,
            _ => CiJobState::Neutral,
        },
        "in_progress" => CiJobState::Running,
        // queued, requested, waiting, pending
        _ => CiJobState::Queued,
    }
}

impl From<RawCheckRun> for CiJob {
    fn from(r: RawCheckRun) -> Self {
        let state = check_state(&r.status, r.conclusion.as_deref());
        Self {
            name: r.name,
            kind: CiJobKind::Check,
            state,
            group: r.app.and_then(|a| a.name),
            description: r.conclusion.filter(|c| c == "timed_out" || c == "stale"),
            started_at: r.started_at.as_deref().and_then(parse_timestamp),
            finished_at: r.completed_at.as_deref().and_then(parse_timestamp),
            url: https_only(r.html_url.or(r.details_url)),
            allow_failure: false,
        }
    }
}

impl From<RawStatus> for CiJob {
    fn from(s: RawStatus) -> Self {
        let state = match s.state.as_str() {
            "success" => CiJobState::Success,
            "failure" | "error" => CiJobState::Failure,
            _ => CiJobState::Queued,
        };
        let created = s.created_at.as_deref().and_then(parse_timestamp);
        let updated = s.updated_at.as_deref().and_then(parse_timestamp);
        Self {
            name: s.context.unwrap_or_else(|| "status".into()),
            kind: CiJobKind::Status,
            state,
            group: None,
            description: s.description.filter(|d| !d.trim().is_empty()),
            started_at: created,
            finished_at: if state == CiJobState::Queued {
                None
            } else {
                updated
            },
            url: https_only(s.target_url),
            allow_failure: false,
        }
    }
}

impl GithubClient {
    fn ci_segments(full_name: &str) -> Result<(String, String), ProviderError> {
        match full_name.trim_matches('/').split_once('/') {
            Some((o, r)) if !o.is_empty() && !r.is_empty() && !r.contains('/') => {
                Ok((o.to_owned(), r.strip_suffix(".git").unwrap_or(r).to_owned()))
            }
            _ => Err(ProviderError::InvalidInput {
                message: format!("`{full_name}` is not an owner/repo name"),
            }),
        }
    }

    async fn check_runs(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
    ) -> Result<(Vec<CiJob>, bool), ProviderError> {
        let mut jobs = Vec::new();
        let mut page = 1u32;
        loop {
            let page_s = page.to_string();
            let resp = self
                .api()
                .get_json::<RawCheckRuns>(
                    &["repos", owner, repo, "commits", sha, "check-runs"],
                    &[("per_page", "100"), ("page", &page_s)],
                )
                .await?;
            let total = resp.body.total_count;
            let next = resp.next_page();
            jobs.extend(resp.body.check_runs.into_iter().map(CiJob::from));
            match next {
                Some(n) if n > page && page < MAX_CHECK_PAGES => page = n,
                Some(_) => return Ok((jobs, true)),
                None => {
                    let truncated = (jobs.len() as u64) < total;
                    return Ok((jobs, truncated));
                }
            }
        }
    }

    async fn combined_status(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
    ) -> Result<Vec<CiJob>, ProviderError> {
        let resp = self
            .api()
            .get_json::<RawCombined>(
                &["repos", owner, repo, "commits", sha, "status"],
                &[("per_page", "100")],
            )
            .await?;
        // The combined `state` is "pending" when there are no statuses at all: fold ourselves.
        Ok(resp.body.statuses.into_iter().map(CiJob::from).collect())
    }

    pub async fn ci_status(&self, full_name: &str, sha: &str) -> Result<CiStatus, ProviderError> {
        let (owner, repo) = Self::ci_segments(full_name)?;
        let (checks, statuses) = tokio::join!(
            self.check_runs(&owner, &repo, sha),
            self.combined_status(&owner, &repo, sha)
        );
        let statuses = match statuses {
            Ok(s) => s,
            Err(e) if unknown_commit(&e) => return Ok(CiStatus::none(sha)),
            Err(e) => return Err(e),
        };
        let mut out = CiStatus::none(sha);
        match checks {
            Ok((jobs, truncated)) => {
                out.jobs = jobs;
                out.truncated = truncated;
            }
            Err(e) if unknown_commit(&e) => return Ok(CiStatus::none(sha)),
            Err(e @ (ProviderError::Forbidden { .. } | ProviderError::NotFound { .. })) => {
                out.checks_error = Some(e);
            }
            Err(e) => return Err(e),
        }
        out.jobs.extend(statuses);
        out.state = fold_state(&out.jobs);
        Ok(out)
    }
}

// ----- GitLab --------------------------------------------------------------------------------

#[derive(Deserialize)]
struct RawPipeline {
    id: u64,
    status: String,
    web_url: Option<String>,
}

#[derive(Deserialize)]
struct RawJob {
    name: String,
    stage: Option<String>,
    status: String,
    started_at: Option<String>,
    finished_at: Option<String>,
    web_url: Option<String>,
    #[serde(default)]
    allow_failure: bool,
}

fn gitlab_job_state(status: &str) -> CiJobState {
    match status {
        "success" => CiJobState::Success,
        "failed" => CiJobState::Failure,
        "canceled" | "canceling" => CiJobState::Cancelled,
        "skipped" => CiJobState::Skipped,
        "manual" => CiJobState::Manual,
        "running" => CiJobState::Running,
        // created, pending, preparing, waiting_for_resource, waiting_for_callback, scheduled
        _ => CiJobState::Queued,
    }
}

/// Pipeline status → overall state (the pipeline already accounts for `allow_failure`).
fn gitlab_pipeline_state(status: &str) -> CiState {
    match status {
        "success" => CiState::Success,
        "failed" | "canceled" | "canceling" => CiState::Failure,
        "skipped" | "manual" => CiState::Neutral,
        _ => CiState::Pending,
    }
}

impl From<RawJob> for CiJob {
    fn from(j: RawJob) -> Self {
        Self {
            state: gitlab_job_state(&j.status),
            name: j.name,
            kind: CiJobKind::Job,
            group: j.stage,
            description: None,
            started_at: j.started_at.as_deref().and_then(parse_timestamp),
            finished_at: j.finished_at.as_deref().and_then(parse_timestamp),
            url: https_only(j.web_url),
            allow_failure: j.allow_failure,
        }
    }
}

impl GitlabClient {
    pub async fn ci_status(
        &self,
        full_name: &str,
        sha: &str,
        with_jobs: bool,
    ) -> Result<CiStatus, ProviderError> {
        let path = full_name.trim_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);
        if !path.contains('/') {
            return Err(ProviderError::InvalidInput {
                message: format!("`{full_name}` is not a group/project path"),
            });
        }
        let pipelines = self
            .api()
            .get_json::<Vec<RawPipeline>>(
                &["projects", path, "pipelines"],
                &[
                    ("sha", sha),
                    ("order_by", "id"),
                    ("sort", "desc"),
                    ("per_page", "1"),
                ],
            )
            .await?
            .body;
        let Some(pipeline) = pipelines.into_iter().next() else {
            return Ok(CiStatus::none(sha));
        };
        let mut out = CiStatus::none(sha);
        out.state = gitlab_pipeline_state(&pipeline.status);
        out.pipeline_url = https_only(pipeline.web_url);
        if with_jobs {
            let id = pipeline.id.to_string();
            let resp = self
                .api()
                .get_json::<Vec<RawJob>>(
                    &["projects", path, "pipelines", &id, "jobs"],
                    &[("per_page", "100")],
                )
                .await?;
            out.truncated = resp.next_page().is_some();
            out.jobs = resp.body.into_iter().map(CiJob::from).collect();
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(state: CiJobState, allow_failure: bool) -> CiJob {
        CiJob {
            name: "j".into(),
            kind: CiJobKind::Job,
            state,
            group: None,
            description: None,
            started_at: None,
            finished_at: None,
            url: None,
            allow_failure,
        }
    }

    #[test]
    fn folds_job_states() {
        use CiJobState::*;
        assert_eq!(fold_state(&[]), CiState::None);
        assert_eq!(fold_state(&[job(Skipped, false)]), CiState::Neutral);
        assert_eq!(
            fold_state(&[job(Success, false), job(Skipped, false)]),
            CiState::Success
        );
        assert_eq!(
            fold_state(&[job(Success, false), job(Running, false)]),
            CiState::Pending
        );
        assert_eq!(
            fold_state(&[job(Queued, false), job(Failure, false)]),
            CiState::Failure
        );
        assert_eq!(
            fold_state(&[job(Success, false), job(Failure, true)]),
            CiState::Success
        );
        assert_eq!(fold_state(&[job(Cancelled, false)]), CiState::Failure);
        assert!(CiState::Success.is_final() && !CiState::Pending.is_final());
        assert!(!CiState::None.is_final());
    }

    #[test]
    fn maps_provider_states() {
        assert_eq!(
            check_state("completed", Some("success")),
            CiJobState::Success
        );
        assert_eq!(
            check_state("completed", Some("timed_out")),
            CiJobState::Failure
        );
        assert_eq!(
            check_state("completed", Some("action_required")),
            CiJobState::Manual
        );
        assert_eq!(check_state("in_progress", None), CiJobState::Running);
        assert_eq!(check_state("queued", None), CiJobState::Queued);
        assert_eq!(gitlab_job_state("canceled"), CiJobState::Cancelled);
        assert_eq!(gitlab_pipeline_state("running"), CiState::Pending);
        assert_eq!(gitlab_pipeline_state("manual"), CiState::Neutral);
        assert_eq!(gitlab_pipeline_state("failed"), CiState::Failure);
    }

    #[test]
    fn unknown_commits_are_not_errors() {
        assert!(unknown_commit(&ProviderError::Rejected {
            status: 422,
            message: "No commit found for SHA: abc".into()
        }));
        assert!(unknown_commit(&ProviderError::NotFound {
            message: "No commit found for SHA: abc".into(),
            hint: None
        }));
        assert!(!unknown_commit(&ProviderError::NotFound {
            message: "Not Found".into(),
            hint: None
        }));
    }
}
