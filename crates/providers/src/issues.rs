//! Issues of one hosted project (Phase 4.2): list with filters and paging, details, comments,
//! create, close / reopen, labels and assignees.
//!
//! GitHub: REST `repos/{owner}/{repo}/issues` (which also returns pull requests — those are
//! dropped from listings), text search through `search/issues`. GitLab: REST v4
//! `projects/{url-encoded path}/issues`, addressed by the project-scoped `iid`.

use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::ProviderClient;
use crate::error::ProviderError;
use crate::github::GithubClient;
use crate::gitlab::GitlabClient;
use crate::timefmt::parse_timestamp;

/// Pages read at most when a call collects a whole list (comments, labels, assignees).
const MAX_PAGES: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum IssueState {
    Open,
    Closed,
}

/// Which issues a listing returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum IssueStateFilter {
    #[default]
    Open,
    Closed,
    All,
}

/// A user as shown on an issue (author, assignee, commenter).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct IssueUser {
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct IssueLabel {
    pub name: String,
    /// `#rrggbb` when the provider sent a valid color.
    pub color: Option<String>,
    pub description: Option<String>,
}

/// One row of an issue listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct IssueSummary {
    /// GitHub issue number / GitLab `iid`.
    pub number: u64,
    pub title: String,
    pub state: IssueState,
    pub author: Option<IssueUser>,
    pub labels: Vec<IssueLabel>,
    pub assignees: Vec<IssueUser>,
    pub comments: u32,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub web_url: String,
    /// GitHub numbers are shared by issues and pull requests; `true` when this number is a PR
    /// (only possible from [`ProviderClient::issue`]; listings drop pull requests).
    pub is_pull_request: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct IssueDetail {
    pub issue: IssueSummary,
    /// Markdown source as written (untrusted).
    pub body: String,
    pub closed_at: Option<i64>,
    pub closed_by: Option<IssueUser>,
    /// Comments are limited to collaborators.
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct IssuePage {
    pub items: Vec<IssueSummary>,
    pub next_page: Option<u32>,
    /// Total matches when the provider reports it.
    pub total: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct IssueComment {
    pub id: String,
    pub author: Option<IssueUser>,
    pub body: String,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub web_url: Option<String>,
    /// GitLab system note ("changed the description", "added ~bug label"…).
    pub system: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct IssueCommentList {
    pub comments: Vec<IssueComment>,
    /// More comments exist than were read.
    pub truncated: bool,
}

/// Filters of an issue listing. `assignee` / `author` are usernames; `assignee = "@none"`
/// lists unassigned issues.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct IssueListQuery {
    #[serde(default)]
    pub state: IssueStateFilter,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    /// Free text (title / description).
    #[serde(default)]
    pub search: Option<String>,
    /// 1-based.
    #[serde(default)]
    pub page: u32,
    /// 1..=100 (0 = 30).
    #[serde(default)]
    pub per_page: u8,
}

/// Special `assignee` filter value: issues without assignee.
pub const ASSIGNEE_NONE: &str = "@none";

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct IssueCreate {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub labels: Vec<String>,
    /// Usernames.
    #[serde(default)]
    pub assignees: Vec<String>,
}

/// Fields to change; `None` keeps the current value, an empty list clears it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct IssueUpdate {
    #[serde(default)]
    pub state: Option<IssueState>,
    #[serde(default)]
    pub labels: Option<Vec<String>>,
    /// Usernames.
    #[serde(default)]
    pub assignees: Option<Vec<String>>,
}

fn invalid(message: String) -> ProviderError {
    ProviderError::InvalidInput { message }
}

/// `owner/repo` → `[owner, repo]` (GitHub).
pub(crate) fn github_repo(full_name: &str) -> Result<[&str; 2], ProviderError> {
    match full_name.trim_matches('/').split_once('/') {
        Some((owner, name)) if !owner.is_empty() && !name.is_empty() && !name.contains('/') => {
            Ok([owner, name.strip_suffix(".git").unwrap_or(name)])
        }
        _ => Err(invalid(format!("`{full_name}` is not an owner/repo name"))),
    }
}

/// `group/sub/project` (GitLab); sent as one URL-encoded path segment.
pub(crate) fn gitlab_project(full_name: &str) -> Result<&str, ProviderError> {
    let path = full_name.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    if path.contains('/') && !path.split('/').any(str::is_empty) {
        Ok(path)
    } else {
        Err(invalid(format!(
            "`{full_name}` is not a group/project path"
        )))
    }
}

fn page_args(page: u32, per_page: u8) -> (String, String) {
    let per_page = if per_page == 0 { 30 } else { per_page.min(100) };
    (page.max(1).to_string(), per_page.to_string())
}

fn number_s(number: u64) -> Result<String, ProviderError> {
    if number == 0 {
        return Err(invalid("issue number must be positive".into()));
    }
    Ok(number.to_string())
}

fn clean_list(items: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in items {
        let item = item.trim();
        if !item.is_empty() && !out.iter().any(|o| o == item) {
            out.push(item.to_owned());
        }
    }
    out
}

fn non_empty(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|s| !s.is_empty())
}

