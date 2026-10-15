//! Pull requests (GitHub) / merge requests (GitLab) — Phase 4.1, PLAN §5.8.
//!
//! API commands take the account and the project (`fullName`) explicitly, so a pull request can
//! be opened from anywhere (sidebar of a repo, "My work" across accounts). The frontend gets
//! that account from `hosted_remote_list` (resolver, §4.3) or from a one-shot "only for this
//! action" choice (§4.5). Local commands (checkout, create context, local diff) run git with the
//! account context of the repo remote that points at the project.

use std::path::Path;

use nanomgg_git_engine::branch;
use nanomgg_git_engine::ops::{self, FailureKind, OpError};
use nanomgg_git_engine::progress::{CancelToken, run_streaming};
use nanomgg_git_engine::undo::{self, UndoKind};
use nanomgg_git_engine::{AccountContext, GitRunner};
use nanomgg_providers::pulls::{
    PullComment, PullCommentTarget, PullCommit, PullCreate, PullCreated, PullDetail, PullFiles,
    PullListQuery, PullMergeRequest, PullMerged, PullPage, PullReviewEvent, PullReviews,
};
use nanomgg_providers::{ProviderError, ProviderFamily};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::accounts::{self, AccountOverride};
use crate::branches::Outcome;
use crate::error::AppError;
use crate::hosting::{HostedRemote, account_client, hosted_remotes};
use crate::repos::repo_root;
use crate::state::AppState;

/// Local diffs larger than this are not sent to the UI.
const MAX_LOCAL_DIFF: usize = 2 * 1024 * 1024;

/// Error of the pull request commands: a provider answer (with the account that made the call,
/// so the UI can offer "sign in again" / "use another account") or an app error.
#[derive(Debug, thiserror::Error, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PullCommandError {
    #[error("{message}")]
    #[serde(rename_all = "camelCase")]
    Provider {
        message: String,
        account_id: String,
        /// Boxed: keeps `Result<_, PullCommandError>` small.
        error: Box<ProviderError>,
    },
    #[error("{message}")]
    #[serde(rename_all = "camelCase")]
    App { message: String, error: AppError },
}

impl From<AppError> for PullCommandError {
    fn from(error: AppError) -> Self {
        Self::App {
            message: error.to_string(),
            error,
        }
    }
}

type PullResult<T> = Result<T, PullCommandError>;

fn provider_err(account_id: &str) -> impl FnOnce(ProviderError) -> PullCommandError + '_ {
    move |error| PullCommandError::Provider {
        message: nanomgg_logging::redact(&error.to_string()).into_owned(),
        account_id: account_id.to_owned(),
        error: Box::new(error),
    }
}

fn internal(e: impl std::fmt::Display) -> PullCommandError {
    AppError::Internal(e.to_string()).into()
}

// ---------------------------------------------------------------------------------------------
// Provider API.

#[tauri::command]
#[specta::specta]
pub async fn pull_list(
    app: AppHandle,
    account_id: String,
    full_name: String,
    query: PullListQuery,
) -> PullResult<PullPage> {
    let client = account_client(&app, &account_id)?;
    client
        .pull_list(&full_name, &query)
        .await
        .map_err(provider_err(&account_id))
}

#[tauri::command]
#[specta::specta]
pub async fn pull_detail(
    app: AppHandle,
    account_id: String,
    full_name: String,
    number: u32,
) -> PullResult<PullDetail> {
    let client = account_client(&app, &account_id)?;
    client
        .pull_detail(&full_name, number.into())
        .await
        .map_err(provider_err(&account_id))
}

#[tauri::command]
#[specta::specta]
pub async fn pull_reviews(
    app: AppHandle,
    account_id: String,
    full_name: String,
    number: u32,
) -> PullResult<PullReviews> {
    let client = account_client(&app, &account_id)?;
    client
        .pull_reviews(&full_name, number.into())
        .await
        .map_err(provider_err(&account_id))
}

#[tauri::command]
#[specta::specta]
pub async fn pull_commits(
    app: AppHandle,
    account_id: String,
    full_name: String,
    number: u32,
) -> PullResult<Vec<PullCommit>> {
    let client = account_client(&app, &account_id)?;
    client
        .pull_commits(&full_name, number.into())
        .await
        .map_err(provider_err(&account_id))
}

#[tauri::command]
#[specta::specta]
pub async fn pull_files(
    app: AppHandle,
    account_id: String,
    full_name: String,
    number: u32,
) -> PullResult<PullFiles> {
    let client = account_client(&app, &account_id)?;
    client
        .pull_files(&full_name, number.into())
        .await
        .map_err(provider_err(&account_id))
}

