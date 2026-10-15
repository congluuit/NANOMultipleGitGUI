//! CI status of commits (PLAN §5.6, Phase 4.3) for the graph badges, the commit details, the
//! status bar and PR/MR pages.
//!
//! - [`ci_statuses`]: several commits of an open repository in one call. The project is the
//!   hosted remote the frontend names (the current branch's upstream remote), else `origin`;
//!   commits without CI there are also looked up on one other hosted remote (fork workflow:
//!   CI often runs on `upstream`). At most [`MAX_SHAS`] commits per call, [`CONCURRENCY`]
//!   requests at a time.
//! - [`ci_status_for`]: one commit of any project with an explicit account (PR/MR pages).
//!
//! Results are cached in memory per (account, project, commit): final results for
//! [`TTL_FINAL`], "no CI" for [`TTL_NONE`], running ones for [`TTL_PENDING`] — the frontend
//! polls running commits every ~20 s while they are on screen.
//! Repositories without a hosted remote, unbound remotes and accounts without a token are
//! reported as a [`CiProblem`], never as an error.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use nanomgg_providers::ProviderClient;
use nanomgg_providers::ProviderError;
use nanomgg_providers::ci::{CiState, CiStatus};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::hosting::{HostedRemote, account_client, hosted_remotes};
use crate::state::AppState;

/// Commits read per call (the graph asks for the tips of its branches + recent commits).
pub const MAX_SHAS: usize = 40;
/// Parallel requests of one call.
const CONCURRENCY: usize = 4;
const TTL_FINAL: Duration = Duration::from_secs(10 * 60);
const TTL_NONE: Duration = Duration::from_secs(2 * 60);
const TTL_PENDING: Duration = Duration::from_secs(15);
const CACHE_CAP: usize = 4000;

/// Why no CI can be shown for a repository (rendered as a quiet hint, not an error).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CiProblem {
    /// No remote points at GitHub / GitLab.
    NoHostedRemote,
    /// The remote is not bound to an account.
    #[serde(rename_all = "camelCase")]
    Unbound { remote: String },
    /// The account has no token (sign in again).
    #[serde(rename_all = "camelCase")]
    NoToken { account_id: String },
    /// The provider refused / failed (missing scope, 404 for a private repo, rate limit…).
    #[serde(rename_all = "camelCase")]
    Provider {
        account_id: String,
        error: ProviderError,
    },
}

/// CI of one commit and where it was found.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CiEntry {
    pub remote: Option<String>,
    pub full_name: String,
    pub account_id: String,
    pub status: CiStatus,
}

/// Result of [`ci_statuses`] / [`ci_status_for`].
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CiBatch {
    /// One entry per commit that could be read, in request order.
    pub entries: Vec<CiEntry>,
    /// Set when nothing (or not everything) could be read.
    pub problem: Option<CiProblem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    account_id: String,
    host: String,
    full_name: String,
    sha: String,
}

#[derive(Debug, Clone)]
struct Cached {
    at: Instant,
    with_jobs: bool,
    status: CiStatus,
}

static CACHE: LazyLock<Mutex<HashMap<CacheKey, Cached>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn cache() -> MutexGuard<'static, HashMap<CacheKey, Cached>> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner())
}

fn ttl(state: CiState) -> Duration {
    match state {
        CiState::Pending => TTL_PENDING,
        CiState::None => TTL_NONE,
        _ => TTL_FINAL,
    }
}

fn cached(key: &CacheKey, with_jobs: bool, now: Instant) -> Option<CiStatus> {
    let map = cache();
    let hit = map.get(key)?;
    (now.duration_since(hit.at) < ttl(hit.status.state) && (hit.with_jobs || !with_jobs))
        .then(|| hit.status.clone())
}

fn store(key: CacheKey, with_jobs: bool, status: &CiStatus, now: Instant) {
    let mut map = cache();
    if map.len() >= CACHE_CAP {
        map.retain(|_, v| now.duration_since(v.at) < ttl(v.status.state));
        if map.len() >= CACHE_CAP {
            map.clear();
        }
    }
    map.insert(
        key,
        Cached {
            at: now,
            with_jobs,
            status: status.clone(),
        },
    );
}