/// `rrggbb` / `#rrggbb` → `#rrggbb`; anything else is dropped (the UI uses it in styles).
pub(crate) fn label_color(raw: Option<&str>) -> Option<String> {
    let hex = raw?.trim().trim_start_matches('#');
    (hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| format!("#{}", hex.to_ascii_lowercase()))
}

/// Search qualifier value, quoted when it contains spaces (GitHub search syntax).
fn qualifier(value: &str) -> String {
    let value = value.replace('"', "");
    if value.contains(char::is_whitespace) {
        format!("\"{value}\"")
    } else {
        value
    }
}

async fn collect_pages<T, F, Fut>(mut fetch: F) -> Result<(Vec<T>, bool), ProviderError>
where
    F: FnMut(u32) -> Fut,
    Fut: std::future::Future<Output = Result<(Vec<T>, Option<u32>), ProviderError>>,
{
    let mut out = Vec::new();
    let mut page = 1u32;
    for _ in 0..MAX_PAGES {
        let (items, next) = fetch(page).await?;
        out.extend(items);
        match next {
            Some(n) if n > page => page = n,
            _ => return Ok((out, false)),
        }
    }
    Ok((out, true))
}

// ---------------------------------------------------------------------------------------------
// GitHub.

#[derive(Deserialize)]
struct GhUser {
    login: String,
    avatar_url: Option<String>,
    name: Option<String>,
}

impl From<GhUser> for IssueUser {
    fn from(u: GhUser) -> Self {
        Self {
            username: u.login,
            display_name: u.name.filter(|n| !n.trim().is_empty()),
            avatar_url: u.avatar_url,
        }
    }
}

#[derive(Deserialize)]
struct GhLabel {
    name: String,
    color: Option<String>,
    description: Option<String>,
}

impl From<GhLabel> for IssueLabel {
    fn from(l: GhLabel) -> Self {
        Self {
            color: label_color(l.color.as_deref()),
            description: l.description.filter(|d| !d.trim().is_empty()),
            name: l.name,
        }
    }
}

#[derive(Deserialize)]
struct GhIssue {
    number: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    state: String,
    user: Option<GhUser>,
    #[serde(default)]
    labels: Vec<GhLabel>,
    #[serde(default)]
    assignees: Vec<GhUser>,
    #[serde(default)]
    comments: u32,
    created_at: Option<String>,
    updated_at: Option<String>,
    closed_at: Option<String>,
    closed_by: Option<GhUser>,
    #[serde(default)]
    html_url: String,
    body: Option<String>,
    #[serde(default)]
    locked: bool,
    pull_request: Option<serde_json::Value>,
}

