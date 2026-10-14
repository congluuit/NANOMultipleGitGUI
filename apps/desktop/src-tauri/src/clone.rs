//! Multi-account clone (PLAN §4.7, §6.5): the unified repository list of every account, account
//! guessing for a pasted URL, the default destination folder, and the clone itself with
//! progress events, cancellation, the `source = clone` binding and opening the new tab.
//!
//! - [`clone_repo_list`]: repositories of every (or the selected) account, fetched in parallel
//!   and cached in memory. A stale cache is returned at once and refreshed in the background
//!   ([`CloneListUpdated`] tells the frontend to re-read it). Each account carries its own
//!   error, so one failing account never hides the others. Searching/filtering is client-side.
//! - [`clone_guess`]: parses a URL, then lists candidate accounts — rule-engine matches first,
//!   then every account on that host — plus the HTTPS / SSH form of the URL.
//! - [`clone_destination`]: `{clone_dir}/{owner}/{repo}` (template setting
//!   [`KEY_PATH_TEMPLATE`]) under the account's `defaultCloneDir` (fallback `~/Source`), the
//!   free space of that volume, and the repository / LFS size when the provider reported it.
//! - [`clone_start`]: `git clone` as the chosen account ([`crate::accounts::context_for_account`]),
//!   [`CloneProgress`] events, cancel with `transfer_cancel(opId)`. Afterwards: the repo binding
//!   `{source: clone}`, the identity copied to `.git/config` when asked, recent list + tab. For
//!   "download LFS later" the frontend starts `lfs_pull` (transfer queue) on the returned tab.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use nanomgg_accounts::url::hostname_of;
use nanomgg_accounts::{
    AccountDirectory as Directory, AccountSecret, BindingSource, MatchInput, RemoteUrl,
    RepoBinding, Transport, UrlScheme,
};
use nanomgg_git_engine::clone::{self, CloneOptions, CloneOutcome, Destination, LfsMode};
use nanomgg_git_engine::ops::{FailureKind, OpError};
use nanomgg_git_engine::progress::{CancelToken, Progress, ProgressPhase};
use nanomgg_git_engine::{AccountContext, repo};
use nanomgg_providers::{
    CancellationToken, HttpConfig, ListOptions, ProviderClient, ProviderEndpoint, ProviderError,
    Repository, SecretToken, build_http_client, list_all_repositories,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::accounts::commands::account_secret;
use crate::accounts::{Snapshot, context_for_account, storage_err};
use crate::branches::{Outcome, Registration};
use crate::error::{AppError, AppResult};
use crate::repos::RepoTab;
use crate::state::AppState;

/// Setting: destination template, relative to the account's clone folder.
pub const KEY_PATH_TEMPLATE: &str = "clone.pathTemplate";
pub const DEFAULT_TEMPLATE: &str = "{clone_dir}/{owner}/{repo}";
/// Cached repository lists older than this are refreshed in the background.
const LIST_TTL: Duration = Duration::from_secs(10 * 60);
/// Minimum time between two progress events of a clone.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

// ----- repository list -----------------------------------------------------------------------

/// A repository row of the clone browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CloneRepo {
    pub account_id: String,
    /// Provider id of the repository.
    pub id: String,
    /// `owner/name` or `group/sub/project`.
    pub full_name: String,
    pub owner: String,
    pub name: String,
    pub description: Option<String>,
    pub private: bool,
    pub fork: bool,
    /// `full_name` of the fork parent, when known.
    pub parent_full_name: Option<String>,
    pub archived: bool,
    pub default_branch: Option<String>,
    pub clone_https: String,
    pub clone_ssh: String,
    pub web_url: String,
    /// Unix seconds.
    pub updated_at: Option<i64>,
    pub stars: u32,
    pub size_bytes: Option<u64>,
    pub lfs_enabled: Option<bool>,
    pub lfs_size_bytes: Option<u64>,
    pub can_push: bool,
}

impl CloneRepo {
    fn new(account_id: &str, r: Repository) -> Self {
        Self {
            account_id: account_id.to_owned(),
            id: r.id,
            full_name: r.full_name,
            owner: r.owner,
            name: r.name,
            description: r.description,
            private: r.private,
            fork: r.fork,
            parent_full_name: r.parent_full_name,
            archived: r.archived,
            default_branch: r.default_branch,
            clone_https: r.clone_https,
            clone_ssh: r.clone_ssh,
            web_url: r.web_url,
            updated_at: r.updated_at,
            stars: r.stars,
            size_bytes: r.size_bytes,
            lfs_enabled: r.lfs_enabled,
            lfs_size_bytes: r.lfs_size_bytes,
            can_push: r.permissions.can_push,
        }
    }
}