/// Lower-cased, de-duplicated full commit ids (or abbreviations ≥ 7), at most [`MAX_SHAS`].
fn clean_shas(shas: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for sha in shas {
        let s = sha.trim().to_ascii_lowercase();
        if s.len() >= 7
            && s.len() <= 64
            && s.chars().all(|c| c.is_ascii_hexdigit())
            && !out.contains(&s)
        {
            out.push(s);
            if out.len() == MAX_SHAS {
                break;
            }
        }
    }
    out
}

/// One project + account to ask.
#[derive(Debug, Clone)]
struct Target {
    remote: Option<String>,
    host: String,
    full_name: String,
    account_id: String,
}

/// Reads `shas` of one target, cache first, [`CONCURRENCY`] requests at a time.
/// Returns one result per sha, in order.
async fn read_target(
    client: &ProviderClient,
    target: &Target,
    shas: &[String],
    with_jobs: bool,
) -> Vec<Result<CiStatus, ProviderError>> {
    let now = Instant::now();
    let key = |sha: &str| CacheKey {
        account_id: target.account_id.clone(),
        host: target.host.clone(),
        full_name: target.full_name.to_ascii_lowercase(),
        sha: sha.to_owned(),
    };
    enum Slot {
        Hit(CiStatus),
        Task(tauri::async_runtime::JoinHandle<Result<CiStatus, ProviderError>>),
    }
    let semaphore = Arc::new(tokio::sync::Semaphore::new(CONCURRENCY));
    let mut slots = Vec::with_capacity(shas.len());
    for sha in shas {
        if let Some(hit) = cached(&key(sha), with_jobs, now) {
            slots.push(Slot::Hit(hit));
            continue;
        }
        let client = client.clone();
        let full_name = target.full_name.clone();
        let sha = sha.clone();
        let semaphore = semaphore.clone();
        slots.push(Slot::Task(tauri::async_runtime::spawn(async move {
            let _permit = semaphore.acquire_owned().await;
            client.ci_status(&full_name, &sha, with_jobs).await
        })));
    }
    let mut out = Vec::with_capacity(shas.len());
    for (sha, slot) in shas.iter().zip(slots) {
        let result = match slot {
            Slot::Hit(status) => Ok(status),
            Slot::Task(handle) => {
                let result = handle.await.unwrap_or_else(|e| {
                    Err(ProviderError::Network {
                        message: e.to_string(),
                    })
                });
                if let Ok(status) = &result {
                    store(key(sha), with_jobs, status, Instant::now());
                }
                result
            }
        };
        out.push(result);
    }
    out
}

fn target_of(r: &HostedRemote) -> Result<Target, CiProblem> {
    let account_id = r.account_id.clone().ok_or_else(|| CiProblem::Unbound {
        remote: r.remote.clone(),
    })?;
    if !r.has_token {
        return Err(CiProblem::NoToken { account_id });
    }
    Ok(Target {
        remote: Some(r.remote.clone()),
        host: r.host.clone(),
        full_name: r.full_name.clone(),
        account_id,
    })
}

/// The preferred hosted remote (`remote` if it is one) and one fallback.
fn pick_remotes(all: Vec<HostedRemote>, remote: Option<&str>) -> Vec<HostedRemote> {
    let mut all = all;
    if let Some(name) = remote
        && let Some(i) = all.iter().position(|r| r.remote == name)
    {
        let preferred = all.remove(i);
        all.insert(0, preferred);
    }
    // The fallback must be another project, not a second remote of the same one.
    let mut out: Vec<HostedRemote> = Vec::new();
    for r in all {
        if !out
            .iter()
            .any(|o| o.host == r.host && o.full_name.eq_ignore_ascii_case(&r.full_name))
        {
            out.push(r);
        }
        if out.len() == 2 {
            break;
        }
    }
    out
}