impl GhIssue {
    fn summary(&mut self) -> IssueSummary {
        IssueSummary {
            number: self.number,
            title: std::mem::take(&mut self.title),
            state: if self.state == "closed" {
                IssueState::Closed
            } else {
                IssueState::Open
            },
            author: self.user.take().map(Into::into),
            labels: std::mem::take(&mut self.labels)
                .into_iter()
                .map(Into::into)
                .collect(),
            assignees: std::mem::take(&mut self.assignees)
                .into_iter()
                .map(Into::into)
                .collect(),
            comments: self.comments,
            created_at: self.created_at.as_deref().and_then(parse_timestamp),
            updated_at: self.updated_at.as_deref().and_then(parse_timestamp),
            web_url: std::mem::take(&mut self.html_url),
            is_pull_request: self.pull_request.is_some(),
        }
    }

    fn detail(mut self) -> IssueDetail {
        IssueDetail {
            issue: self.summary(),
            body: self.body.unwrap_or_default(),
            closed_at: self.closed_at.as_deref().and_then(parse_timestamp),
            closed_by: self.closed_by.map(Into::into),
            locked: self.locked,
        }
    }
}

#[derive(Deserialize)]
struct GhSearch<T> {
    total_count: u64,
    items: Vec<T>,
}

#[derive(Deserialize)]
struct GhComment {
    id: u64,
    user: Option<GhUser>,
    body: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    html_url: Option<String>,
}

impl From<GhComment> for IssueComment {
    fn from(c: GhComment) -> Self {
        Self {
            id: c.id.to_string(),
            author: c.user.map(Into::into),
            body: c.body.unwrap_or_default(),
            created_at: c.created_at.as_deref().and_then(parse_timestamp),
            updated_at: c.updated_at.as_deref().and_then(parse_timestamp),
            web_url: c.html_url,
            system: false,
        }
    }
}

impl GithubClient {
    pub async fn issue_list(
        &self,
        full_name: &str,
        query: &IssueListQuery,
    ) -> Result<IssuePage, ProviderError> {
        let [owner, repo] = github_repo(full_name)?;
        let (page, per_page) = page_args(query.page, query.per_page);
        let labels = clean_list(&query.labels);
        if let Some(text) = non_empty(query.search.as_deref()) {
            let mut q = vec![format!("repo:{owner}/{repo}"), "is:issue".to_owned()];
            match query.state {
                IssueStateFilter::Open => q.push("is:open".into()),
                IssueStateFilter::Closed => q.push("is:closed".into()),
                IssueStateFilter::All => {}
            }
            for l in &labels {
                q.push(format!("label:{}", qualifier(l)));
            }
            match non_empty(query.assignee.as_deref()) {
                Some(ASSIGNEE_NONE) => q.push("no:assignee".into()),
                Some(a) => q.push(format!("assignee:{}", qualifier(a))),
                None => {}
            }
            if let Some(a) = non_empty(query.author.as_deref()) {
                q.push(format!("author:{}", qualifier(a)));
            }
            q.push(text.to_owned());
            let q = q.join(" ");
            let resp = self
                .api()
                .get_json::<GhSearch<GhIssue>>(
                    &["search", "issues"],
                    &[
                        ("q", q.as_str()),
                        ("sort", "updated"),
                        ("per_page", &per_page),
                        ("page", &page),
                    ],
                )
                .await?;
            let next_page = resp.next_page();
            return Ok(IssuePage {
                total: Some(resp.body.total_count),
                items: resp
                    .body
                    .items
                    .into_iter()
                    .filter(|i| i.pull_request.is_none())
                    .map(|mut i| i.summary())
                    .collect(),
                next_page,
            });
        }
        let labels = labels.join(",");
        let mut q: Vec<(&str, &str)> = vec![
            (
                "state",
                match query.state {
                    IssueStateFilter::Open => "open",
                    IssueStateFilter::Closed => "closed",
                    IssueStateFilter::All => "all",
                },
            ),
            ("sort", "updated"),
            ("direction", "desc"),
            ("per_page", &per_page),
            ("page", &page),
        ];
        if !labels.is_empty() {
            q.push(("labels", &labels));
        }
        match non_empty(query.assignee.as_deref()) {
            Some(ASSIGNEE_NONE) => q.push(("assignee", "none")),
            Some(a) => q.push(("assignee", a)),
            None => {}
        }
        if let Some(a) = non_empty(query.author.as_deref()) {
            q.push(("creator", a));
        }
        let resp = self
            .api()
            .get_json::<Vec<GhIssue>>(&["repos", owner, repo, "issues"], &q)
            .await?;
        let next_page = resp.next_page();
        Ok(IssuePage {
            items: resp
                .body
                .into_iter()
                .filter(|i| i.pull_request.is_none())
                .map(|mut i| i.summary())
                .collect(),
            next_page,
            total: None,
        })
    }