/// Why an account's list could not be loaded. `code` is `noToken`, `noProvider` or a
/// provider error code (`unauthorized`, `forbidden`, `rateLimited`, `network`…).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListError {
    pub code: String,
    pub message: String,
}

impl ListError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: nanomgg_logging::redact(&message.into()).into_owned(),
        }
    }
}

impl From<ProviderError> for ListError {
    fn from(e: ProviderError) -> Self {
        Self::new(e.code(), e.to_string())
    }
}

/// One account's part of the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountRepos {
    pub account_id: String,
    pub repos: Vec<CloneRepo>,
    /// When the list was fetched (ms since the epoch); `None` = never loaded.
    pub fetched_at: Option<i64>,
    /// The last fetch failed (`repos` may still hold an older successful result).
    pub error: Option<ListError>,
    /// The provider had more repositories than the listing cap.
    pub truncated: bool,
    /// A background refresh is running; [`CloneListUpdated`] follows.
    pub refreshing: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CloneRepoList {
    pub accounts: Vec<AccountRepos>,
}

/// A background refresh finished: re-read [`clone_repo_list`].
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct CloneListUpdated {
    pub account_ids: Vec<String>,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    repos: Vec<CloneRepo>,
    fetched_at: i64,
    fetched: Instant,
    error: Option<ListError>,
    truncated: bool,
}

static CACHE: LazyLock<Mutex<HashMap<String, CacheEntry>>> = LazyLock::new(Mutex::default);
static IN_FLIGHT: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Mutex::default);

fn cache() -> MutexGuard<'static, HashMap<String, CacheEntry>> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner())
}

fn in_flight() -> MutexGuard<'static, HashSet<String>> {
    IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner())
}

/// Stores a fetch result; a failure keeps the previous repositories (with the error).
fn store(account_id: &str, result: Result<(Vec<CloneRepo>, bool), ListError>) {
    let mut cache = cache();
    let entry = match result {
        Ok((repos, truncated)) => CacheEntry {
            repos,
            fetched_at: now_ms(),
            fetched: Instant::now(),
            error: None,
            truncated,
        },
        Err(error) => {
            let previous = cache.remove(account_id);
            CacheEntry {
                repos: previous
                    .as_ref()
                    .map(|p| p.repos.clone())
                    .unwrap_or_default(),
                truncated: previous.as_ref().is_some_and(|p| p.truncated),
                fetched_at: now_ms(),
                fetched: Instant::now(),
                error: Some(error),
            }
        }
    };
    cache.insert(account_id.to_owned(), entry);
}

fn cached(account_id: &str) -> Option<CacheEntry> {
    cache().get(account_id).cloned()
}

fn provider_kind(kind: nanomgg_accounts::ProviderKind) -> nanomgg_providers::ProviderKind {
    use nanomgg_accounts::ProviderKind as A;
    use nanomgg_providers::ProviderKind as P;
    match kind {
        A::Github => P::Github,
        A::GithubEnterprise => P::GithubEnterprise,
        A::Gitlab => P::Gitlab,
        A::GitlabSelfHosted => P::GitlabSelfHosted,
    }
}

/// API client of an account (its provider endpoint + keychain token).
fn client_for(st: &AppState, account_id: &str) -> Result<ProviderClient, ListError> {
    let account = st
        .storage
        .account(account_id)
        .ok()
        .flatten()
        .ok_or_else(|| ListError::new("noProvider", format!("account `{account_id}` not found")))?;
    let provider = st
        .storage
        .provider(&account.provider_id)
        .ok()
        .flatten()
        .ok_or_else(|| ListError::new("noProvider", "the account's provider is missing"))?;
    let mut token = account_secret(st, account_id, AccountSecret::Token)
        .map_err(|e| ListError::new("noToken", e.to_string()))?
        .ok_or_else(|| ListError::new("noToken", "no token stored for this account"))?;
    let endpoint = ProviderEndpoint::custom(
        provider_kind(provider.kind),
        &provider.base_url,
        &provider.api_url,
    )?;
    let http = build_http_client(&HttpConfig::default())?;
    // Moved out of the zeroizing buffer, never copied.
    let token = SecretToken::new(std::mem::take(&mut *token));
    Ok(ProviderClient::new(http, endpoint, token))
}

