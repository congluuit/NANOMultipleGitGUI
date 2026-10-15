//! "My work" of one account across every project it can see (Phase 4.2): pull / merge
//! requests waiting for its review, the ones it opened, and issues assigned to it.
//!
//! GitHub: `search/issues` with `@me` qualifiers (3 searches; the search API allows 30
//! requests per minute). GitLab: the instance-wide `merge_requests` / `issues` listings.

use serde::{Deserialize, Serialize};

use crate::ProviderClient;
use crate::error::ProviderError;
use crate::github::GithubClient;
use crate::gitlab::GitlabClient;
use crate::timefmt::parse_timestamp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum WorkItemKind {
    /// GitHub pull request / GitLab merge request.
    PullRequest,
    Issue,
}

/// One row of the "My work" panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct WorkItem {
    pub kind: WorkItemKind,
    /// `owner/repo` or `group/sub/project`.
    pub full_name: String,
    /// GitHub number / GitLab `iid`.
    pub number: u64,
    pub title: String,
    pub author: Option<String>,
    pub author_avatar: Option<String>,
    pub updated_at: Option<i64>,
    /// Draft pull / merge request.
    pub draft: bool,
    pub labels: Vec<String>,
    pub comments: Option<u32>,
    pub web_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct WorkList {
    pub items: Vec<WorkItem>,
    /// All matches (may exceed `items`) when the provider reports it.
    pub total: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct WorkLists {
    /// Open PRs/MRs where the account is a requested reviewer.
    pub review_requested: WorkList,
    /// Open PRs/MRs the account opened.
    pub authored: WorkList,
    /// Open issues assigned to the account.
    pub assigned: WorkList,
}

/// `.../repos/{owner}/{repo}` (GitHub `repository_url`) → `owner/repo`.
pub(crate) fn full_name_from_repo_url(url: &str) -> Option<String> {
    let (_, rest) = url.rsplit_once("/repos/")?;
    let mut parts = rest.trim_end_matches('/').split('/');
    let (owner, repo) = (parts.next()?, parts.next()?);
    (!owner.is_empty() && !repo.is_empty() && parts.next().is_none())
        .then(|| format!("{owner}/{repo}"))
}

/// GitLab `references.full` (`grp/sub/proj!12`, `grp/proj#3`) or the web URL
/// (`https://gl/grp/proj/-/merge_requests/12`) → `grp/sub/proj`.
pub(crate) fn gitlab_path(reference: Option<&str>, web_url: &str) -> Option<String> {
    if let Some(r) = reference
        && let Some(i) = r.rfind(['!', '#'])
        && r[..i].contains('/')
    {
        return Some(r[..i].to_owned());
    }
    let (before, _) = web_url.split_once("/-/")?;
    let url = url::Url::parse(before).ok()?;
    let path = url.path().trim_matches('/');
    path.contains('/').then(|| path.to_owned())
}

#[derive(Deserialize)]
struct GhUser {
    login: String,
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct GhLabel {
    name: String,
}

#[derive(Deserialize)]
struct GhItem {
    number: u64,
    #[serde(default)]
    title: String,
    user: Option<GhUser>,
    updated_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    labels: Vec<GhLabel>,
    comments: Option<u32>,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    repository_url: String,
    pull_request: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct GhSearch {
    total_count: u64,
    items: Vec<GhItem>,
}

impl GithubClient {
    async fn work_search(&self, q: &str, per_page: &str) -> Result<WorkList, ProviderError> {
        let resp = self
            .api()
            .get_json::<GhSearch>(
                &["search", "issues"],
                &[
                    ("q", q),
                    ("sort", "updated"),
                    ("order", "desc"),
                    ("per_page", per_page),
                ],
            )
            .await?;
        Ok(WorkList {
            total: Some(resp.body.total_count),
            items: resp
                .body
                .items
                .into_iter()
                .filter_map(|i| {
                    Some(WorkItem {
                        kind: if i.pull_request.is_some() {
                            WorkItemKind::PullRequest
                        } else {
                            WorkItemKind::Issue
                        },
                        full_name: full_name_from_repo_url(&i.repository_url)?,
                        number: i.number,
                        title: i.title,
                        author_avatar: i.user.as_ref().and_then(|u| u.avatar_url.clone()),
                        author: i.user.map(|u| u.login),
                        updated_at: i.updated_at.as_deref().and_then(parse_timestamp),
                        draft: i.draft,
                        labels: i.labels.into_iter().map(|l| l.name).collect(),
                        comments: i.comments,
                        web_url: i.html_url,
                    })
                })
                .collect(),
        })
    }

    pub async fn my_work(&self, per_list: u8) -> Result<WorkLists, ProviderError> {
        let per_page = per_list.clamp(1, 100).to_string();
        let (review, authored, assigned) = tokio::join!(
            self.work_search(
                "is:pr is:open review-requested:@me archived:false",
                &per_page
            ),
            self.work_search("is:pr is:open author:@me archived:false", &per_page),
            self.work_search("is:issue is:open assignee:@me archived:false", &per_page),
        );
        Ok(WorkLists {
            review_requested: review?,
            authored: authored?,
            assigned: assigned?,
        })
    }
}

#[derive(Deserialize)]
struct GlUser {
    username: String,
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct GlReferences {
    full: Option<String>,
}

#[derive(Deserialize)]
struct GlItem {
    iid: u64,
    #[serde(default)]
    title: String,
    author: Option<GlUser>,
    updated_at: Option<String>,
    #[serde(default)]
    draft: Option<bool>,
    #[serde(default)]
    work_in_progress: Option<bool>,
    #[serde(default)]
    labels: Vec<serde_json::Value>,
    user_notes_count: Option<u32>,
    #[serde(default)]
    web_url: String,
    references: Option<GlReferences>,
}

impl GlItem {
    fn into_work(self, kind: WorkItemKind) -> Option<WorkItem> {
        let full_name = gitlab_path(
            self.references.as_ref().and_then(|r| r.full.as_deref()),
            &self.web_url,
        )?;
        Some(WorkItem {
            kind,
            full_name,
            number: self.iid,
            title: self.title,
            author_avatar: self.author.as_ref().and_then(|u| u.avatar_url.clone()),
            author: self.author.map(|u| u.username),
            updated_at: self.updated_at.as_deref().and_then(parse_timestamp),
            draft: self.draft.or(self.work_in_progress).unwrap_or(false),
            labels: self
                .labels
                .into_iter()
                .filter_map(|l| match l {
                    serde_json::Value::String(s) => Some(s),
                    serde_json::Value::Object(mut o) => match o.remove("name") {
                        Some(serde_json::Value::String(s)) => Some(s),
                        _ => None,
                    },
                    _ => None,
                })
                .collect(),
            comments: self.user_notes_count,
            web_url: self.web_url,
        })
    }
}

impl GitlabClient {
    async fn work_list(
        &self,
        resource: &str,
        query: &[(&str, &str)],
        kind: WorkItemKind,
    ) -> Result<WorkList, ProviderError> {
        let resp = self
            .api()
            .get_json::<Vec<GlItem>>(&[resource], query)
            .await?;
        let total = resp.header("x-total").and_then(|t| t.parse().ok());
        Ok(WorkList {
            items: resp
                .body
                .into_iter()
                .filter_map(|i| i.into_work(kind))
                .collect(),
            total,
        })
    }

    pub async fn my_work(&self, per_list: u8) -> Result<WorkLists, ProviderError> {
        #[derive(Deserialize)]
        struct Me {
            username: String,
        }
        let me = self.api().get_json::<Me>(&["user"], &[]).await?.body;
        let per_page = per_list.clamp(1, 100).to_string();
        let common = |extra: [(&'static str, &'static str); 1]| {
            let mut q: Vec<(&str, &str)> = vec![
                ("state", "opened"),
                ("order_by", "updated_at"),
                ("sort", "desc"),
                ("per_page", per_page.as_str()),
            ];
            q.extend(extra);
            q
        };
        let mut review_q = common([("scope", "all")]);
        review_q.push(("reviewer_username", me.username.as_str()));
        let authored_q = common([("scope", "created_by_me")]);
        let assigned_q = common([("scope", "assigned_to_me")]);
        let (review, authored, assigned) = tokio::join!(
            self.work_list("merge_requests", &review_q, WorkItemKind::PullRequest),
            self.work_list("merge_requests", &authored_q, WorkItemKind::PullRequest),
            self.work_list("issues", &assigned_q, WorkItemKind::Issue),
        );
        Ok(WorkLists {
            review_requested: review?,
            authored: authored?,
            assigned: assigned?,
        })
    }
}

impl ProviderClient {
    /// The three "My work" lists of the account (open items only, most recently updated first,
    /// at most `per_list` each).
    pub async fn my_work(&self, per_list: u8) -> Result<WorkLists, ProviderError> {
        match self {
            Self::Github(c) => c.my_work(per_list).await,
            Self::Gitlab(c) => c.my_work(per_list).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_names_from_urls() {
        assert_eq!(
            full_name_from_repo_url("https://api.github.com/repos/acme/game").as_deref(),
            Some("acme/game")
        );
        assert_eq!(
            full_name_from_repo_url("https://ghe.acme.dev/api/v3/repos/a/b/").as_deref(),
            Some("a/b")
        );
        assert_eq!(full_name_from_repo_url("https://x/repos/a/b/c"), None);
        assert_eq!(full_name_from_repo_url("https://x/users/a"), None);
        assert_eq!(
            gitlab_path(Some("grp/sub/proj!12"), "").as_deref(),
            Some("grp/sub/proj")
        );
        assert_eq!(
            gitlab_path(Some("grp/proj#3"), "").as_deref(),
            Some("grp/proj")
        );
        assert_eq!(
            gitlab_path(Some("!12"), "https://gl.acme.dev/g/p/-/merge_requests/12").as_deref(),
            Some("g/p")
        );
        assert_eq!(gitlab_path(None, "https://gl/p/-/issues/1"), None);
    }
}
