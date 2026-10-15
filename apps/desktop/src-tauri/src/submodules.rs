//! Submodule commands (PLAN §5.9): list, init + update (progress + cancel, like fetch), update
//! to the recorded commit, sync URLs, add, deinit, remove.
//!
//! # Which account a submodule's network access runs as
//!
//! A submodule usually lives elsewhere than its parent (another org, another host). For each
//! submodule URL the context is chosen in this order:
//! 1. relative URL (`../lib.git`) or no URL: the parent's `origin` account (same server);
//! 2. the rule engine / the parent's own bindings when they fit the URL's host
//!    (`nanomgg_accounts::resolve` with the submodule URL as `origin`);
//! 3. the account on the URL's host (as the clone dialog guesses: username = owner first);
//! 4. otherwise the parent's default account — the credential server only answers for that
//!    account's host, so a submodule on another host then uses no app credential (limitation,
//!    shown in the UI as `source = parent` with `hostMismatch`).
//!
//! One `git submodule update` runs per account: nested submodules (`--recursive`) inherit the
//! context of their top-level submodule.

use std::path::Path;
use std::time::{Duration, Instant};

use nanomgg_accounts::url::hostname_of;
use nanomgg_accounts::{RemoteInfo, RemoteUrl};
use nanomgg_git_engine::AccountContext;
use nanomgg_git_engine::ops::OpError;
use nanomgg_git_engine::progress::{CancelToken, Progress};
use nanomgg_git_engine::submodule::{self, Submodule, SubmoduleUpdateSummary, UpdateOptions};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::accounts::{self, Snapshot, storage_err};
use crate::branches::{Outcome, Registration, TransferKind, TransferProgress};
use crate::error::{AppError, AppResult};
use crate::repos::repo_root;
use crate::state::AppState;

/// Where a submodule's account comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SubmoduleAccountSource {
    /// Relative URL: same server as the parent, parent's account.
    Parent,
    /// A rule or one of the parent's bindings matches the URL.
    Rule,
    /// The only / best account on the URL's host.
    Host,
    /// Nothing matched: the parent's default account is used (see `host_mismatch`).
    Fallback,
    /// No account at all (unbound parent): git's own credentials.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SubmoduleAccount {
    pub account_id: Option<String>,
    pub source: SubmoduleAccountSource,
    /// The account's provider is on another host than the submodule URL: the app cannot give
    /// git a credential for it (public submodules still work).
    pub host_mismatch: bool,
}

/// A submodule with the account its network access will use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SubmoduleInfo {
    pub submodule: Submodule,
    pub account: SubmoduleAccount,
}

