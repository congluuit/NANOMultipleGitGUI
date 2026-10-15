//! "My work" panel (Phase 4.2): PR/MRs waiting for my review, my PR/MRs and issues assigned to
//! me, gathered from every account that has a token — one task per account, so a failing
//! account (expired token, rate limit, offline instance) only marks its own row.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use nanomgg_accounts::AccountSecret;
use nanomgg_providers::mywork::WorkLists;
use nanomgg_providers::{CancellationToken, cancellable};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::accounts::commands::account_secret;
use crate::accounts::storage_err;
use crate::error::{AppError, AppResult};
use crate::hosting::account_client;
use crate::issues::{IssueCommandError, blocking};
use crate::state::AppState;

/// A cached answer younger than this is returned without asking the provider again.
const CACHE_TTL: Duration = Duration::from_secs(60);
/// Items per list and account.
const PER_LIST: u8 = 30;

/// One account's part of the panel.
#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkAccount {
    pub account_id: String,
    /// `None` when the fetch failed (see `error`).
    pub lists: Option<WorkLists>,
    pub error: Option<IssueCommandError>,
    /// When `lists` was fetched (ms since the epoch).
    pub fetched_at: Option<i64>,
}

#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkOverview {
    /// Accounts with a token, in switcher order.
    pub accounts: Vec<WorkAccount>,
    /// Accounts left out because no token is stored (the UI offers to sign in).
    pub without_token: Vec<String>,
    /// Stopped by [`mywork_cancel`] or a newer call; accounts that were still loading carry a
    /// `cancelled` provider error.
    pub cancelled: bool,
}

struct Cached {
    lists: WorkLists,
    fetched_at: i64,
    at: Instant,
}

static CACHE: LazyLock<Mutex<HashMap<String, Cached>>> = LazyLock::new(Mutex::default);
static RUNNING: LazyLock<Mutex<Option<CancellationToken>>> = LazyLock::new(Mutex::default);

fn cache() -> MutexGuard<'static, HashMap<String, Cached>> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner())
}

fn running() -> MutexGuard<'static, Option<CancellationToken>> {
    RUNNING.lock().unwrap_or_else(|e| e.into_inner())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// `(with token, without token)` account ids in switcher order.
fn accounts_by_token(st: &AppState) -> AppResult<(Vec<String>, Vec<String>)> {
    let mut accounts = st.storage.accounts().map_err(storage_err)?;
    accounts.sort_by_key(|a| a.sort_order);
    Ok(accounts
        .into_iter()
        .map(|a| a.id)
        .partition(|id| matches!(account_secret(st, id, AccountSecret::Token), Ok(Some(_)))))
}

async fn fetch_one(app: AppHandle, account_id: String, cancel: CancellationToken) -> WorkAccount {
    let id = account_id.clone();
    let result: Result<WorkLists, IssueCommandError> = async {
        let client = blocking(move || account_client(&app, &id)).await?;
        Ok(cancellable(&cancel, client.my_work(PER_LIST)).await?)
    }
    .await;
    match result {
        Ok(lists) => {
            let fetched_at = now_ms();
            cache().insert(
                account_id.clone(),
                Cached {
                    lists: lists.clone(),
                    fetched_at,
                    at: Instant::now(),
                },
            );
            WorkAccount {
                account_id,
                lists: Some(lists),
                error: None,
                fetched_at: Some(fetched_at),
            }
        }
        Err(error) => {
            if let IssueCommandError::Provider { error: e, .. } = &error {
                tracing::warn!(account = %account_id, code = e.code(), "my work fetch failed");
            }
            WorkAccount {
                account_id,
                lists: None,
                error: Some(error),
                fetched_at: None,
            }
        }
    }
}

/// The "My work" lists of every account with a token. Answers younger than a minute come from
/// the cache unless `refresh`. A new call cancels the one still running.
#[tauri::command]
#[specta::specta]
pub async fn mywork_list(app: AppHandle, refresh: bool) -> AppResult<WorkOverview> {
    let a = app.clone();
    let (with_token, without_token) =
        blocking(move || accounts_by_token(&a.state::<AppState>())).await?;
    let cancel = CancellationToken::new();
    if let Some(previous) = running().replace(cancel.clone()) {
        previous.cancel();
    }
    let mut slots: Vec<Option<WorkAccount>> = Vec::with_capacity(with_token.len());
    let mut tasks = Vec::new();
    {
        let cache = cache();
        for (i, id) in with_token.iter().enumerate() {
            match cache.get(id) {
                Some(c) if !refresh && c.at.elapsed() < CACHE_TTL => {
                    slots.push(Some(WorkAccount {
                        account_id: id.clone(),
                        lists: Some(c.lists.clone()),
                        error: None,
                        fetched_at: Some(c.fetched_at),
                    }))
                }
                _ => {
                    slots.push(None);
                    let (app, id, cancel) = (app.clone(), id.clone(), cancel.clone());
                    tasks.push((i, tauri::async_runtime::spawn(fetch_one(app, id, cancel))));
                }
            }
        }
    }
    for (i, task) in tasks {
        let account = match task.await {
            Ok(account) => account,
            Err(e) => WorkAccount {
                account_id: with_token[i].clone(),
                lists: None,
                error: Some(AppError::Internal(e.to_string()).into()),
                fetched_at: None,
            },
        };
        slots[i] = Some(account);
    }
    // A newer call replaces the token and cancels this one, so the slot is still ours only if
    // nothing cancelled us.
    let cancelled = cancel.is_cancelled();
    if !cancelled {
        *running() = None;
    }
    Ok(WorkOverview {
        accounts: slots.into_iter().flatten().collect(),
        without_token,
        cancelled,
    })
}

/// Stops the running [`mywork_list`] (it then answers with `cancelled: true`).
#[tauri::command]
#[specta::specta]
pub fn mywork_cancel() -> bool {
    match running().take() {
        Some(c) => {
            c.cancel();
            true
        }
        None => false,
    }
}