#[tauri::command]
#[specta::specta]
pub async fn pull_comments(
    app: AppHandle,
    account_id: String,
    full_name: String,
    number: u32,
) -> PullResult<Vec<PullComment>> {
    let client = account_client(&app, &account_id)?;
    client
        .pull_comments(&full_name, number.into())
        .await
        .map_err(provider_err(&account_id))
}

#[tauri::command]
#[specta::specta]
pub async fn pull_comment_create(
    app: AppHandle,
    account_id: String,
    full_name: String,
    number: u32,
    target: PullCommentTarget,
    body: String,
) -> PullResult<PullComment> {
    let client = account_client(&app, &account_id)?;
    client
        .pull_comment(&full_name, number.into(), &target, &body)
        .await
        .map_err(provider_err(&account_id))
}

#[tauri::command]
#[specta::specta]
pub async fn pull_review_submit(
    app: AppHandle,
    account_id: String,
    full_name: String,
    number: u32,
    event: PullReviewEvent,
    body: String,
) -> PullResult<()> {
    let client = account_client(&app, &account_id)?;
    client
        .pull_review(&full_name, number.into(), event, &body)
        .await
        .map_err(provider_err(&account_id))
}

#[tauri::command]
#[specta::specta]
pub async fn pull_create(
    app: AppHandle,
    account_id: String,
    full_name: String,
    create: PullCreate,
) -> PullResult<PullCreated> {
    let client = account_client(&app, &account_id)?;
    let created = client
        .pull_create(&full_name, &create)
        .await
        .map_err(provider_err(&account_id))?;
    tracing::info!(number = created.number, "pull request created");
    Ok(created)
}

#[tauri::command]
#[specta::specta]
pub async fn pull_merge(
    app: AppHandle,
    account_id: String,
    full_name: String,
    number: u32,
    request: PullMergeRequest,
) -> PullResult<PullMerged> {
    let client = account_client(&app, &account_id)?;
    let merged = client
        .pull_merge(&full_name, number.into(), &request)
        .await
        .map_err(provider_err(&account_id))?;
    tracing::info!(number, method = ?request.method, "pull request merged");
    Ok(merged)
}

/// Closes (`open = false`) or reopens a pull request.
#[tauri::command]
#[specta::specta]
pub async fn pull_set_open(
    app: AppHandle,
    account_id: String,
    full_name: String,
    number: u32,
    open: bool,
) -> PullResult<()> {
    let client = account_client(&app, &account_id)?;
    client
        .pull_set_open(&full_name, number.into(), open)
        .await
        .map_err(provider_err(&account_id))
}

// ---------------------------------------------------------------------------------------------
// Local git: remotes of the project, create context, checkout, local diff.

/// Runner for write commands: English messages (so failures can be classified) and no editor.
fn writer(git: &GitRunner) -> GitRunner {
    git.clone()
        .with_env("LC_ALL", "en_US.UTF-8")
        .with_env("LANG", "en_US.UTF-8")
        .with_env("LANGUAGE", "en")
        .with_env("GIT_EDITOR", ":")
}

fn git_text(git: &GitRunner, repo: &Path, args: &[&str]) -> Option<String> {
    git.run_text(repo, &AccountContext::default(), args)
        .ok()
        .filter(|s| !s.is_empty())
}

fn ref_exists(git: &GitRunner, repo: &Path, full_ref: &str) -> bool {
    git.run(
        repo,
        &AccountContext::default(),
        ["show-ref", "--verify", "--quiet", full_ref],
    )
    .is_ok()
}

/// The repo remote whose hosted project is `full_name` (case-insensitive), `origin` first.
fn remote_for(remotes: &[HostedRemote], full_name: &str) -> Option<HostedRemote> {
    let want = full_name.trim_matches('/').trim_end_matches(".git");
    remotes
        .iter()
        .find(|r| r.full_name.eq_ignore_ascii_case(want))
        .cloned()
}

fn same_project(a: &str, b: &str) -> bool {
    a.trim_end_matches(".git")
        .eq_ignore_ascii_case(b.trim_end_matches(".git"))
}

/// Whether the branch is on a remote, and how far the local branch is ahead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullPushState {
    /// Remote the branch is (or would be) pushed to.
    pub remote: Option<String>,
    /// Name of the branch on that remote.
    pub remote_branch: String,
    /// The remote-tracking branch exists.
    pub pushed: bool,
    /// Local commits not on the remote yet.
    pub ahead: u32,
}

