//! Issues of the hosted project behind a repository (Phase 4.2): list, details, comments,
//! create, close / reopen, labels / assignees, and "create a branch from this issue".
//!
//! Listing and creating start from an open repository, so the account is the one the resolver
//! picks for its remote (`hosting::remote_client`). Everything else takes the account and project
//! of a `HostedItemRef` (the UI may also open issues of repositories that are not open, from
//! "My work"); writes take the account the user confirmed ("as ● account").

use nanomgg_git_engine::branch;
use nanomgg_git_engine::undo::{self, UndoKind};
use nanomgg_providers::issues::{
    IssueComment, IssueCommentList, IssueCreate, IssueDetail, IssueLabel, IssueListQuery,
    IssuePage, IssueUpdate, IssueUser,
};
use nanomgg_providers::{ProviderClient, ProviderError};
use serde::Serialize;
use specta::Type;
use tauri::AppHandle;

use crate::branches::{Outcome, run_op};
use crate::error::AppError;
use crate::hosting::{account_client, remote_client};

/// Error of the issue / "My work" commands: a typed provider error (the UI explains missing
/// scopes, 404 on private repositories, rate limits…) or an app error (no hosted remote, no
/// token, storage).
#[derive(Debug, thiserror::Error, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum IssueCommandError {
    #[error("{message}")]
    #[serde(rename_all = "camelCase")]
    Provider {
        message: String,
        error: ProviderError,
    },
    #[error("{message}")]
    #[serde(rename_all = "camelCase")]
    App { message: String, error: AppError },
}

impl From<ProviderError> for IssueCommandError {
    fn from(error: ProviderError) -> Self {
        Self::Provider {
            message: nanomgg_logging::redact(&error.to_string()).into_owned(),
            error,
        }
    }
}

impl From<AppError> for IssueCommandError {
    fn from(error: AppError) -> Self {
        Self::App {
            message: error.to_string(),
            error,
        }
    }
}

pub type IssueResult<T> = Result<T, IssueCommandError>;

/// Runs blocking setup (keychain, git remotes) off the async runtime.
pub(crate) async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

async fn client_of(app: &AppHandle, account_id: String) -> IssueResult<ProviderClient> {
    let app = app.clone();
    Ok(blocking(move || account_client(&app, &account_id)).await?)
}

/// Which project and account a repository-scoped answer used.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IssueListResult {
    pub page: IssuePage,
    pub account_id: String,
    pub full_name: String,
    pub remote: String,
}