    pub async fn issue(&self, full_name: &str, number: u64) -> Result<IssueDetail, ProviderError> {
        let [owner, repo] = github_repo(full_name)?;
        let n = number_s(number)?;
        let resp = self
            .api()
            .get_json::<GhIssue>(&["repos", owner, repo, "issues", &n], &[])
            .await?;
        Ok(resp.body.detail())
    }

    pub async fn issue_comments(
        &self,
        full_name: &str,
        number: u64,
    ) -> Result<IssueCommentList, ProviderError> {
        let [owner, repo] = github_repo(full_name)?;
        let n = number_s(number)?;
        let api = self.api();
        let (comments, truncated) = collect_pages(|page| {
            let page = page.to_string();
            let n = n.clone();
            async move {
                let resp = api
                    .get_json::<Vec<GhComment>>(
                        &["repos", owner, repo, "issues", &n, "comments"],
                        &[("per_page", "100"), ("page", &page)],
                    )
                    .await?;
                let next = resp.next_page();
                Ok((resp.body.into_iter().map(Into::into).collect(), next))
            }
        })
        .await?;
        Ok(IssueCommentList {
            comments,
            truncated,
        })
    }

    pub async fn issue_comment_create(
        &self,
        full_name: &str,
        number: u64,
        body: &str,
    ) -> Result<IssueComment, ProviderError> {
        let [owner, repo] = github_repo(full_name)?;
        let n = number_s(number)?;
        let resp = self
            .api()
            .send_json::<GhComment, _>(
                Method::POST,
                &["repos", owner, repo, "issues", &n, "comments"],
                &serde_json::json!({ "body": body }),
            )
            .await?;
        Ok(resp.body.into())
    }

    pub async fn issue_create(
        &self,
        full_name: &str,
        input: &IssueCreate,
    ) -> Result<IssueDetail, ProviderError> {
        let [owner, repo] = github_repo(full_name)?;
        let body = serde_json::json!({
            "title": input.title.trim(),
            "body": input.body,
            "labels": clean_list(&input.labels),
            "assignees": clean_list(&input.assignees),
        });
        let resp = self
            .api()
            .send_json::<GhIssue, _>(Method::POST, &["repos", owner, repo, "issues"], &body)
            .await?;
        Ok(resp.body.detail())
    }

    pub async fn issue_update(
        &self,
        full_name: &str,
        number: u64,
        change: &IssueUpdate,
    ) -> Result<IssueDetail, ProviderError> {
        let [owner, repo] = github_repo(full_name)?;
        let n = number_s(number)?;
        let mut body = serde_json::Map::new();
        if let Some(state) = change.state {
            let state = match state {
                IssueState::Open => "open",
                IssueState::Closed => "closed",
            };
            body.insert("state".into(), state.into());
        }
        if let Some(labels) = &change.labels {
            body.insert("labels".into(), clean_list(labels).into());
        }
        if let Some(assignees) = &change.assignees {
            body.insert("assignees".into(), clean_list(assignees).into());
        }
        let resp = self
            .api()
            .send_json::<GhIssue, _>(
                Method::PATCH,
                &["repos", owner, repo, "issues", &n],
                &serde_json::Value::Object(body),
            )
            .await?;
        Ok(resp.body.detail())
    }