/// Everything the "Create pull request" dialog needs about a local branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullCreateContext {
    pub branch: String,
    /// Hosted remotes (targets), `origin` first.
    pub remotes: Vec<HostedRemote>,
    /// The chosen target remote (the requested one, else `upstream` when present, else the first).
    pub target: HostedRemote,
    /// Default branch of the target project (API, else the remote's `HEAD`, else `main`).
    pub target_branch: String,
    pub push: PullPushState,
    /// `source_branch` to send to the API (`owner:branch` for a GitHub fork).
    pub source_ref: String,
    /// The branch is pushed to another project than the target (a fork).
    pub cross_repo: bool,
    /// Cross-project merge requests are not supported for GitLab yet.
    pub cross_repo_unsupported: bool,
    pub suggested_title: String,
    pub suggested_body: String,
    /// Commits of the branch not in the target branch (as far as local refs know).
    pub commit_count: u32,
}

fn push_state(git: &GitRunner, repo: &Path, branch: &str) -> PullPushState {
    let up_remote = git_text(
        git,
        repo,
        &["config", "--get", &format!("branch.{branch}.remote")],
    )
    .filter(|r| r != ".");
    let up_merge = git_text(
        git,
        repo,
        &["config", "--get", &format!("branch.{branch}.merge")],
    );
    let remote_branch = up_merge
        .as_deref()
        .and_then(|m| m.strip_prefix("refs/heads/"))
        .unwrap_or(branch)
        .to_owned();
    let remotes: Vec<String> = git_text(git, repo, &["remote"])
        .map(|s| s.lines().map(|l| l.trim().to_owned()).collect())
        .unwrap_or_default();
    let remote = up_remote.or_else(|| {
        if remotes.iter().any(|r| r == "origin") {
            Some("origin".into())
        } else if remotes.len() == 1 {
            remotes.first().cloned()
        } else {
            None
        }
    });
    let tracking = remote
        .as_ref()
        .map(|r| format!("refs/remotes/{r}/{remote_branch}"));
    let pushed = tracking
        .as_deref()
        .is_some_and(|t| ref_exists(git, repo, t));
    let ahead = if pushed {
        let range = format!(
            "{}..refs/heads/{branch}",
            tracking.as_deref().unwrap_or_default()
        );
        git_text(git, repo, &["rev-list", "--count", &range, "--"])
            .and_then(|n| n.parse().ok())
            .unwrap_or(0)
    } else {
        0
    };
    PullPushState {
        remote,
        remote_branch,
        pushed,
        ahead,
    }
}

/// "feature/add-login_page" → "Add login page".
fn humanize_branch(branch: &str) -> String {
    let last = branch.rsplit('/').next().unwrap_or(branch);
    let words: Vec<&str> = last
        .split(['-', '_', ' '])
        .filter(|w| !w.is_empty())
        .collect();
    let text = words.join(" ");
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => branch.to_owned(),
    }
}