/// Fetches every repository of one account and stores the result in the cache.
async fn fetch_account(st: &AppState, account_id: &str) {
    let result = match client_for(st, account_id) {
        Ok(client) => list_all_repositories(
            &client,
            &ListOptions::default(),
            &CancellationToken::new(),
            |_| {},
        )
        .await
        .map(|listing| {
            let repos = listing
                .repos
                .into_iter()
                .map(|r| CloneRepo::new(account_id, r))
                .collect();
            (repos, listing.truncated)
        })
        .map_err(ListError::from),
        Err(e) => Err(e),
    };
    if let Err(e) = &result {
        tracing::warn!(account = account_id, code = %e.code, "repository list failed");
    }
    store(account_id, result);
}

/// Fetches several accounts in parallel (one task each).
async fn fetch_accounts(app: &AppHandle, ids: &[String]) {
    let tasks: Vec<_> = ids
        .iter()
        .map(|id| {
            let app = app.clone();
            let id = id.clone();
            tauri::async_runtime::spawn(async move {
                fetch_account(&app.state::<AppState>(), &id).await;
            })
        })
        .collect();
    for task in tasks {
        if let Err(e) = task.await {
            tracing::warn!(error = %e, "repository list task failed");
        }
    }
}

/// Builds the response from the cache, in account order.
fn snapshot(ids: &[String], refreshing: &HashSet<String>) -> CloneRepoList {
    let cache = cache();
    CloneRepoList {
        accounts: ids
            .iter()
            .map(|id| match cache.get(id) {
                Some(e) => AccountRepos {
                    account_id: id.clone(),
                    repos: e.repos.clone(),
                    fetched_at: Some(e.fetched_at),
                    error: e.error.clone(),
                    truncated: e.truncated,
                    refreshing: refreshing.contains(id),
                },
                None => AccountRepos {
                    account_id: id.clone(),
                    repos: Vec::new(),
                    fetched_at: None,
                    error: None,
                    truncated: false,
                    refreshing: refreshing.contains(id),
                },
            })
            .collect(),
    }
}

/// Requested account ids that exist, in the user's account order.
fn account_ids(st: &AppState, wanted: Option<Vec<String>>) -> AppResult<Vec<String>> {
    let all = st.storage.accounts().map_err(storage_err)?;
    Ok(all
        .into_iter()
        .map(|a| a.id)
        .filter(|id| wanted.as_ref().is_none_or(|w| w.contains(id)))
        .collect())
}

/// Repositories of `accountIds` (default: every account). Missing lists are fetched now;
/// `refresh` refetches all of them; a stale cache is returned immediately and refreshed in
/// the background (then [`CloneListUpdated`] is emitted).
#[tauri::command]
#[specta::specta]
pub async fn clone_repo_list(
    app: AppHandle,
    account_ids: Option<Vec<String>>,
    refresh: bool,
) -> AppResult<CloneRepoList> {
    let ids = {
        let st = app.state::<AppState>();
        self::account_ids(&st, account_ids)?
    };
    let (now, stale): (Vec<String>, Vec<String>) = if refresh {
        (ids.clone(), Vec::new())
    } else {
        let missing = ids.iter().filter(|id| cached(id).is_none()).cloned();
        let stale = ids
            .iter()
            .filter(|id| cached(id).is_some_and(|e| e.fetched.elapsed() > LIST_TTL))
            .cloned();
        (missing.collect(), stale.collect())
    };
    if !now.is_empty() {
        fetch_accounts(&app, &now).await;
    }
    let background: Vec<String> = {
        let mut running = in_flight();
        stale
            .into_iter()
            .filter(|id| running.insert(id.clone()))
            .collect()
    };
    let refreshing: HashSet<String> = in_flight().clone();
    if !background.is_empty() {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            fetch_accounts(&app, &background).await;
            {
                let mut running = in_flight();
                for id in &background {
                    running.remove(id);
                }
            }
            let event = CloneListUpdated {
                account_ids: background,
            };
            if let Err(e) = event.emit(&app) {
                tracing::warn!(error = %e, "failed to emit clone list update");
            }
        });
    }
    Ok(snapshot(&ids, &refreshing))
}