    pub async fn issue_labels(&self, full_name: &str) -> Result<Vec<IssueLabel>, ProviderError> {
        let [owner, repo] = github_repo(full_name)?;
        let api = self.api();
        let (labels, _) = collect_pages(|page| {
            let page = page.to_string();
            async move {
                let resp = api
                    .get_json::<Vec<GhLabel>>(
                        &["repos", owner, repo, "labels"],
                        &[("per_page", "100"), ("page", &page)],
                    )
                    .await?;
                let next = resp.next_page();
                Ok((resp.body.into_iter().map(Into::into).collect(), next))
            }
        })
        .await?;
        Ok(labels)
    }

    pub async fn issue_assignable_users(
        &self,
        full_name: &str,
    ) -> Result<Vec<IssueUser>, ProviderError> {
        let [owner, repo] = github_repo(full_name)?;
        let api = self.api();
        let (users, _) = collect_pages(|page| {
            let page = page.to_string();
            async move {
                let resp = api
                    .get_json::<Vec<GhUser>>(
                        &["repos", owner, repo, "assignees"],
                        &[("per_page", "100"), ("page", &page)],
                    )
                    .await?;
                let next = resp.next_page();
                Ok((resp.body.into_iter().map(Into::into).collect(), next))
            }
        })
        .await?;
        Ok(users)
    }
}

// ---------------------------------------------------------------------------------------------
// GitLab.

#[derive(Deserialize)]
struct GlUser {
    id: Option<u64>,
    username: String,
    name: Option<String>,
    avatar_url: Option<String>,
    state: Option<String>,
}

impl From<GlUser> for IssueUser {
    fn from(u: GlUser) -> Self {
        Self {
            username: u.username,
            display_name: u.name.filter(|n| !n.trim().is_empty()),
            avatar_url: u.avatar_url,
        }
    }
}

/// Labels come as plain names, or as objects with `with_labels_details=true`.
#[derive(Deserialize)]
#[serde(untagged)]
enum GlLabel {
    Name(String),
    Full {
        name: String,
        color: Option<String>,
        description: Option<String>,
    },
}

impl From<GlLabel> for IssueLabel {
    fn from(l: GlLabel) -> Self {
        match l {
            GlLabel::Name(name) => Self {
                name,
                color: None,
                description: None,
            },
            GlLabel::Full {
                name,
                color,
                description,
            } => Self {
                name,
                color: label_color(color.as_deref()),
                description: description.filter(|d| !d.trim().is_empty()),
            },
        }
    }
}

#[derive(Deserialize)]
struct GlIssue {
    iid: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    state: String,
    author: Option<GlUser>,
    #[serde(default)]
    labels: Vec<GlLabel>,
    #[serde(default)]
    assignees: Vec<GlUser>,
    #[serde(default)]
    user_notes_count: u32,
    created_at: Option<String>,
    updated_at: Option<String>,
    closed_at: Option<String>,
    closed_by: Option<GlUser>,
    #[serde(default)]
    web_url: String,
    description: Option<String>,
    #[serde(default)]
    discussion_locked: Option<bool>,
}

impl GlIssue {
    fn summary(&mut self) -> IssueSummary {
        IssueSummary {
            number: self.iid,
            title: std::mem::take(&mut self.title),
            state: if self.state == "closed" {
                IssueState::Closed
            } else {
                IssueState::Open
            },
            author: self.author.take().map(Into::into),
            labels: std::mem::take(&mut self.labels)
                .into_iter()
                .map(Into::into)
                .collect(),
            assignees: std::mem::take(&mut self.assignees)
                .into_iter()
                .map(Into::into)
                .collect(),
            comments: self.user_notes_count,
            created_at: self.created_at.as_deref().and_then(parse_timestamp),
            updated_at: self.updated_at.as_deref().and_then(parse_timestamp),
            web_url: std::mem::take(&mut self.web_url),
            is_pull_request: false,
        }
    }

    fn detail(mut self) -> IssueDetail {
        IssueDetail {
            issue: self.summary(),
            body: self.description.unwrap_or_default(),
            closed_at: self.closed_at.as_deref().and_then(parse_timestamp),
            closed_by: self.closed_by.map(Into::into),
            locked: self.discussion_locked.unwrap_or(false),
        }
    }
}