/// A page of issues of the project behind `remote` (default: `origin`, else the first hosted
/// remote) of an open repository.
#[tauri::command]
#[specta::specta]
pub async fn issue_list(
    app: AppHandle,
    repo_id: i64,
    remote: Option<String>,
    query: IssueListQuery,
) -> IssueResult<IssueListResult> {
    let a = app.clone();
    let (client, hosted) =
        blocking(move || remote_client(&a, repo_id, remote.as_deref(), None)).await?;
    let page = client.issue_list(&hosted.full_name, &query).await?;
    Ok(IssueListResult {
        page,
        account_id: hosted.account_id.unwrap_or_default(),
        full_name: hosted.full_name,
        remote: hosted.remote,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn issue_get(
    app: AppHandle,
    account_id: String,
    full_name: String,
    number: u32,
) -> IssueResult<IssueDetail> {
    let client = client_of(&app, account_id).await?;
    Ok(client.issue(&full_name, u64::from(number)).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn issue_comments(
    app: AppHandle,
    account_id: String,
    full_name: String,
    number: u32,
) -> IssueResult<IssueCommentList> {
    let client = client_of(&app, account_id).await?;
    Ok(client.issue_comments(&full_name, u64::from(number)).await?)
}

/// Posts a comment as `account_id` (the account the user confirmed).
#[tauri::command]
#[specta::specta]
pub async fn issue_comment_create(
    app: AppHandle,
    account_id: String,
    full_name: String,
    number: u32,
    body: String,
) -> IssueResult<IssueComment> {
    let client = client_of(&app, account_id).await?;
    Ok(client
        .issue_comment_create(&full_name, u64::from(number), &body)
        .await?)
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IssueCreated {
    pub issue: IssueDetail,
    pub account_id: String,
    pub full_name: String,
}

/// Creates an issue on the project behind `remote` of an open repository, as the resolved
/// account or `as_account` ("only for this action").
#[tauri::command]
#[specta::specta]
pub async fn issue_create(
    app: AppHandle,
    repo_id: i64,
    remote: Option<String>,
    as_account: Option<String>,
    input: IssueCreate,
) -> IssueResult<IssueCreated> {
    let a = app.clone();
    let override_account = as_account.clone();
    let (client, hosted) = blocking(move || {
        remote_client(&a, repo_id, remote.as_deref(), override_account.as_deref())
    })
    .await?;
    let issue = client.issue_create(&hosted.full_name, &input).await?;
    Ok(IssueCreated {
        issue,
        account_id: as_account.or(hosted.account_id).unwrap_or_default(),
        full_name: hosted.full_name,
    })
}

/// Close / reopen, replace labels or assignees, as `account_id`.
#[tauri::command]
#[specta::specta]
pub async fn issue_update(
    app: AppHandle,
    account_id: String,
    full_name: String,
    number: u32,
    change: IssueUpdate,
) -> IssueResult<IssueDetail> {
    let client = client_of(&app, account_id).await?;
    Ok(client
        .issue_update(&full_name, u64::from(number), &change)
        .await?)
}

#[tauri::command]
#[specta::specta]
pub async fn issue_labels(
    app: AppHandle,
    account_id: String,
    full_name: String,
) -> IssueResult<Vec<IssueLabel>> {
    let client = client_of(&app, account_id).await?;
    Ok(client.issue_labels(&full_name).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn issue_assignable_users(
    app: AppHandle,
    account_id: String,
    full_name: String,
) -> IssueResult<Vec<IssueUser>> {
    let client = client_of(&app, account_id).await?;
    Ok(client.issue_assignable_users(&full_name).await?)
}

// ---------------------------------------------------------------------------------------------
// Branch from an issue.

/// Longest slug kept after `feature/<n>-`.
const SLUG_MAX: usize = 48;

/// Base letter of an accented Latin letter (Vietnamese first), `None` when not accented.
fn unaccent(c: char) -> Option<&'static str> {
    const TABLE: &[(&str, &str)] = &[
        ("àáảãạăằắẳẵặâầấẩẫậäåāą", "a"),
        ("èéẻẽẹêềếểễệëēęě", "e"),
        ("ìíỉĩịîïī", "i"),
        ("òóỏõọôồốổỗộơờớởỡợöøō", "o"),
        ("ùúủũụưừứửữựûüūů", "u"),
        ("ỳýỷỹỵÿ", "y"),
        ("đð", "d"),
        ("çćč", "c"),
        ("ñńň", "n"),
        ("śšş", "s"),
        ("źżž", "z"),
        ("ľłĺ", "l"),
        ("řŕ", "r"),
        ("ťţ", "t"),
        ("ğ", "g"),
        ("ß", "ss"),
        ("æ", "ae"),
        ("œ", "oe"),
    ];
    TABLE
        .iter()
        .find(|(chars, _)| chars.contains(c))
        .map(|(_, base)| *base)
}

/// `"Sửa lỗi đăng nhập (Google)"` → `"sua-loi-dang-nhap-google"`: ASCII letters / digits
/// separated by single dashes, at most [`SLUG_MAX`] characters, cut at a word boundary.
pub(crate) fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in title.chars().flat_map(char::to_lowercase) {
        let piece: Option<std::borrow::Cow<str>> = if c.is_ascii_alphanumeric() {
            Some(c.to_string().into())
        } else {
            unaccent(c).map(Into::into)
        };
        match piece {
            Some(p) => {
                if dash && !out.is_empty() {
                    out.push('-');
                }
                dash = false;
                out.push_str(&p);
            }
            None => dash = true,
        }
    }
    if out.len() > SLUG_MAX {
        let cut = out[..SLUG_MAX].rfind('-').filter(|&i| i >= SLUG_MAX / 2);
        out.truncate(cut.unwrap_or(SLUG_MAX));
    }
    out.trim_matches('-').to_owned()
}

/// `feature/<number>-<slug>` (just `feature/<number>` when the title has no usable letters).
pub(crate) fn branch_name_for(number: u32, title: &str) -> String {
    let slug = slugify(title);
    if slug.is_empty() {
        format!("feature/{number}")
    } else {
        format!("feature/{number}-{slug}")
    }
}

/// Suggested branch name for an issue (the UI lets the user edit it).
#[tauri::command]
#[specta::specta]
pub fn issue_branch_name(number: u32, title: String) -> String {
    branch_name_for(number, &title)
}

/// Creates local branch `name` (default: [`issue_branch_name`]) at `start` (default: `HEAD`),
/// optionally switching to it. Names are checked with `git check-ref-format --branch`;
/// recorded for Undo like any branch creation. Returns the branch name.
#[tauri::command]
#[specta::specta]
pub async fn issue_branch_create(
    app: AppHandle,
    repo_id: i64,
    number: u32,
    title: String,
    name: Option<String>,
    start: Option<String>,
    checkout: bool,
) -> Result<Outcome<String>, AppError> {
    let name = name
        .map(|n| n.trim().to_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| branch_name_for(number, &title));
    let start = start
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "HEAD".into());
    run_op(app, repo_id, move |git, repo, ctx| {
        let kind = UndoKind::CreateBranch { name: name.clone() };
        undo::record(git, repo, kind, &[&name], || {
            branch::create_branch(git, repo, ctx, &name, &start, checkout)
        })?;
        Ok(name)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_drop_vietnamese_accents() {
        assert_eq!(
            slugify("Sửa lỗi đăng nhập (Google) — Ứng dụng"),
            "sua-loi-dang-nhap-google-ung-dung"
        );
        assert_eq!(slugify("ĐĂNG KÝ tài khoản!!"), "dang-ky-tai-khoan");
        assert_eq!(slugify("Crème brûlée & ñandú"), "creme-brulee-nandu");
        assert_eq!(slugify("  --  "), "");
        assert_eq!(slugify("日本語"), "");
        assert_eq!(slugify("..lock/~^:?*[\\ name"), "lock-name");
        let long = slugify(&"word ".repeat(30));
        assert!(long.len() <= SLUG_MAX && !long.ends_with('-'), "{long}");
        assert_eq!(branch_name_for(123, "Tiêu đề"), "feature/123-tieu-de");
        assert_eq!(branch_name_for(7, "???"), "feature/7");
    }

    #[test]
    fn branch_names_pass_check_ref_format() {
        let dir = tempfile::tempdir().unwrap();
        let git = nanomgg_git_engine::GitRunner::default();
        for title in ["Sửa lỗi .lock", "a..b", "@{x}", "ends with.", "HEAD"] {
            let name = branch_name_for(42, title);
            branch::validate_branch_name(&git, dir.path(), &name)
                .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        }
    }

    #[test]
    fn provider_errors_keep_their_kind() {
        let e = IssueCommandError::from(ProviderError::RateLimited {
            reset_at: Some(10),
            retry_after_secs: None,
        });
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "provider");
        assert_eq!(v["error"]["kind"], "rateLimited");
        let e = IssueCommandError::from(AppError::Unsupported("no remote".into()));
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "app");
        assert_eq!(v["error"]["kind"], "unsupported");
    }
}