// ----- URL guess -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GuessReason {
    /// A rule of the rule engine matches the URL.
    Rule,
    /// The account's provider is on the URL's host.
    Host,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CloneCandidate {
    pub account_id: String,
    /// Identity chosen by the matching rule.
    pub identity_id: Option<String>,
    pub reason: GuessReason,
    pub rule_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CloneGuess {
    /// Git can clone this (URL or local path).
    pub valid: bool,
    /// Local path or `file://` URL: no account needed.
    pub local: bool,
    pub host: Option<String>,
    /// Owner / group path (`acme/platform` for `acme/platform/api`).
    pub owner: Option<String>,
    pub repo: Option<String>,
    pub transport: Option<Transport>,
    /// The URL in HTTPS form (`None` for local paths).
    pub https_url: Option<String>,
    /// The URL in SSH form (`git@host:owner/repo.git`).
    pub ssh_url: Option<String>,
    /// Best first: rule matches, then accounts on the host. Empty = unknown host.
    pub candidates: Vec<CloneCandidate>,
}

fn transport_of(url: &RemoteUrl) -> Option<Transport> {
    match url.scheme {
        UrlScheme::Ssh => Some(Transport::Ssh),
        UrlScheme::Https | UrlScheme::Http => Some(Transport::Https),
        UrlScheme::Git | UrlScheme::Local => None,
    }
}

/// Clone URL transport (`ssh` for SSH remotes, else `https`).
pub fn url_transport(url: &str) -> Option<Transport> {
    RemoteUrl::parse(url).as_ref().and_then(transport_of)
}

pub(crate) fn clone_guess_impl(st: &AppState, url: &str) -> AppResult<CloneGuess> {
    let url = url.trim();
    let parsed = if clone::validate_url(url).is_ok() {
        RemoteUrl::parse(url)
    } else {
        None
    };
    let Some(parsed) = parsed else {
        return Ok(CloneGuess {
            valid: false,
            local: false,
            host: None,
            owner: None,
            repo: None,
            transport: None,
            https_url: None,
            ssh_url: None,
            candidates: Vec::new(),
        });
    };
    let repo = parsed.repo_name().map(str::to_owned);
    if parsed.is_local() {
        return Ok(CloneGuess {
            valid: true,
            local: true,
            host: None,
            owner: None,
            repo,
            transport: None,
            https_url: None,
            ssh_url: None,
            candidates: Vec::new(),
        });
    }
    let snap = Snapshot::load(&st.storage)?;
    let mut candidates: Vec<CloneCandidate> = Vec::new();
    let input = MatchInput {
        repo_path: None,
        remote_url: Some(url),
    };
    for rule in snap.rules.matching(&input) {
        if !candidates.iter().any(|c| c.account_id == rule.account_id)
            && snap.accounts.iter().any(|a| a.id == rule.account_id)
        {
            candidates.push(CloneCandidate {
                account_id: rule.account_id.clone(),
                identity_id: rule.identity_id.clone(),
                reason: GuessReason::Rule,
                rule_id: Some(rule.id.clone()),
            });
        }
    }
    let dir = snap.dir();
    let host = parsed.host.clone();
    // Base URL of a provider on this host (keeps a custom HTTPS port / sub-path).
    let mut provider_base: Option<String> = None;
    for account in &snap.accounts {
        let Some(provider) = dir.provider_of(&account.id) else {
            continue;
        };
        let on_host = provider
            .host()
            .is_some_and(|h| hostname_of(&h) == hostname_of(&host));
        if !on_host {
            continue;
        }
        provider_base.get_or_insert_with(|| provider.base_url.trim_end_matches('/').to_owned());
        if !candidates.iter().any(|c| c.account_id == account.id) {
            candidates.push(CloneCandidate {
                account_id: account.id.clone(),
                identity_id: None,
                reason: GuessReason::Host,
                rule_id: None,
            });
        }
    }
    // Among accounts matched only by host, the one whose username is the URL's owner
    // (`github.com/bob/x` → @bob) is the likely choice; rule matches keep their precedence.
    let owner = parsed
        .namespace()
        .and_then(|ns| ns.split('/').next())
        .map(str::to_lowercase);
    if let Some(owner) = owner {
        let first_host = candidates
            .iter()
            .position(|c| c.reason == GuessReason::Host)
            .unwrap_or(candidates.len());
        candidates[first_host..].sort_by_key(|c| {
            let username = snap
                .accounts
                .iter()
                .find(|a| a.id == c.account_id)
                .map(|a| a.username.to_lowercase());
            username.as_deref() != Some(owner.as_str())
        });
    }
    let transport = transport_of(&parsed);
    let path = parsed.path.clone();
    let https_url = match transport {
        Some(Transport::Https) => url.to_owned(),
        _ => format!(
            "{}/{path}.git",
            provider_base.unwrap_or_else(|| format!("https://{host}"))
        ),
    };
    let ssh_url = match transport {
        Some(Transport::Ssh) => url.to_owned(),
        _ => format!("git@{host}:{path}.git"),
    };
    Ok(CloneGuess {
        valid: true,
        local: false,
        host: Some(parsed.host_port()),
        owner: parsed.namespace().map(str::to_owned),
        repo,
        transport,
        https_url: Some(https_url),
        ssh_url: Some(ssh_url),
        candidates,
    })
}

/// Parses `url` and lists the accounts that could clone it (see [`CloneGuess`]).
#[tauri::command]
#[specta::specta]
pub async fn clone_guess(app: AppHandle, url: String) -> AppResult<CloneGuess> {
    blocking(app, move |st| clone_guess_impl(st, &url)).await
}

// ----- destination ---------------------------------------------------------------------------

/// Values substituted into the destination template.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TemplateVars {
    pub clone_dir: String,
    pub owner: String,
    pub repo: String,
    pub host: String,
    pub account: String,
}