/// `(subject, body)` of the commits in `range`, newest first (at most `max`).
fn commit_messages(git: &GitRunner, repo: &Path, range: &str, max: u32) -> Vec<(String, String)> {
    let max = format!("--max-count={max}");
    git_text(
        git,
        repo,
        &["log", &max, "--format=%s%x1f%b%x1e", range, "--"],
    )
    .map(|out| {
        out.split('\x1e')
            .filter_map(|rec| {
                let rec = rec.trim_matches(['\n', '\r']);
                let (s, b) = rec.split_once('\x1f')?;
                Some((s.trim().to_owned(), b.trim().to_owned()))
            })
            .filter(|(s, _)| !s.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

fn suggest(branch: &str, commits: &[(String, String)]) -> (String, String) {
    match commits {
        [] => (humanize_branch(branch), String::new()),
        [(subject, body)] => (subject.clone(), body.clone()),
        many => (
            humanize_branch(branch),
            many.iter()
                .rev()
                .map(|(s, _)| format!("- {s}"))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
    }
}

/// Context for creating a pull request from `branch` (current branch when `None`) into the
/// project of `target_remote`, branch `target_branch` (default branch when `None`).
#[tauri::command]
#[specta::specta]
pub async fn pull_create_context(
    app: AppHandle,
    repo_id: i64,
    branch: Option<String>,
    target_remote: Option<String>,
    target_branch: Option<String>,
) -> PullResult<PullCreateContext> {
    let app2 = app.clone();
    let (branch, remotes, target, push, head_remote) =
        tauri::async_runtime::spawn_blocking(move || -> PullResult<_> {
            let st = app2.state::<AppState>();
            let repo = repo_root(&st, repo_id)?;
            let branch = match branch.filter(|b| !b.trim().is_empty()) {
                Some(b) => b,
                None => ops::current_branch(&st.git, &repo).ok_or_else(|| {
                    AppError::Unsupported(
                        "HEAD is detached — check out a branch to create a pull request".into(),
                    )
                })?,
            };
            branch::validate_branch_name(&st.git, &repo, &branch)
                .map_err(|e| AppError::InvalidInput(e.to_string()))?;
            let remotes = hosted_remotes(&st, repo_id)?;
            let target = match target_remote.as_deref() {
                Some(name) => remotes.iter().find(|r| r.remote == name).cloned(),
                None => remotes
                    .iter()
                    .find(|r| r.remote == "upstream")
                    .or(remotes.first())
                    .cloned(),
            }
            .ok_or_else(|| {
                AppError::Unsupported("this repository has no GitHub / GitLab remote".into())
            })?;
            let push = push_state(&st.git, &repo, &branch);
            let head_remote = push
                .remote
                .as_deref()
                .and_then(|r| remotes.iter().find(|h| h.remote == r).cloned());
            Ok((branch, remotes, target, push, head_remote))
        })
        .await
        .map_err(internal)??;

    // Default branch: API of the target's account, else the remote's HEAD, else `main`.
    let mut default_branch = target_branch.filter(|b| !b.trim().is_empty());
    if default_branch.is_none()
        && let Some(account) = target.account_id.as_deref().filter(|_| target.has_token)
        && let Ok(client) = account_client(&app, account)
    {
        default_branch = client.pull_default_branch(&target.full_name).await.ok();
    }
    let target_remote_name = target.remote.clone();
    let branch2 = branch.clone();
    let app3 = app.clone();
    let (target_branch, commits) = tauri::async_runtime::spawn_blocking(move || {
        let st = app3.state::<AppState>();
        let repo = repo_root(&st, repo_id)?;
        let target_branch = default_branch
            .or_else(|| {
                git_text(
                    &st.git,
                    &repo,
                    &[
                        "symbolic-ref",
                        "--short",
                        &format!("refs/remotes/{target_remote_name}/HEAD"),
                    ],
                )
                .and_then(|s| s.split_once('/').map(|(_, b)| b.to_owned()))
            })
            .unwrap_or_else(|| "main".into());
        let base = format!("refs/remotes/{target_remote_name}/{target_branch}");
        let head = format!("refs/heads/{branch2}");
        // Without the target branch locally, only the last commit is a safe guess.
        let commits = if ref_exists(&st.git, &repo, &base) {
            commit_messages(&st.git, &repo, &format!("{base}..{head}"), 50)
        } else {
            commit_messages(&st.git, &repo, &head, 1)
        };
        Ok::<_, PullCommandError>((target_branch, commits))
    })
    .await
    .map_err(internal)??;

    let head_project = head_remote
        .as_ref()
        .map(|h| h.full_name.clone())
        .unwrap_or_else(|| target.full_name.clone());
    let cross_repo = !same_project(&head_project, &target.full_name);
    let source_ref = if cross_repo {
        let owner = head_project.split('/').next().unwrap_or_default();
        format!("{owner}:{}", push.remote_branch)
    } else {
        push.remote_branch.clone()
    };
    let cross_repo_unsupported = cross_repo && target.family == Some(ProviderFamily::Gitlab);
    let (suggested_title, suggested_body) = suggest(&branch, &commits);
    Ok(PullCreateContext {
        commit_count: commits.len() as u32,
        branch,
        remotes,
        target,
        target_branch,
        push,
        source_ref,
        cross_repo,
        cross_repo_unsupported,
        suggested_title,
        suggested_body,
    })
}

/// What to check out.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullCheckoutRequest {
    /// Project of the pull request (its target).
    pub full_name: String,
    pub number: u32,
    pub source_branch: String,
    pub from_fork: bool,
    /// Provider family (which ref namespace holds the pull request head).
    pub family: ProviderFamily,
    /// Local branch name; `None` = the source branch name, with a suffix when taken.
    #[serde(default)]
    #[specta(optional)]
    pub local_name: Option<String>,
}

/// Result of a checkout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullCheckout {
    pub branch: String,
    /// A new local branch was created (else an existing one was switched to).
    pub created: bool,
    /// Upstream set for the branch (same-project pull requests only).
    pub upstream: Option<String>,
}

fn pull_ref(family: ProviderFamily, number: u32) -> String {
    match family {
        ProviderFamily::Github => format!("refs/pull/{number}/head"),
        ProviderFamily::Gitlab => format!("refs/merge-requests/{number}/head"),
    }
}

fn fetch_refspecs(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    remote: &str,
    refspecs: &[String],
) -> Result<(), OpError> {
    let mut args = vec![
        "fetch".to_owned(),
        "--progress".to_owned(),
        "--no-tags".to_owned(),
        "--end-of-options".to_owned(),
        remote.to_owned(),
    ];
    args.extend(refspecs.iter().cloned());
    run_streaming(&writer(git), repo, ctx, &args, &CancelToken::new(), |_| {})?;
    Ok(())
}

/// A free local branch name: `base`, else `base-pr<N>`, else `base-pr<N>-2`…
fn free_branch_name(git: &GitRunner, repo: &Path, base: &str, number: u32) -> String {
    let taken = |n: &str| ref_exists(git, repo, &format!("refs/heads/{n}"));
    if !taken(base) {
        return base.to_owned();
    }
    let with_pr = format!("{base}-pr{number}");
    if !taken(&with_pr) {
        return with_pr;
    }
    (2..100)
        .map(|i| format!("{with_pr}-{i}"))
        .find(|n| !taken(n))
        .unwrap_or_else(|| format!("{with_pr}-{}", std::process::id()))
}

pub(crate) fn checkout_pull(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    remote: &str,
    req: &PullCheckoutRequest,
) -> Result<PullCheckout, OpError> {
    let src = req.source_branch.trim();
    if !req.from_fork {
        // Same project: a normal tracking branch of `remote/src`.
        branch::validate_branch_name(git, repo, src)?;
        let tracking = format!("refs/remotes/{remote}/{src}");
        fetch_refspecs(
            git,
            repo,
            ctx,
            remote,
            &[format!("+refs/heads/{src}:{tracking}")],
        )?;
        let upstream = format!("{remote}/{src}");
        let wanted = req.local_name.as_deref().unwrap_or(src);
        if ref_exists(git, repo, &format!("refs/heads/{wanted}")) {
            let current = git_text(
                git,
                repo,
                &[
                    "rev-parse",
                    "--abbrev-ref",
                    &format!("{wanted}@{{upstream}}"),
                ],
            );
            if current.as_deref() == Some(upstream.as_str()) {
                branch::checkout_branch(git, repo, ctx, wanted)?;
                return Ok(PullCheckout {
                    branch: wanted.to_owned(),
                    created: false,
                    upstream: Some(upstream),
                });
            }
            if req.local_name.is_some() {
                return Err(OpError::rejected(
                    FailureKind::AlreadyExists,
                    format!("a branch named '{wanted}' already exists"),
                ));
            }
        }
        let name = match &req.local_name {
            Some(n) => n.clone(),
            None => free_branch_name(git, repo, src, req.number),
        };
        let kind = UndoKind::CreateBranch { name: name.clone() };
        let created = undo::record(git, repo, kind, &[&name], || {
            branch::checkout_remote_branch(git, repo, ctx, remote, src, Some(&name))
        })?;
        return Ok(PullCheckout {
            branch: created,
            created: true,
            upstream: Some(upstream),
        });
    }
    // Fork: the head lives in the target project's pull request ref.
    fetch_refspecs(git, repo, ctx, remote, &[pull_ref(req.family, req.number)])?;
    let oid = ops::resolve_commit(git, repo, "FETCH_HEAD")?;
    let base = req.local_name.as_deref().unwrap_or(src);
    branch::validate_branch_name(git, repo, base)?;
    let existing = format!("refs/heads/{base}");
    if ref_exists(git, repo, &existing) {
        let at = git_text(git, repo, &["rev-parse", &existing]);
        if at.as_deref() == Some(oid.as_str()) {
            branch::checkout_branch(git, repo, ctx, base)?;
            return Ok(PullCheckout {
                branch: base.to_owned(),
                created: false,
                upstream: None,
            });
        }
        if req.local_name.is_some() {
            return Err(OpError::rejected(
                FailureKind::AlreadyExists,
                format!("a branch named '{base}' already exists"),
            ));
        }
    }
    let name = free_branch_name(git, repo, base, req.number);
    let kind = UndoKind::CreateBranch { name: name.clone() };
    undo::record(git, repo, kind, &[&name], || {
        branch::create_branch(git, repo, ctx, &name, &oid, true)
    })?;
    Ok(PullCheckout {
        branch: name,
        created: true,
        upstream: None,
    })
}

fn outcome<T>(result: Result<T, OpError>) -> Result<Outcome<T>, AppError> {
    match result {
        Ok(value) => Ok(Outcome::Done { value }),
        Err(OpError::Rejected { kind, message }) => Ok(Outcome::Rejected {
            reason: kind,
            message: nanomgg_logging::redact(&message).into_owned(),
        }),
        Err(OpError::Cancelled) => Ok(Outcome::Cancelled),
        Err(OpError::Git(e)) => Err(e.into()),
    }
}

/// The repo remote of `full_name` and the git context to fetch from it (`as_account` = one-shot
/// override).
fn remote_context(
    st: &AppState,
    repo_id: i64,
    full_name: &str,
    as_account: Option<&AccountOverride>,
) -> Result<(String, AccountContext), AppError> {
    let remote = remote_for(&hosted_remotes(st, repo_id)?, full_name).ok_or_else(|| {
        AppError::Unsupported(format!(
            "no remote of this repository points at {full_name} — add it as a remote first"
        ))
    })?;
    let ctx = accounts::context_with(st, repo_id, Some(&remote.remote), as_account)?;
    Ok((remote.remote, ctx))
}

/// Checks out the pull request's source branch: a tracking branch for same-project pull
/// requests, a local branch at `refs/pull/N/head` / `refs/merge-requests/N/head` for forks.
#[tauri::command]
#[specta::specta]
pub async fn pull_checkout(
    app: AppHandle,
    repo_id: i64,
    request: PullCheckoutRequest,
    as_account: Option<AccountOverride>,
) -> PullResult<Outcome<PullCheckout>> {
    let repo = repo_root(&app.state::<AppState>(), repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        let (remote, ctx) = remote_context(&st, repo_id, &request.full_name, as_account.as_ref())?;
        let result = checkout_pull(&st.git, &repo, &ctx, &remote, &request);
        tracing::info!(
            repo = repo_id,
            number = request.number,
            ok = result.is_ok(),
            "pull request checkout"
        );
        Ok(outcome(result)?)
    })
    .await
    .map_err(internal)?
}

/// Unified diff of one file computed locally (for files whose patch the provider left out).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullLocalDiff {
    /// Hunks from the first `@@`; `None` for binary files or when too large.
    pub patch: Option<String>,
    pub binary: bool,
    pub too_large: bool,
}

/// What [`pull_local_diff`] compares.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullLocalDiffRequest {
    pub full_name: String,
    pub number: u32,
    pub family: ProviderFamily,
    pub base_sha: String,
    pub head_sha: String,
    pub target_branch: String,
    pub path: String,
    #[serde(default)]
    #[specta(optional)]
    pub old_path: Option<String>,
}