#[derive(Deserialize)]
struct GlNote {
    id: u64,
    author: Option<GlUser>,
    body: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    #[serde(default)]
    system: bool,
}

impl From<GlNote> for IssueComment {
    fn from(n: GlNote) -> Self {
        Self {
            id: n.id.to_string(),
            author: n.author.map(Into::into),
            body: n.body.unwrap_or_default(),
            created_at: n.created_at.as_deref().and_then(parse_timestamp),
            updated_at: n.updated_at.as_deref().and_then(parse_timestamp),
            web_url: None,
            system: n.system,
        }
    }
}

impl GitlabClient {
    pub async fn issue_list(
        &self,
        full_name: &str,
        query: &IssueListQuery,
    ) -> Result<IssuePage, ProviderError> {
        let project = gitlab_project(full_name)?;
        let (page, per_page) = page_args(query.page, query.per_page);
        let labels = clean_list(&query.labels).join(",");
        let mut q: Vec<(&str, &str)> = vec![
            ("with_labels_details", "true"),
            ("order_by", "updated_at"),
            ("sort", "desc"),
            ("per_page", &per_page),
            ("page", &page),
        ];
        match query.state {
            IssueStateFilter::Open => q.push(("state", "opened")),
            IssueStateFilter::Closed => q.push(("state", "closed")),
            IssueStateFilter::All => {}
        }
        if !labels.is_empty() {
            q.push(("labels", &labels));
        }
        match non_empty(query.assignee.as_deref()) {
            Some(ASSIGNEE_NONE) => q.push(("assignee_id", "None")),
            Some(a) => q.push(("assignee_username", a)),
            None => {}
        }
        if let Some(a) = non_empty(query.author.as_deref()) {
            q.push(("author_username", a));
        }
        if let Some(text) = non_empty(query.search.as_deref()) {
            q.push(("search", text));
        }
        let resp = self
            .api()
            .get_json::<Vec<GlIssue>>(&["projects", project, "issues"], &q)
            .await?;
        let next_page = resp.next_page();
        let total = resp.header("x-total").and_then(|t| t.parse().ok());
        Ok(IssuePage {
            items: resp.body.into_iter().map(|mut i| i.summary()).collect(),
            next_page,
            total,
        })
    }

    pub async fn issue(&self, full_name: &str, number: u64) -> Result<IssueDetail, ProviderError> {
        let project = gitlab_project(full_name)?;
        let n = number_s(number)?;
        let resp = self
            .api()
            .get_json::<GlIssue>(
                &["projects", project, "issues", &n],
                &[("with_labels_details", "true")],
            )
            .await?;
        Ok(resp.body.detail())
    }

    pub async fn issue_comments(
        &self,
        full_name: &str,
        number: u64,
    ) -> Result<IssueCommentList, ProviderError> {
        let project = gitlab_project(full_name)?;
        let n = number_s(number)?;
        let api = self.api();
        let (comments, truncated) = collect_pages(|page| {
            let page = page.to_string();
            let n = n.clone();
            async move {
                let resp = api
                    .get_json::<Vec<GlNote>>(
                        &["projects", project, "issues", &n, "notes"],
                        &[
                            ("sort", "asc"),
                            ("order_by", "created_at"),
                            ("per_page", "100"),
                            ("page", &page),
                        ],
                    )
                    .await?;
                let next = resp.next_page();
                Ok((resp.body.into_iter().map(Into::into).collect(), next))
            }
        })
        .await?;
        Ok(IssueCommentList {
            comments,
            truncated,
        })
    }

    pub async fn issue_comment_create(
        &self,
        full_name: &str,
        number: u64,
        body: &str,
    ) -> Result<IssueComment, ProviderError> {
        let project = gitlab_project(full_name)?;
        let n = number_s(number)?;
        let resp = self
            .api()
            .send_json::<GlNote, _>(
                Method::POST,
                &["projects", project, "issues", &n, "notes"],
                &serde_json::json!({ "body": body }),
            )
            .await?;
        Ok(resp.body.into())
    }