/// Makes a value safe as one or more path segments: Windows-reserved characters and control
/// characters become `_`, `.`/`..` segments and trailing dots/spaces are neutralised.
fn sanitize_segments(value: &str) -> String {
    value
        .split(['/', '\\'])
        .filter(|s| !s.is_empty())
        .map(|seg| {
            let cleaned: String = seg
                .chars()
                .map(|c| {
                    if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') {
                        '_'
                    } else {
                        c
                    }
                })
                .collect();
            let trimmed = cleaned.trim_end_matches(['.', ' ']).trim_start();
            if trimmed.is_empty() {
                "_".to_owned()
            } else {
                trimmed.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Renders `template` (`{clone_dir}`, `{owner}`, `{repo}`, `{host}`, `{account}`). A result that
/// is not absolute is placed under `clone_dir`. Separators become the platform's, empty
/// segments (e.g. no owner) collapse. Mirrored by `renderTemplate` in the frontend.
pub fn render_template(template: &str, vars: &TemplateVars) -> PathBuf {
    let clone_dir = vars.clone_dir.trim_end_matches(['/', '\\']);
    let rendered = template
        .replace("{owner}", &sanitize_segments(&vars.owner))
        .replace("{repo}", &sanitize_segments(&vars.repo))
        .replace("{host}", &sanitize_segments(&vars.host.replace(':', "_")))
        .replace("{account}", &sanitize_segments(&vars.account));
    let rendered = if rendered.contains("{clone_dir}") {
        rendered.replace("{clone_dir}", clone_dir)
    } else if Path::new(&rendered).is_absolute() || rendered.starts_with(['/', '\\']) {
        rendered
    } else {
        format!("{clone_dir}/{rendered}")
    };
    let sep = std::path::MAIN_SEPARATOR;
    let unified: String = rendered
        .chars()
        .map(|c| if c == '/' || c == '\\' { sep } else { c })
        .collect();
    // Keep a leading `\\` (UNC share), collapse every other run of separators.
    let unc = unified.starts_with(&format!("{sep}{sep}"));
    let mut out = String::with_capacity(unified.len());
    let mut prev_sep = false;
    for (i, c) in unified.chars().enumerate() {
        if c == sep {
            if prev_sep && !(unc && i == 1) {
                continue;
            }
            prev_sep = true;
        } else {
            prev_sep = false;
        }
        out.push(c);
    }
    while out.len() > 3 && out.ends_with(sep) {
        out.pop();
    }
    PathBuf::from(out)
}

/// `~/Source`.
fn default_clone_root() -> PathBuf {
    clone_root_under(std::env::var_os(if cfg!(windows) {
        "USERPROFILE"
    } else {
        "HOME"
    }))
}

fn clone_root_under(home: Option<std::ffi::OsString>) -> PathBuf {
    home.map(PathBuf::from).unwrap_or_default().join("Source")
}

/// Free bytes on the volume that will hold `path` (its nearest existing ancestor).
fn free_space_for(path: &Path) -> Option<u64> {
    let mut p = Some(path);
    while let Some(cur) = p {
        if cur.exists() {
            return nanomgg_lfs::transfer::free_space(cur);
        }
        p = cur.parent();
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CloneDestination {
    /// Suggested (or the given) destination folder.
    pub path: String,
    /// Template used for the suggestion.
    pub template: String,
    /// The account's clone folder (or `~/Source`).
    pub clone_dir: String,
    pub owner: String,
    pub repo: String,
    pub host: String,
    /// Account user name (`{account}`).
    pub account: String,
    /// Free bytes on the destination volume (`None` if unknown).
    pub free_bytes: Option<u64>,
    /// The folder exists.
    pub exists: bool,
    /// Usable: missing or an empty folder.
    pub usable: bool,
    /// Repository size reported by the provider, when the URL is in the cached list.
    pub size_bytes: Option<u64>,
    pub lfs_size_bytes: Option<u64>,
}

fn path_template(st: &AppState) -> AppResult<String> {
    Ok(st
        .storage
        .get_setting::<String>(KEY_PATH_TEMPLATE)?
        .filter(|t| t.contains("{repo}"))
        .unwrap_or_else(|| DEFAULT_TEMPLATE.to_owned()))
}

/// Same repository: same host name and path (case-insensitive, `.git` ignored).
fn same_repo(a: &RemoteUrl, b: &str) -> bool {
    RemoteUrl::parse(b).is_some_and(|b| {
        !a.is_local()
            && hostname_of(&a.host) == hostname_of(&b.host)
            && a.path.eq_ignore_ascii_case(&b.path)
    })
}

/// Size of the repository at `url` from the cached lists (the account's first).
fn cached_size(account_id: Option<&str>, url: &RemoteUrl) -> Option<(Option<u64>, Option<u64>)> {
    let cache = cache();
    let mut entries: Vec<&CacheEntry> = Vec::new();
    if let Some(id) = account_id
        && let Some(e) = cache.get(id)
    {
        entries.push(e);
    }
    entries.extend(cache.values());
    entries
        .into_iter()
        .flat_map(|e| e.repos.iter())
        .find(|r| same_repo(url, &r.clone_https) || same_repo(url, &r.clone_ssh))
        .map(|r| (r.size_bytes, r.lfs_size_bytes))
}

pub(crate) fn clone_destination_impl(
    st: &AppState,
    account_id: Option<&str>,
    url: &str,
    path: Option<&str>,
) -> AppResult<CloneDestination> {
    let account = match account_id {
        Some(id) => st.storage.account(id).map_err(storage_err)?,
        None => None,
    };
    let clone_dir = account
        .as_ref()
        .and_then(|a| a.default_clone_dir.clone())
        .filter(|d| !d.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_clone_root);
    let parsed = RemoteUrl::parse(url.trim());
    let repo = parsed
        .as_ref()
        .and_then(|p| p.repo_name())
        .map(|r| r.trim_end_matches(".git").to_owned())
        .unwrap_or_default();
    let owner = parsed
        .as_ref()
        .and_then(|p| p.namespace())
        .unwrap_or_default()
        .to_owned();
    let host = parsed.as_ref().map(|p| p.host_port()).unwrap_or_default();
    let template = path_template(st)?;
    let vars = TemplateVars {
        clone_dir: clone_dir.to_string_lossy().into_owned(),
        owner: if parsed.as_ref().is_some_and(RemoteUrl::is_local) {
            String::new()
        } else {
            owner.clone()
        },
        repo: repo.clone(),
        host: host.clone(),
        account: account
            .as_ref()
            .map(|a| a.username.clone())
            .unwrap_or_default(),
    };
    let dest = match path.map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => PathBuf::from(p),
        None => render_template(&template, &vars),
    };
    let check = clone::check_destination(&dest);
    let sizes = parsed.as_ref().and_then(|p| cached_size(account_id, p));
    Ok(CloneDestination {
        path: dest.to_string_lossy().into_owned(),
        template,
        clone_dir: vars.clone_dir,
        owner: vars.owner,
        repo,
        host,
        account: vars.account,
        free_bytes: free_space_for(&dest),
        exists: dest.exists(),
        usable: matches!(check, Ok(Destination::Missing | Destination::EmptyDir)),
        size_bytes: sizes.and_then(|s| s.0),
        lfs_size_bytes: sizes.and_then(|s| s.1),
    })
}

/// Default destination for cloning `url` as `accountId` (or checks `path` when given), with
/// the free space of its volume.
#[tauri::command]
#[specta::specta]
pub async fn clone_destination(
    app: AppHandle,
    account_id: Option<String>,
    url: String,
    path: Option<String>,
) -> AppResult<CloneDestination> {
    blocking(app, move |st| {
        clone_destination_impl(st, account_id.as_deref(), &url, path.as_deref())
    })
    .await
}

/// Saves the destination template (must contain `{repo}`). Returns the stored template.
#[tauri::command]
#[specta::specta]
pub async fn clone_set_template(app: AppHandle, template: String) -> AppResult<String> {
    blocking(app, move |st| {
        let t = template.trim();
        if t.is_empty() {
            st.storage.delete_setting(KEY_PATH_TEMPLATE)?;
            return Ok(DEFAULT_TEMPLATE.to_owned());
        }
        if !t.contains("{repo}") || t.chars().any(char::is_control) {
            return Err(AppError::InvalidInput(
                "the template must contain {repo}".into(),
            ));
        }
        st.storage.set_setting(KEY_PATH_TEMPLATE, t)?;
        Ok(t.to_owned())
    })
    .await
}

// ----- clone ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CloneRequest {
    /// URL in the chosen transport (HTTPS or SSH form), or a local path.
    pub url: String,
    /// Account to clone as; `None` = git's own configuration (local / public repositories).
    #[serde(default)]
    #[specta(optional)]
    pub account_id: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub identity_id: Option<String>,
    pub dest: String,
    #[serde(default)]
    #[specta(optional)]
    pub branch: Option<String>,
    /// `Some(1)` = shallow clone.
    #[serde(default)]
    #[specta(optional)]
    pub depth: Option<u32>,
    #[serde(default)]
    pub recurse_submodules: bool,
    #[serde(default)]
    pub lfs: LfsMode,
    /// Copy the identity's name/email into the new repository's `.git/config`.
    #[serde(default)]
    pub write_identity: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CloneResult {
    pub tab: RepoTab,
    /// LFS files were left as pointers ("download later"): start `lfs_pull`.
    pub lfs_pending: bool,
    pub uses_lfs: bool,
    /// The partial LFS download failed after a successful clone (retry from the LFS panel).
    pub lfs_error: Option<String>,
    pub identity_written: bool,
}

/// Progress of a running clone, at most ~10 per second (phase changes always sent).
#[derive(Debug, Clone, Serialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct CloneProgress {
    /// Id chosen by the frontend when it started the clone.
    pub op_id: String,
    pub phase: ProgressPhase,
    pub label: String,
    pub remote: bool,
    pub percent: Option<u8>,
    pub current: Option<u64>,
    pub total: Option<u64>,
    /// e.g. `1.20 MiB`.
    pub bytes: Option<String>,
    /// e.g. `2.00 MiB/s`.
    pub throughput: Option<String>,
    pub done: bool,
}

#[derive(Debug, Default)]
struct Throttle {
    last: Option<(Instant, ProgressPhase, String)>,
}

impl Throttle {
    fn ready(&mut self, p: &Progress, now: Instant) -> bool {
        let send = match &self.last {
            None => true,
            Some((at, phase, label)) => {
                p.done
                    || *phase != p.phase
                    || *label != p.label
                    || now.duration_since(*at) >= PROGRESS_INTERVAL
            }
        };
        if send {
            self.last = Some((now, p.phase, p.label.clone()));
        }
        send
    }
}

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppState) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>()))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Runs the clone as the requested account (blocking).
pub(crate) fn clone_blocking(
    st: &AppState,
    req: &CloneRequest,
    cancel: &CancelToken,
    on_progress: impl FnMut(Progress),
) -> Result<CloneOutcome, OpError> {
    let ctx = match &req.account_id {
        Some(id) => context_for_account(st, id, req.identity_id.as_deref())
            .map_err(|e| OpError::rejected(FailureKind::InvalidInput, e.to_string()))?,
        None => AccountContext::default(),
    };
    let opts = CloneOptions {
        url: req.url.trim().to_owned(),
        dest: PathBuf::from(req.dest.trim()),
        branch: req
            .branch
            .as_deref()
            .map(str::trim)
            .filter(|b| !b.is_empty())
            .map(str::to_owned),
        depth: req.depth,
        recurse_submodules: req.recurse_submodules,
        lfs: req.lfs.clone(),
    };
    clone::clone(&st.git, &ctx, &opts, cancel, on_progress)
}