/// CI of `shas` (full or abbreviated commit ids) of open repository `repo_id`. `remote` is the
/// preferred remote (the current branch's upstream remote); GitLab jobs are only read with
/// `with_jobs` (badges do not need them).
#[tauri::command]
#[specta::specta]
pub async fn ci_statuses(
    app: AppHandle,
    repo_id: i64,
    shas: Vec<String>,
    remote: Option<String>,
    with_jobs: bool,
) -> AppResult<CiBatch> {
    let shas = clean_shas(&shas);
    let handle = app.clone();
    let remotes = tauri::async_runtime::spawn_blocking(move || {
        hosted_remotes(&handle.state::<AppState>(), repo_id)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;
    let remotes = pick_remotes(remotes, remote.as_deref());
    let Some(first) = remotes.first() else {
        return Ok(CiBatch {
            entries: Vec::new(),
            problem: Some(CiProblem::NoHostedRemote),
        });
    };
    let primary = match target_of(first) {
        Ok(t) => t,
        Err(problem) => {
            return Ok(CiBatch {
                entries: Vec::new(),
                problem: Some(problem),
            });
        }
    };
    let fallback = remotes.get(1).and_then(|r| target_of(r).ok());
    if shas.is_empty() {
        return Ok(CiBatch {
            entries: Vec::new(),
            problem: None,
        });
    }
    let client = client_for(&app, &primary.account_id).await?;
    let results = read_target(&client, &primary, &shas, with_jobs).await;

    let mut entries: Vec<Option<CiEntry>> = vec![None; shas.len()];
    let mut problem = None;
    let mut retry: Vec<usize> = Vec::new();
    for (i, result) in results.into_iter().enumerate() {
        match result {
            Ok(status) => {
                if status.state == CiState::None {
                    retry.push(i);
                }
                entries[i] = Some(entry(&primary, status));
            }
            Err(error) => {
                retry.push(i);
                if problem.is_none() {
                    problem = Some(CiProblem::Provider {
                        account_id: primary.account_id.clone(),
                        error,
                    });
                }
            }
        }
    }
    if let Some(fallback) = fallback.filter(|_| !retry.is_empty()) {
        let client = if fallback.account_id == primary.account_id {
            client
        } else {
            client_for(&app, &fallback.account_id).await?
        };
        let wanted: Vec<String> = retry.iter().map(|&i| shas[i].clone()).collect();
        let results = read_target(&client, &fallback, &wanted, with_jobs).await;
        for (&i, result) in retry.iter().zip(results) {
            if let Ok(status) = result
                && (status.state != CiState::None || entries[i].is_none())
            {
                entries[i] = Some(entry(&fallback, status));
            }
        }
        // A commit found on the fallback is enough: drop the primary's error if all resolved.
        if entries.iter().all(Option::is_some) {
            problem = None;
        }
    }
    Ok(CiBatch {
        entries: entries.into_iter().flatten().collect(),
        problem,
    })
}

fn entry(target: &Target, status: CiStatus) -> CiEntry {
    CiEntry {
        remote: target.remote.clone(),
        full_name: target.full_name.clone(),
        account_id: target.account_id.clone(),
        status,
    }
}

async fn client_for(app: &AppHandle, account_id: &str) -> AppResult<ProviderClient> {
    let app = app.clone();
    let account_id = account_id.to_owned();
    tauri::async_runtime::spawn_blocking(move || account_client(&app, &account_id))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

/// CI of one commit of `full_name` read with `account_id` (PR/MR pages, which know the account
/// and project but may have no open repository). Always includes the jobs.
#[tauri::command]
#[specta::specta]
pub async fn ci_status_for(
    app: AppHandle,
    account_id: String,
    full_name: String,
    sha: String,
) -> AppResult<CiBatch> {
    let shas = clean_shas(std::slice::from_ref(&sha));
    if shas.is_empty() {
        return Err(AppError::InvalidInput(format!(
            "`{sha}` is not a commit id"
        )));
    }
    let client = match client_for(&app, &account_id).await {
        Ok(c) => c,
        Err(AppError::Unsupported(_)) => {
            return Ok(CiBatch {
                entries: Vec::new(),
                problem: Some(CiProblem::NoToken { account_id }),
            });
        }
        Err(e) => return Err(e),
    };
    let target = Target {
        remote: None,
        host: client.api().endpoint().web_host(),
        full_name,
        account_id,
    };
    let mut results = read_target(&client, &target, &shas, true).await;
    Ok(match results.pop() {
        Some(Ok(status)) => CiBatch {
            entries: vec![entry(&target, status)],
            problem: None,
        },
        Some(Err(error)) => CiBatch {
            entries: Vec::new(),
            problem: Some(CiProblem::Provider {
                account_id: target.account_id,
                error,
            }),
        },
        None => CiBatch {
            entries: Vec::new(),
            problem: None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote(name: &str, full: &str, account: Option<&str>, token: bool) -> HostedRemote {
        HostedRemote {
            remote: name.into(),
            host: "github.com".into(),
            full_name: full.into(),
            account_id: account.map(Into::into),
            family: None,
            has_token: token,
        }
    }

    #[test]
    fn cleans_and_caps_shas() {
        let mut input: Vec<String> = vec![
            " ABCDEF1 ".into(),
            "abcdef1".into(),
            "xyz".into(),
            "12345".into(),
            "--all".into(),
        ];
        input.extend((0..100).map(|i| format!("{i:040x}")));
        let out = clean_shas(&input);
        assert_eq!(out[0], "abcdef1");
        assert_eq!(out.len(), MAX_SHAS);
        assert!(out.iter().all(|s| s.chars().all(|c| c.is_ascii_hexdigit())));
    }

    #[test]
    fn prefers_the_requested_remote_and_one_other_project() {
        let all = vec![
            remote("origin", "me/lib", Some("a"), true),
            remote("upstream", "org/lib", Some("a"), true),
            remote("mirror", "ORG/lib", Some("a"), true),
            remote("other", "x/y", Some("a"), true),
        ];
        let picked = pick_remotes(all.clone(), Some("upstream"));
        let names: Vec<_> = picked.iter().map(|r| r.remote.as_str()).collect();
        assert_eq!(names, ["upstream", "origin"]);
        let picked = pick_remotes(all, Some("missing"));
        assert_eq!(picked[0].remote, "origin");
        assert!(pick_remotes(Vec::new(), None).is_empty());
    }

    #[test]
    fn unbound_and_tokenless_remotes_are_problems() {
        assert_eq!(
            target_of(&remote("origin", "a/b", None, false)).unwrap_err(),
            CiProblem::Unbound {
                remote: "origin".into()
            }
        );
        assert_eq!(
            target_of(&remote("origin", "a/b", Some("acc"), false)).unwrap_err(),
            CiProblem::NoToken {
                account_id: "acc".into()
            }
        );
        let t = target_of(&remote("origin", "a/b", Some("acc"), true)).unwrap();
        assert_eq!(t.account_id, "acc");
    }

    #[test]
    fn cache_respects_ttl_and_jobs() {
        let key = CacheKey {
            account_id: "acc".into(),
            host: "github.com".into(),
            full_name: "a/b".into(),
            sha: "abcdef1".into(),
        };
        let status = CiStatus {
            sha: "abcdef1".into(),
            state: CiState::Pending,
            jobs: Vec::new(),
            pipeline_url: None,
            truncated: false,
            checks_error: None,
        };
        let t0 = Instant::now();
        store(key.clone(), false, &status, t0);
        assert!(cached(&key, false, t0).is_some());
        // Stored without jobs: not enough for a request that wants them.
        assert!(cached(&key, true, t0).is_none());
        assert!(cached(&key, false, t0 + TTL_PENDING + Duration::from_secs(1)).is_none());
        let done = CiStatus {
            state: CiState::Success,
            ..status
        };
        store(key.clone(), true, &done, t0);
        assert!(cached(&key, true, t0 + TTL_PENDING * 2).is_some());
        assert!(cached(&key, false, t0 + TTL_FINAL + Duration::from_secs(1)).is_none());
    }
}