fn is_sha(s: &str) -> bool {
    (7..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn has_commit(git: &GitRunner, repo: &Path, sha: &str) -> bool {
    git.run(
        repo,
        &AccountContext::default(),
        ["cat-file", "-e", &format!("{sha}^{{commit}}")],
    )
    .is_ok()
}

/// Strips the `diff --git` / `index` / `---` / `+++` header so the text starts at the first hunk.
fn hunks_only(diff: &str) -> &str {
    match diff.find("\n@@") {
        Some(i) => &diff[i + 1..],
        None if diff.starts_with("@@") => diff,
        None => "",
    }
}

pub(crate) fn local_diff(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    remote: &str,
    req: &PullLocalDiffRequest,
) -> Result<PullLocalDiff, OpError> {
    if !is_sha(&req.base_sha) || !is_sha(&req.head_sha) {
        return Err(OpError::rejected(
            FailureKind::InvalidInput,
            "invalid commit id",
        ));
    }
    if !has_commit(git, repo, &req.head_sha) || !has_commit(git, repo, &req.base_sha) {
        let mut specs = vec![pull_ref(req.family, req.number)];
        if branch::validate_branch_name(git, repo, &req.target_branch).is_ok() {
            specs.push(format!("refs/heads/{}", req.target_branch));
        }
        fetch_refspecs(git, repo, ctx, remote, &specs)?;
    }
    let range = format!("{}...{}", req.base_sha, req.head_sha);
    let mut args = vec![
        "diff",
        "--no-color",
        "--no-ext-diff",
        "-M",
        range.as_str(),
        "--",
        req.path.as_str(),
    ];
    if let Some(old) = req.old_path.as_deref().filter(|o| *o != req.path) {
        args.push(old);
    }
    let out = git
        .run(repo, &AccountContext::default(), &args)
        .map_err(ops::classify)?;
    if out.stdout.len() > MAX_LOCAL_DIFF {
        return Ok(PullLocalDiff {
            patch: None,
            binary: false,
            too_large: true,
        });
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let binary = text
        .lines()
        .take(6)
        .any(|l| l.starts_with("Binary files ") || l == "GIT binary patch");
    let hunks = hunks_only(&text);
    Ok(PullLocalDiff {
        patch: (!binary && !hunks.is_empty()).then(|| hunks.to_owned()),
        binary,
        too_large: false,
    })
}

/// Diff of one file of a pull request computed with local git (fetching the pull request ref
/// first when its commits are not here yet).
#[tauri::command]
#[specta::specta]
pub async fn pull_local_diff(
    app: AppHandle,
    repo_id: i64,
    request: PullLocalDiffRequest,
) -> PullResult<Outcome<PullLocalDiff>> {
    let repo = repo_root(&app.state::<AppState>(), repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        let (remote, ctx) = remote_context(&st, repo_id, &request.full_name, None)?;
        Ok(outcome(local_diff(
            &st.git, &repo, &ctx, &remote, &request,
        ))?)
    })
    .await
    .map_err(internal)?
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    fn git_cmd(dir: &Path, args: &[&str]) {
        let st = Command::new("git")
            .current_dir(dir)
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .status()
            .unwrap();
        assert!(st.success(), "git {args:?}");
    }

    fn init(dir: &Path) {
        git_cmd(dir, &["init", "-q", "-b", "main"]);
        git_cmd(dir, &["config", "user.name", "T"]);
        git_cmd(dir, &["config", "user.email", "t@example.com"]);
        git_cmd(dir, &["config", "core.autocrlf", "false"]);
        git_cmd(dir, &["config", "commit.gpgsign", "false"]);
    }

    fn commit(dir: &Path, file: &str, content: &str, msg: &str) {
        std::fs::write(dir.join(file), content).unwrap();
        git_cmd(dir, &["add", file]);
        git_cmd(dir, &["commit", "-q", "-m", msg]);
    }

    /// `server` (bare-ish: a normal repo with `refs/pull/7/head`) + a clone `work`.
    fn setup() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let server = tmp.path().join("server");
        let work = tmp.path().join("work");
        std::fs::create_dir_all(&server).unwrap();
        init(&server);
        commit(&server, "a.txt", "one\n", "base");
        git_cmd(&server, &["checkout", "-q", "-b", "feature/x"]);
        commit(&server, "a.txt", "one\ntwo\n", "Add two");
        git_cmd(&server, &["checkout", "-q", "main"]);
        // A fork's head that only exists as a pull request ref.
        git_cmd(&server, &["checkout", "-q", "-b", "tmp-fork"]);
        commit(&server, "b.txt", "fork\n", "Fork change");
        git_cmd(&server, &["update-ref", "refs/pull/7/head", "HEAD"]);
        git_cmd(&server, &["checkout", "-q", "main"]);
        git_cmd(&server, &["branch", "-q", "-D", "tmp-fork"]);
        let s = server.to_string_lossy().replace('\\', "/");
        // `--no-local`: only reachable objects, like a real clone (the fork head stays remote).
        git_cmd(
            tmp.path(),
            &[
                "-c",
                "core.autocrlf=false",
                "clone",
                "-q",
                "--no-local",
                &s,
                "work",
            ],
        );
        init_identity(&work);
        (tmp, server, work)
    }

    fn init_identity(dir: &Path) {
        git_cmd(dir, &["config", "user.name", "T"]);
        git_cmd(dir, &["config", "user.email", "t@example.com"]);
        git_cmd(dir, &["config", "core.autocrlf", "false"]);
        git_cmd(dir, &["config", "commit.gpgsign", "false"]);
    }

    fn runner() -> GitRunner {
        GitRunner::new("git")
    }

    fn req(from_fork: bool, number: u32, src: &str) -> PullCheckoutRequest {
        PullCheckoutRequest {
            full_name: "acme/app".into(),
            number,
            source_branch: src.into(),
            from_fork,
            family: ProviderFamily::Github,
            local_name: None,
        }
    }

    #[test]
    fn checkout_same_project_tracks_and_reuses() {
        let (_tmp, _server, work) = setup();
        let git = runner();
        let ctx = AccountContext::default();
        let r = checkout_pull(&git, &work, &ctx, "origin", &req(false, 3, "feature/x")).unwrap();
        assert_eq!(r.branch, "feature/x");
        assert!(r.created);
        assert_eq!(r.upstream.as_deref(), Some("origin/feature/x"));
        assert_eq!(
            ops::current_branch(&git, &work).as_deref(),
            Some("feature/x")
        );
        // Again: the existing tracking branch is reused.
        git_cmd(&work, &["checkout", "-q", "main"]);
        let r = checkout_pull(&git, &work, &ctx, "origin", &req(false, 3, "feature/x")).unwrap();
        assert!(!r.created);
        assert_eq!(r.branch, "feature/x");
    }

    #[test]
    fn checkout_fork_uses_pull_ref_and_avoids_name_clash() {
        let (_tmp, _server, work) = setup();
        let git = runner();
        let ctx = AccountContext::default();
        // A local `main` exists already: the fork's `main` gets a suffix.
        let r = checkout_pull(&git, &work, &ctx, "origin", &req(true, 7, "main")).unwrap();
        assert_eq!(r.branch, "main-pr7");
        assert!(r.created && r.upstream.is_none());
        assert!(work.join("b.txt").exists());
        // Same head again: switched back to, not recreated.
        git_cmd(&work, &["checkout", "-q", "main"]);
        let r = checkout_pull(&git, &work, &ctx, "origin", &req(true, 7, "main-pr7")).unwrap();
        assert_eq!(r.branch, "main-pr7");
        assert!(!r.created);
        // An explicit name that is taken is refused.
        let mut taken = req(true, 7, "main");
        taken.local_name = Some("main".into());
        let e = checkout_pull(&git, &work, &ctx, "origin", &taken).unwrap_err();
        assert!(matches!(
            e,
            OpError::Rejected {
                kind: FailureKind::AlreadyExists,
                ..
            }
        ));
        // A missing pull request ref is a recognisable failure.
        assert!(checkout_pull(&git, &work, &ctx, "origin", &req(true, 99, "x")).is_err());
    }

    #[test]
    fn local_diff_fetches_missing_commits() {
        let (_tmp, server, work) = setup();
        let git = runner();
        let head = git_text(&git, &server, &["rev-parse", "refs/pull/7/head"]).unwrap();
        let base = git_text(&git, &server, &["rev-parse", "main"]).unwrap();
        assert!(!has_commit(&git, &work, &head));
        let r = PullLocalDiffRequest {
            full_name: "acme/app".into(),
            number: 7,
            family: ProviderFamily::Github,
            base_sha: base,
            head_sha: head,
            target_branch: "main".into(),
            path: "b.txt".into(),
            old_path: None,
        };
        let d = local_diff(&git, &work, &AccountContext::default(), "origin", &r).unwrap();
        let patch = d.patch.unwrap();
        assert!(patch.starts_with("@@"), "{patch}");
        assert!(patch.contains("+fork"));
        assert!(!d.binary && !d.too_large);
        let bad = PullLocalDiffRequest {
            base_sha: "--output=x".into(),
            ..r
        };
        assert!(local_diff(&git, &work, &AccountContext::default(), "origin", &bad).is_err());
    }

    #[test]
    fn push_state_and_suggestions() {
        let (_tmp, _server, work) = setup();
        let git = runner();
        git_cmd(&work, &["checkout", "-q", "-b", "feat/new-login_page"]);
        commit(&work, "c.txt", "c\n", "Add login\n\nDetails here");
        let s = push_state(&git, &work, "feat/new-login_page");
        assert_eq!(s.remote.as_deref(), Some("origin"));
        assert!(!s.pushed);
        let msgs = commit_messages(&git, &work, "refs/remotes/origin/main..HEAD", 50);
        assert_eq!(msgs.len(), 1);
        assert_eq!(
            suggest("feat/new-login_page", &msgs),
            ("Add login".into(), "Details here".into())
        );
        commit(&work, "d.txt", "d\n", "Second");
        let msgs = commit_messages(&git, &work, "refs/remotes/origin/main..HEAD", 50);
        assert_eq!(
            suggest("feat/new-login_page", &msgs),
            ("New login page".into(), "- Add login\n- Second".into())
        );
        git_cmd(
            &work,
            &["push", "-q", "-u", "origin", "feat/new-login_page"],
        );
        commit(&work, "e.txt", "e\n", "Third");
        let s = push_state(&git, &work, "feat/new-login_page");
        assert!(s.pushed);
        assert_eq!(s.ahead, 1);
        assert_eq!(s.remote_branch, "feat/new-login_page");
    }

    #[test]
    fn helpers() {
        assert_eq!(humanize_branch("main"), "Main");
        assert_eq!(
            hunks_only("diff --git a b\n--- a\n+++ b\n@@ -1 +1 @@\n-a\n+b\n"),
            "@@ -1 +1 @@\n-a\n+b\n"
        );
        assert_eq!(hunks_only("Binary files a and b differ\n"), "");
        assert!(same_project("Acme/App.git", "acme/app"));
        let r = |name: &str| HostedRemote {
            remote: name.into(),
            host: "github.com".into(),
            full_name: format!("{name}/app"),
            account_id: None,
            family: None,
            has_token: false,
        };
        let remotes = [r("origin"), r("upstream")];
        assert_eq!(
            remote_for(&remotes, "Upstream/app.git").unwrap().remote,
            "upstream"
        );
        assert!(remote_for(&remotes, "other/app").is_none());
        let e = PullCommandError::from(AppError::InvalidInput("x".into()));
        assert_eq!(
            serde_json::to_value(&e).unwrap()["kind"],
            serde_json::json!("app")
        );
        let e = provider_err("acc_1")(ProviderError::Unauthorized {
            message: "Bad credentials".into(),
        });
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "provider");
        assert_eq!(v["accountId"], "acc_1");
        assert_eq!(v["error"]["kind"], "unauthorized");
    }
}