/// After a successful clone of `repo_id`: the `source = clone` binding, workspaces whose
/// default account is the clone account, and the identity in `.git/config` when asked.
/// Returns whether the identity was written.
pub(crate) fn bind_cloned(
    st: &AppState,
    repo_id: i64,
    root: &Path,
    req: &CloneRequest,
) -> AppResult<bool> {
    let Some(account_id) = req.account_id.as_deref() else {
        return Ok(false);
    };
    let account = st
        .storage
        .account(account_id)
        .map_err(storage_err)?
        .ok_or_else(|| AppError::InvalidInput(format!("account `{account_id}` not found")))?;
    let identities = st
        .storage
        .identities(Some(&account.id))
        .map_err(storage_err)?;
    let dir = Directory {
        accounts: std::slice::from_ref(&account),
        providers: &[],
        identities: &identities,
    };
    let identity = dir.identity_for(&account.id, req.identity_id.as_deref());
    st.storage
        .set_repo_binding(&RepoBinding {
            repo_id,
            remote_name: None,
            account_id: account.id.clone(),
            identity_id: identity.map(|i| i.id.clone()),
            transport: Some(url_transport(&req.url).unwrap_or(Transport::Https)),
            source: BindingSource::Clone,
            locked: false,
            updated_at: 0,
        })
        .map_err(storage_err)?;
    for ws in st.storage.workspaces().map_err(storage_err)? {
        if ws.default_account_id.as_deref() == Some(&account.id) && !ws.repo_ids.contains(&repo_id)
        {
            st.storage
                .add_repo_to_workspace(&ws.id, repo_id)
                .map_err(storage_err)?;
        }
    }
    let Some(identity) = identity.filter(|_| req.write_identity) else {
        return Ok(false);
    };
    let none = AccountContext::default();
    for (key, value) in [
        ("user.name", identity.name.as_str()),
        ("user.email", identity.email.as_str()),
    ] {
        st.git.run(root, &none, ["config", "--local", key, value])?;
    }
    Ok(true)
}