    /// GitLab assigns by user id: usernames are looked up with `GET /users?username=`.
    async fn user_ids(&self, usernames: &[String]) -> Result<Vec<u64>, ProviderError> {
        let mut ids = Vec::new();
        for name in clean_list(usernames) {
            let users = self
                .api()
                .get_json::<Vec<GlUser>>(&["users"], &[("username", name.as_str())])
                .await?
                .body;
            let id = users
                .into_iter()
                .find(|u| u.username.eq_ignore_ascii_case(&name))
                .and_then(|u| u.id)
                .ok_or_else(|| ProviderError::Rejected {
                    status: 404,
                    message: format!("user `{name}` not found"),
                })?;
            ids.push(id);
        }
        Ok(ids)
    }

    pub async fn issue_create(
        &self,
        full_name: &str,
        input: &IssueCreate,
    ) -> Result<IssueDetail, ProviderError> {
        let project = gitlab_project(full_name)?;
        let mut body = serde_json::json!({
            "title": input.title.trim(),
            "description": input.body,
            "labels": clean_list(&input.labels).join(","),
        });
        let ids = self.user_ids(&input.assignees).await?;
        if !ids.is_empty() {
            body["assignee_ids"] = ids.into();
        }
        let resp = self
            .api()
            .send_json::<GlIssue, _>(Method::POST, &["projects", project, "issues"], &body)
            .await?;
        Ok(resp.body.detail())
    }

    pub async fn issue_update(
        &self,
        full_name: &str,
        number: u64,
        change: &IssueUpdate,
    ) -> Result<IssueDetail, ProviderError> {
        let project = gitlab_project(full_name)?;
        let n = number_s(number)?;
        let mut body = serde_json::Map::new();
        if let Some(state) = change.state {
            let event = match state {
                IssueState::Open => "reopen",
                IssueState::Closed => "close",
            };
            body.insert("state_event".into(), event.into());
        }
        if let Some(labels) = &change.labels {
            body.insert("labels".into(), clean_list(labels).join(",").into());
        }
        if let Some(assignees) = &change.assignees {
            let ids = self.user_ids(assignees).await?;
            // `0` unassigns everyone (documented; an empty array is not accepted everywhere).
            let ids = if ids.is_empty() { vec![0] } else { ids };
            body.insert("assignee_ids".into(), ids.into());
        }
        let resp = self
            .api()
            .send_json::<GlIssue, _>(
                Method::PUT,
                &["projects", project, "issues", &n],
                &serde_json::Value::Object(body),
            )
            .await?;
        Ok(resp.body.detail())
    }

    pub async fn issue_labels(&self, full_name: &str) -> Result<Vec<IssueLabel>, ProviderError> {
        let project = gitlab_project(full_name)?;
        let api = self.api();
        let (labels, _) = collect_pages(|page| {
            let page = page.to_string();
            async move {
                let resp = api
                    .get_json::<Vec<GlLabel>>(
                        &["projects", project, "labels"],
                        &[("per_page", "100"), ("page", &page)],
                    )
                    .await?;
                let next = resp.next_page();
                Ok((resp.body.into_iter().map(Into::into).collect(), next))
            }
        })
        .await?;
        Ok(labels)
    }