fn outcome<T>(result: Result<T, OpError>) -> AppResult<Outcome<T>> {
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

fn is_relative(url: &str) -> bool {
    url.starts_with("../") || url.starts_with("./")
}

/// Account (and context) for a submodule URL; see the module docs for the order.
pub(crate) fn submodule_context(
    st: &AppState,
    repo_id: i64,
    root: &Path,
    url: Option<&str>,
) -> AppResult<(AccountContext, SubmoduleAccount)> {
    let parent = accounts::resolve_repo(st, repo_id, root)?;
    let parent_origin = parent.for_remote(Some("origin")).cloned();
    let with = |id: &str, identity: Option<&str>| accounts::context_for_account(st, id, identity);
    let fallback = |host_mismatch: bool| -> AppResult<(AccountContext, SubmoduleAccount)> {
        match &parent.default {
            Some(r) => Ok((
                with(&r.account_id, r.identity_id.as_deref())?,
                SubmoduleAccount {
                    account_id: Some(r.account_id.clone()),
                    source: if host_mismatch {
                        SubmoduleAccountSource::Fallback
                    } else {
                        SubmoduleAccountSource::Parent
                    },
                    host_mismatch,
                },
            )),
            None => Ok((
                AccountContext::default(),
                SubmoduleAccount {
                    account_id: None,
                    source: SubmoduleAccountSource::None,
                    host_mismatch: false,
                },
            )),
        }
    };
    let Some(url) = url.filter(|u| !u.trim().is_empty()) else {
        return fallback(false);
    };
    if is_relative(url) {
        return match parent_origin {
            Some(r) => Ok((
                with(&r.account_id, r.identity_id.as_deref())?,
                SubmoduleAccount {
                    account_id: Some(r.account_id),
                    source: SubmoduleAccountSource::Parent,
                    host_mismatch: false,
                },
            )),
            None => fallback(false),
        };
    }
    let Some(parsed) = RemoteUrl::parse(url) else {
        return fallback(false);
    };
    if parsed.is_local() {
        return fallback(false);
    }
    // Rules + the parent's bindings (host-checked) applied to the submodule URL.
    let bindings = st.storage.repo_bindings(repo_id).map_err(storage_err)?;
    let snap = Snapshot::load(&st.storage)?;
    let resolution = nanomgg_accounts::resolve(
        &root.to_string_lossy(),
        &[RemoteInfo {
            name: "origin".into(),
            url: url.to_owned(),
        }],
        &bindings,
        &snap.rules,
        &snap.dir(),
    );
    let on_host = |account_id: &str| {
        snap.dir()
            .account_host(account_id)
            .is_some_and(|h| hostname_of(&h) == hostname_of(&parsed.host))
    };
    if let Some(r) = resolution.for_remote(Some("origin"))
        && (r.rule_id.is_some() || on_host(&r.account_id))
    {
        return Ok((
            with(&r.account_id, r.identity_id.as_deref())?,
            SubmoduleAccount {
                account_id: Some(r.account_id.clone()),
                source: SubmoduleAccountSource::Rule,
                host_mismatch: !on_host(&r.account_id),
            },
        ));
    }
    let guess = crate::clone::clone_guess_impl(st, url)?;
    if let Some(c) = guess.candidates.first() {
        return Ok((
            with(&c.account_id, c.identity_id.as_deref())?,
            SubmoduleAccount {
                account_id: Some(c.account_id.clone()),
                source: SubmoduleAccountSource::Host,
                host_mismatch: false,
            },
        ));
    }
    let mismatch = parent
        .default
        .as_ref()
        .is_some_and(|r| !on_host(&r.account_id));
    fallback(mismatch)
}

/// Raw `.gitmodules` URL of a direct submodule (the listed one is redacted).
fn raw_url(st: &AppState, root: &Path, s: &Submodule) -> Option<String> {
    let name = s.name.as_deref()?;
    let dir = match &s.parent {
        Some(p) => root.join(p),
        None => root.to_owned(),
    };
    st.git
        .run_text(
            &dir,
            &AccountContext::default(),
            [
                "config",
                "--file",
                ".gitmodules",
                "--get",
                &format!("submodule.{name}.url"),
            ],
        )
        .ok()
        .filter(|u| !u.is_empty())
}

/// The direct submodule a (possibly nested) path belongs to.
fn top_level<'a>(all: &'a [Submodule], s: &'a Submodule) -> &'a Submodule {
    let mut cur = s;
    while let Some(parent) = &cur.parent {
        match all.iter().find(|x| &x.path == parent) {
            Some(p) => cur = p,
            None => break,
        }
    }
    cur
}

pub(crate) fn list_impl(st: &AppState, repo_id: i64) -> AppResult<Vec<SubmoduleInfo>> {
    let root = repo_root(st, repo_id)?;
    let all = submodule::list(&st.git, &root)?;
    let mut out = Vec::with_capacity(all.len());
    for s in &all {
        let top = top_level(&all, s);
        let (_, account) =
            submodule_context(st, repo_id, &root, raw_url(st, &root, top).as_deref())?;
        out.push(SubmoduleInfo {
            submodule: s.clone(),
            account,
        });
    }
    Ok(out)
}

/// Every submodule (nested ones too) with its state and the account it will use.
#[tauri::command]
#[specta::specta]
pub async fn submodule_list(app: AppHandle, repo_id: i64) -> AppResult<Vec<SubmoduleInfo>> {
    let _ = repo_root(&app.state::<AppState>(), repo_id)?;
    tauri::async_runtime::spawn_blocking(move || list_impl(&app.state::<AppState>(), repo_id))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Runs a submodule network operation with progress events (`transferProgress`, kind
/// `fetch`) and cancellation (`transfer_cancel(opId)`).
async fn run_network<T: Send + 'static>(
    app: AppHandle,
    repo_id: i64,
    op_id: String,
    f: impl FnOnce(
        &AppState,
        &Path,
        &CancelToken,
        &mut dyn FnMut(Progress),
    ) -> AppResult<Result<T, OpError>>
    + Send
    + 'static,
) -> AppResult<Outcome<T>> {
    let root = repo_root(&app.state::<AppState>(), repo_id)?;
    let (registration, token) = Registration::new(&op_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _registration = registration;
        let st = app.state::<AppState>();
        let mut last: Option<(Instant, String)> = None;
        let mut emit = |p: Progress| {
            let now = Instant::now();
            let ready = p.done
                || last.as_ref().is_none_or(|(at, label)| {
                    *label != p.label || now.duration_since(*at) >= Duration::from_millis(100)
                });
            if !ready {
                return;
            }
            last = Some((now, p.label.clone()));
            let event = TransferProgress {
                repo_id,
                op_id: op_id.clone(),
                kind: TransferKind::Fetch,
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
                tracing::warn!(error = %e, "failed to emit submodule progress");
            }
        };
        let result = f(&st, &root, &token, &mut emit)?;
        outcome(result)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_impl(
    st: &AppState,
    repo_id: i64,
    root: &Path,
    paths: &[String],
    init: bool,
    recursive: bool,
    cancel: &CancelToken,
    sink: &mut dyn FnMut(Progress),
) -> AppResult<Result<SubmoduleUpdateSummary, OpError>> {
    let all = submodule::list(&st.git, root)?;
    // Targets: the requested paths, or every direct submodule.
    let targets: Vec<&Submodule> = if paths.is_empty() {
        all.iter().filter(|s| s.parent.is_none()).collect()
    } else {
        let mut v = Vec::new();
        for p in paths {
            let wanted = p.replace('\\', "/");
            match all.iter().find(|s| s.path == wanted) {
                Some(s) => v.push(s),
                None => {
                    return Ok(Err(OpError::rejected(
                        nanomgg_git_engine::ops::FailureKind::NotFound,
                        format!("no submodule at '{p}'"),
                    )));
                }
            }
        }
        v
    };
    // Group by account (context of the top-level submodule).
    let mut groups: Vec<(AccountContext, Vec<String>)> = Vec::new();
    for s in targets {
        let top = top_level(&all, s);
        let (ctx, _) = submodule_context(st, repo_id, root, raw_url(st, root, top).as_deref())?;
        match groups.iter_mut().find(|(c, _)| *c == ctx) {
            Some((_, list)) => list.push(s.path.clone()),
            None => groups.push((ctx, vec![s.path.clone()])),
        }
    }
    let single = groups.len() == 1 && paths.is_empty();
    let mut summary = SubmoduleUpdateSummary::default();
    for (ctx, group) in groups {
        let opts = UpdateOptions {
            paths: if single { Vec::new() } else { group },
            init,
            recursive,
        };
        match submodule::update(&st.git, root, &ctx, &opts, cancel, &mut *sink) {
            Ok(s) => summary.checked_out.extend(s.checked_out),
            Err(e) => return Ok(Err(e)),
        }
    }
    Ok(Ok(summary))
}

/// `git submodule update [--init] [--recursive]` for `paths` (empty = every submodule), each
/// group of submodules as its own account.
#[tauri::command]
#[specta::specta]
pub async fn submodule_update(
    app: AppHandle,
    repo_id: i64,
    op_id: String,
    paths: Vec<String>,
    init: bool,
    recursive: bool,
) -> AppResult<Outcome<SubmoduleUpdateSummary>> {
    run_network(app, repo_id, op_id, move |st, root, cancel, sink| {
        update_impl(st, repo_id, root, &paths, init, recursive, cancel, sink)
    })
    .await
}

/// `git submodule add [-b branch] -- <url> <path>`; the clone runs as the account the URL
/// resolves to.
#[tauri::command]
#[specta::specta]
pub async fn submodule_add(
    app: AppHandle,
    repo_id: i64,
    op_id: String,
    url: String,
    path: String,
    branch: Option<String>,
) -> AppResult<Outcome<Submodule>> {
    run_network(app, repo_id, op_id, move |st, root, cancel, sink| {
        let (ctx, _) = submodule_context(st, repo_id, root, Some(url.trim()))?;
        Ok(submodule::add(
            &st.git,
            root,
            &ctx,
            &url,
            &path,
            branch.as_deref(),
            cancel,
            sink,
        ))
    })
    .await
}

/// Account a submodule URL would use (for the "Add submodule" dialog).
#[tauri::command]
#[specta::specta]
pub async fn submodule_url_account(
    app: AppHandle,
    repo_id: i64,
    url: String,
) -> AppResult<SubmoduleAccount> {
    let root = repo_root(&app.state::<AppState>(), repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        Ok(submodule_context(&st, repo_id, &root, Some(url.trim()))?.1)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

async fn run_local<T: Send + 'static>(
    app: AppHandle,
    repo_id: i64,
    f: impl FnOnce(&AppState, &Path) -> Result<T, OpError> + Send + 'static,
) -> AppResult<Outcome<T>> {
    let root = repo_root(&app.state::<AppState>(), repo_id)?;
    tauri::async_runtime::spawn_blocking(move || outcome(f(&app.state::<AppState>(), &root)))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

/// `git submodule sync [--recursive]` (copy URLs from `.gitmodules` to the config).
#[tauri::command]
#[specta::specta]
pub async fn submodule_sync(
    app: AppHandle,
    repo_id: i64,
    paths: Vec<String>,
    recursive: bool,
) -> AppResult<Outcome<()>> {
    run_local(app, repo_id, move |st, root| {
        submodule::sync(&st.git, root, &AccountContext::default(), &paths, recursive)
    })
    .await
}

/// `git submodule deinit [-f]`: empties the folder, keeps `.gitmodules`. Local changes come
/// back as `dirtyWorktree` (ask, then retry with `force`).
#[tauri::command]
#[specta::specta]
pub async fn submodule_deinit(
    app: AppHandle,
    repo_id: i64,
    path: String,
    force: bool,
) -> AppResult<Outcome<()>> {
    run_local(app, repo_id, move |st, root| {
        submodule::deinit(&st.git, root, &AccountContext::default(), &path, force)
    })
    .await
}

/// Removes the submodule from the repository (deinit + `git rm`); commit afterwards.
#[tauri::command]
#[specta::specta]
pub async fn submodule_remove(
    app: AppHandle,
    repo_id: i64,
    path: String,
    force: bool,
) -> AppResult<Outcome<()>> {
    run_local(app, repo_id, move |st, root| {
        submodule::remove(&st.git, root, &AccountContext::default(), &path, force)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_urls_are_detected() {
        assert!(is_relative("../lib.git"));
        assert!(is_relative("./x"));
        assert!(!is_relative("https://github.com/a/b.git"));
    }

    #[test]
    fn top_level_walks_parents() {
        let sub = |path: &str, parent: Option<&str>| Submodule {
            name: Some(path.into()),
            path: path.into(),
            parent: parent.map(str::to_owned),
            depth: 0,
            url: None,
            branch: None,
            recorded: None,
            head: None,
            head_branch: None,
            state: nanomgg_git_engine::submodule::SubmoduleState::Uninitialized,
            modified: false,
            untracked: false,
        };
        let all = vec![
            sub("a", None),
            sub("a/b", Some("a")),
            sub("a/b/c", Some("a/b")),
        ];
        assert_eq!(top_level(&all, &all[2]).path, "a");
        assert_eq!(top_level(&all, &all[0]).path, "a");
    }

    use crate::accounts::commands::{
        AccountInput, IdentityInput, ProviderInput, RuleInput, account_save_impl,
        identity_save_impl, provider_save_impl, rule_save_impl,
    };
    use nanomgg_accounts::{AuthMethod, ProviderKind, SigningMethod};
    use nanomgg_git_engine::repo;
    use nanomgg_storage::Storage;

    fn provider(st: &AppState, kind: ProviderKind, base_url: &str) -> String {
        provider_save_impl(
            st,
            ProviderInput {
                id: None,
                kind,
                name: base_url.into(),
                base_url: base_url.into(),
                api_url: None,
                oauth_client_id: None,
                large_file_warn_mb: None,
                large_file_block_mb: None,
            },
        )
        .unwrap()
        .id
    }

    fn account(st: &AppState, provider_id: &str, user: &str, signing_key: Option<&str>) -> String {
        let id = account_save_impl(
            st,
            AccountInput {
                id: None,
                provider_id: provider_id.into(),
                username: user.into(),
                display_name: None,
                avatar_url: None,
                auth_method: AuthMethod::Pat,
                scopes: vec![],
                token_expires_at: None,
                color: None,
                label: Some(user.to_uppercase()),
                default_clone_dir: None,
            },
        )
        .unwrap()
        .id;
        identity_save_impl(
            st,
            IdentityInput {
                id: None,
                account_id: id.clone(),
                name: user.to_uppercase(),
                email: format!("{user}@acme.dev"),
                signing: if signing_key.is_some() {
                    SigningMethod::Ssh
                } else {
                    SigningMethod::None
                },
                signing_key_ref: signing_key.map(str::to_owned),
                is_default: true,
            },
        )
        .unwrap();
        id
    }

    #[test]
    fn submodule_urls_resolve_to_accounts_and_identities_carry_signing() {
        let tmp = tempfile::tempdir().unwrap();
        let st = AppState::new(Storage::open_in_memory().unwrap());
        let gh = provider(
            &st,
            ProviderKind::GithubEnterprise,
            "https://github.acme.dev",
        );
        let gl = provider(
            &st,
            ProviderKind::GitlabSelfHosted,
            "https://gitlab.example.com",
        );
        let key = tmp.path().join("sign.pub");
        let alice = account(&st, &gh, "alice", Some(&key.to_string_lossy()));
        let bob = account(&st, &gl, "bob", None);
        rule_save_impl(
            &st,
            RuleInput {
                id: None,
                priority: None,
                host: Some("github.acme.dev".into()),
                owner_or_group: Some("acme".into()),
                path_glob: None,
                remote_regex: None,
                account_id: alice.clone(),
                identity_id: None,
                enabled: true,
            },
        )
        .unwrap();

        let dir = tmp.path().join("app");
        std::fs::create_dir_all(&dir).unwrap();
        let none = AccountContext::default();
        st.git
            .run(&dir, &none, ["init", "-q", "-b", "main"])
            .unwrap();
        st.git
            .run(
                &dir,
                &none,
                [
                    "remote",
                    "add",
                    "origin",
                    "https://github.acme.dev/acme/app.git",
                ],
            )
            .unwrap();
        let info = repo::discover(&st.git, &dir).unwrap();
        let rec = st
            .storage
            .touch_repo(
                &info.root.to_string_lossy(),
                &crate::repos::path_key(&info.root),
                "app",
                1,
            )
            .unwrap();
        st.workspace().repos.insert(rec.id, info.clone());
        let root = info.root;

        let (ctx, a) = submodule_context(&st, rec.id, &root, Some("../lib.git")).unwrap();
        assert_eq!(a.source, SubmoduleAccountSource::Parent);
        assert_eq!(a.account_id.as_deref(), Some(alice.as_str()));
        // The identity's SSH signing is injected.
        let signing = ctx.signing.clone().expect("signing injected");
        assert_eq!(signing.format, nanomgg_git_engine::SigningFormat::Ssh);
        assert!(
            ctx.config_entries()
                .contains(&("commit.gpgsign".into(), "true".into()))
        );

        let (_, a) = submodule_context(
            &st,
            rec.id,
            &root,
            Some("https://github.acme.dev/acme/lib.git"),
        )
        .unwrap();
        assert_eq!(a.source, SubmoduleAccountSource::Rule);
        let (ctx, a) = submodule_context(
            &st,
            rec.id,
            &root,
            Some("git@gitlab.example.com:team/x.git"),
        )
        .unwrap();
        assert_eq!(a.source, SubmoduleAccountSource::Host);
        assert_eq!(a.account_id.as_deref(), Some(bob.as_str()));
        assert!(ctx.signing.is_none());
        let (_, a) =
            submodule_context(&st, rec.id, &root, Some("https://elsewhere.dev/x.git")).unwrap();
        assert_eq!(a.source, SubmoduleAccountSource::Fallback);
        assert!(a.host_mismatch);
        assert!(list_impl(&st, rec.id).unwrap().is_empty());
    }
}