fn op_outcome<T>(result: Result<T, OpError>) -> AppResult<Outcome<T>> {
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

/// Clones `request.url` into `request.dest` with [`CloneProgress`] events; cancel with
/// `transfer_cancel(opId)` (the half-cloned folder is removed). On success the repository is
/// bound to the account (`source: clone`), added to the recent list and opened as a tab.
#[tauri::command]
#[specta::specta]
pub async fn clone_start(
    app: AppHandle,
    op_id: String,
    request: CloneRequest,
) -> AppResult<Outcome<CloneResult>> {
    // Registered before the thread starts, so a cancel sent right away is not lost.
    let (registration, token) = Registration::new(&op_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _registration = registration;
        let st = app.state::<AppState>();
        let started = Instant::now();
        let mut throttle = Throttle::default();
        let emit = |p: Progress| {
            if !throttle.ready(&p, Instant::now()) {
                return;
            }
            let event = CloneProgress {
                op_id: op_id.clone(),
                phase: p.phase,
                label: p.label,
                remote: p.remote,
                percent: p.percent,
                current: p.current,
                total: p.total,
                bytes: p.bytes,
                throughput: p.throughput,
                done: p.done,
            };
            if let Err(e) = event.emit(&app) {
                tracing::warn!(error = %e, "failed to emit clone progress");
            }
        };
        let result = clone_blocking(&st, &request, &token, emit);
        tracing::info!(
            ok = result.is_ok(),
            cancelled = matches!(result, Err(OpError::Cancelled)),
            ms = started.elapsed().as_millis() as u64,
            "clone finished"
        );
        let out = match op_outcome(result)? {
            Outcome::Done { value } => value,
            Outcome::Rejected { reason, message } => {
                return Ok(Outcome::Rejected { reason, message });
            }
            Outcome::Cancelled => return Ok(Outcome::Cancelled),
        };
        let info = repo::discover(&st.git, &out.path)?;
        let tab = crate::repos::register(&app, &st, info)?;
        let identity_written = match bind_cloned(&st, tab.id, &out.path, &request) {
            Ok(written) => written,
            Err(e) => {
                // The clone itself succeeded: keep the tab, report the binding problem in logs.
                tracing::warn!(repo = tab.id, error = %e, "could not bind the cloned repository");
                false
            }
        };
        Ok(Outcome::Done {
            value: CloneResult {
                tab,
                lfs_pending: out.lfs_pending,
                uses_lfs: out.uses_lfs,
                lfs_error: out
                    .lfs_error
                    .map(|e| nanomgg_logging::redact(&e).into_owned()),
                identity_written,
            },
        })
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[cfg(test)]
mod tests;