    /// Active project members, including inherited group members.
    pub async fn issue_assignable_users(
        &self,
        full_name: &str,
    ) -> Result<Vec<IssueUser>, ProviderError> {
        let project = gitlab_project(full_name)?;
        let api = self.api();
        let (users, _) = collect_pages(|page| {
            let page = page.to_string();
            async move {
                let resp = api
                    .get_json::<Vec<GlUser>>(
                        &["projects", project, "members", "all"],
                        &[("per_page", "100"), ("page", &page)],
                    )
                    .await?;
                let next = resp.next_page();
                let users = resp
                    .body
                    .into_iter()
                    .filter(|u| u.state.as_deref().is_none_or(|s| s == "active"))
                    .map(IssueUser::from)
                    .collect::<Vec<_>>();
                Ok((users, next))
            }
        })
        .await?;
        let mut out: Vec<IssueUser> = Vec::new();
        for u in users {
            if !out.iter().any(|o| o.username == u.username) {
                out.push(u);
            }
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------------------------
// Provider-neutral entry points.

macro_rules! delegate {
    ($self:ident, $c:ident => $e:expr) => {
        match $self {
            ProviderClient::Github($c) => $e,
            ProviderClient::Gitlab($c) => $e,
        }
    };
}

impl ProviderClient {
    /// One page of issues of `full_name` (pull requests excluded), most recently updated first.
    pub async fn issue_list(
        &self,
        full_name: &str,
        query: &IssueListQuery,
    ) -> Result<IssuePage, ProviderError> {
        delegate!(self, c => c.issue_list(full_name, query).await)
    }

    /// One issue. On GitHub the number may be a pull request (`issue.is_pull_request`).
    pub async fn issue(&self, full_name: &str, number: u64) -> Result<IssueDetail, ProviderError> {
        delegate!(self, c => c.issue(full_name, number).await)
    }

    /// Comments (GitLab: notes, including system notes) oldest first.
    pub async fn issue_comments(
        &self,
        full_name: &str,
        number: u64,
    ) -> Result<IssueCommentList, ProviderError> {
        delegate!(self, c => c.issue_comments(full_name, number).await)
    }

    pub async fn issue_comment_create(
        &self,
        full_name: &str,
        number: u64,
        body: &str,
    ) -> Result<IssueComment, ProviderError> {
        if body.trim().is_empty() {
            return Err(invalid("the comment is empty".into()));
        }
        delegate!(self, c => c.issue_comment_create(full_name, number, body).await)
    }

    pub async fn issue_create(
        &self,
        full_name: &str,
        input: &IssueCreate,
    ) -> Result<IssueDetail, ProviderError> {
        if input.title.trim().is_empty() {
            return Err(invalid("the issue title is empty".into()));
        }
        delegate!(self, c => c.issue_create(full_name, input).await)
    }

    /// Close / reopen and replace labels or assignees.
    pub async fn issue_update(
        &self,
        full_name: &str,
        number: u64,
        change: &IssueUpdate,
    ) -> Result<IssueDetail, ProviderError> {
        delegate!(self, c => c.issue_update(full_name, number, change).await)
    }

    pub async fn issue_labels(&self, full_name: &str) -> Result<Vec<IssueLabel>, ProviderError> {
        delegate!(self, c => c.issue_labels(full_name).await)
    }

    pub async fn issue_assignable_users(
        &self,
        full_name: &str,
    ) -> Result<Vec<IssueUser>, ProviderError> {
        delegate!(self, c => c.issue_assignable_users(full_name).await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_colors_and_qualifiers() {
        assert_eq!(github_repo("o/r.git").unwrap(), ["o", "r"]);
        assert!(github_repo("o/r/x").is_err());
        assert!(github_repo("o").is_err());
        assert_eq!(gitlab_project("/g/s/p.git").unwrap(), "g/s/p");
        assert!(gitlab_project("p").is_err());
        assert!(gitlab_project("g//p").is_err());
        assert_eq!(label_color(Some("F29513")).as_deref(), Some("#f29513"));
        assert_eq!(label_color(Some("#d9534f")).as_deref(), Some("#d9534f"));
        assert_eq!(label_color(Some("red;}")), None);
        assert_eq!(qualifier("good first issue"), "\"good first issue\"");
        assert_eq!(qualifier("bug\""), "bug");
        assert_eq!(
            clean_list(&[" a ".into(), "".into(), "a".into(), "b".into()]),
            ["a", "b"]
        );
        assert_eq!(page_args(0, 0), ("1".into(), "30".into()));
        assert_eq!(page_args(3, 200), ("3".into(), "100".into()));
    }
}
