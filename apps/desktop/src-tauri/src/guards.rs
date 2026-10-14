//! Commit Guard & Push Guard (PLAN §4.6, §6.3).
//!
//! - [`commit_guard`]: before a commit — the identity git will record vs the repo's account
//!   (rule-engine expectation, the account's own emails, provider-verified emails), signing
//!   config, unbound repo. Warnings only: the UI decides (switch account / commit anyway).
//! - [`push_guard`]: before a push — unpushed commits whose author/committer differ from the
//!   pushing identity, push permission from the provider API, blobs over the provider's
//!   limits introduced by unpushed commits.
//! - Fixes, restricted to unpushed commits and always behind a backup ref
//!   (`refs/nanomgg/backup/<ts>`): [`guard_fix_authors`], [`guard_lfs_migrate`], then
//!   [`guard_undo`] / [`guard_backups`] / [`guard_backup_delete`].
//!
//! Each report is built in three steps so tests can drive them without Tauri: blocking
//! *facts* (SQLite + git, on a blocking thread), an optional network step (provider emails /
//! permission, short timeout, failures ignored — offline never blocks), then a pure builder.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use nanomgg_accounts::url::hostname_of;
use nanomgg_accounts::{
    Account, AccountSecret, Identity, Provider, RemoteUrl, Resolved, SigningMethod,
};
use nanomgg_git_engine::guard::{
    self, BackupRef, FixAuthorMode, HeavyBlob, PushRange, RestoreOutcome, RewriteOutcome,
    UnpushedCommit,
};
use nanomgg_git_engine::ops::OpError;
use nanomgg_git_engine::{AccountContext, GitRunner};
use nanomgg_lfs::guard::{GuardLevel, GuardThresholds, MIB};
use nanomgg_providers::Provider as _;
use nanomgg_providers::{
    AccessLevel, ProviderClient, ProviderEmail, ProviderEndpoint, SecretToken,
};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::accounts::commands::account_secret;
use crate::accounts::{Snapshot, repo_remotes, resolve_repo};
use crate::auth::AuthRuntime;
use crate::branches::Outcome;
use crate::error::{AppError, AppResult};
use crate::repos::repo_root;
use crate::state::AppState;

/// Provider email lists are cached per account this long.
pub const EMAIL_CACHE_TTL: Duration = Duration::from_secs(10 * 60);
/// Network calls of the guards give up after this long (then: unknown, never blocking).
pub const NETWORK_TIMEOUT: Duration = Duration::from_secs(8);
/// Webmail domains: such an address on an organisation's repo is a hint of a wrong identity.
const PERSONAL_DOMAINS: &[&str] = &[
    "gmail.com",
    "googlemail.com",
    "yahoo.com",
    "outlook.com",
    "hotmail.com",
    "live.com",
    "icloud.com",
    "me.com",
    "proton.me",
    "protonmail.com",
    "gmx.com",
    "yandex.com",
    "qq.com",
    "163.com",
];

type EmailCache = Mutex<HashMap<String, (Instant, Vec<ProviderEmail>)>>;
static EMAIL_CACHE: LazyLock<EmailCache> = LazyLock::new(Mutex::default);

// ----- DTOs ----------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum IdentitySource {
    /// Injected from the bound account's identity.
    Account,
    /// The repo is unbound (or the account has no identity): git's own `user.email`.
    GitConfig,
}

/// The identity git will record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GuardIdentity {
    pub name: String,
    pub email: String,
    pub source: IdentitySource,
    pub account_id: Option<String>,
    pub signing: SigningMethod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SuggestionReason {
    /// A rule matches this repo for that account.
    MatchedRule,
    /// The account's user name is the remote's owner.
    RemoteOwner,
    /// One of the account's identities uses the current email.
    EmailOwner,
    /// The account's email domain looks like the remote's organisation.
    OrgDomain,
    /// Same provider host as the remote.
    SameHost,
}

/// "[Đổi sang tài khoản X]" candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountSuggestion {
    pub account_id: String,
    /// Label, else user name.
    pub label: String,
    pub username: String,
    pub color: String,
    /// Identity the switch would use (rule's identity or the account default).
    pub identity_id: Option<String>,
    pub email: Option<String>,
    pub reason: SuggestionReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MismatchReason {
    /// A rule assigns this repo to another account (binding overrides it).
    RuleAccount,
    /// The email belongs to none of the bound account's identities / provider emails.
    NotAccountEmail,
    /// Webmail address on an organisation's repo while an account with a matching domain exists.
    PersonalEmailOnOrgRepo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CommitFinding {
    #[serde(rename_all = "camelCase")]
    EmailMismatch {
        email: String,
        reason: MismatchReason,
        expected_email: Option<String>,
        expected_account_id: Option<String>,
        suggestions: Vec<AccountSuggestion>,
    },
    /// Not (or not verified) in the provider's email list: the commit will not link to the
    /// profile. Only reported when the list could be fetched.
    #[serde(rename_all = "camelCase")]
    EmailNotVerified {
        email: String,
        account_id: String,
        account_label: String,
    },
    /// `commit.gpgsign=true` but no signing key configured (git would fail or sign wrongly).
    #[serde(rename_all = "camelCase")]
    SigningMissing {
        gpg_sign: bool,
        identity_signing: SigningMethod,
    },
    #[serde(rename_all = "camelCase")]
    Unbound { suggestions: Vec<AccountSuggestion> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CommitGuardReport {
    pub repo_id: i64,
    pub identity: Option<GuardIdentity>,
    /// Account bound to the repo (default context), if any.
    pub account_id: Option<String>,
    pub findings: Vec<CommitFinding>,
    /// Provider emails were checked (false when offline / no token).
    pub emails_checked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PermissionStatus {
    Allowed,
    /// The provider says the account cannot push: block early.
    Denied,
    /// Not checked (unbound, no token, other host) or the check failed (network…).
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PushPermission {
    pub status: PermissionStatus,
    pub level: Option<AccessLevel>,
    pub account_id: Option<String>,
    pub account_label: Option<String>,
    /// Why it is unknown (provider error code: `network`, `timeout`, `noToken`…).
    pub reason: Option<String>,
}

impl PushPermission {
    fn unknown(reason: &str) -> Self {
        Self {
            status: PermissionStatus::Unknown,
            level: None,
            account_id: None,
            account_label: None,
            reason: Some(reason.to_owned()),
        }
    }
}

/// A heavy blob with its verdict against the provider limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GuardHeavyBlob {
    pub oid: String,
    pub path: String,
    pub size: u64,
    pub commits: Vec<String>,
    /// Introduced by an already-pushed commit: explain, never rewrite.
    pub in_shared_commit: bool,
    /// `warn` or `block`.
    pub level: GuardLevel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PushFinding {
    #[serde(rename_all = "camelCase")]
    AuthorMismatch { count: u32, email: String },
    #[serde(rename_all = "camelCase")]
    PermissionDenied {
        account_label: Option<String>,
        level: Option<AccessLevel>,
    },
    /// Blobs over the limits in unpushed commits that can be moved to LFS.
    #[serde(rename_all = "camelCase")]
    HeavyBlobs { block: u32, warn: u32 },
    /// Heavy blobs in commits already on a remote branch (guide only).
    #[serde(rename_all = "camelCase")]
    HeavyBlobsShared { count: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PushGuardReport {
    pub repo_id: i64,
    pub remote: String,
    pub branch: String,
    pub upstream: Option<String>,
    pub identity: Option<GuardIdentity>,
    /// Newest first (at most 1000).
    pub unpushed: Vec<UnpushedCommit>,
    pub total_unpushed: u32,
    /// No unpushed commit is on another remote branch: fixes may rewrite them.
    pub rewritable: bool,
    /// Unpushed commits whose author or committer email differs from `identity`.
    pub author_mismatch: Vec<UnpushedCommit>,
    pub heavy_blobs: Vec<GuardHeavyBlob>,
    pub thresholds: GuardThresholds,
    pub permission: PushPermission,
    pub findings: Vec<PushFinding>,
    /// The push will certainly fail (permission denied or a blob over the hard limit).
    pub blocked: bool,
    /// The branch is checked out (fixes need it).
    pub is_current: bool,
}

// ----- shared helpers ------------------------------------------------------------------------

fn label_of(a: &Account) -> String {
    a.label
        .clone()
        .filter(|l| !l.trim().is_empty())
        .unwrap_or_else(|| a.username.clone())
}

fn email_domain(email: &str) -> Option<String> {
    email
        .trim()
        .rsplit_once('@')
        .map(|(_, d)| d.to_ascii_lowercase())
        .filter(|d| !d.is_empty())
}

fn same_email(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// `users.noreply.github.com` / GitLab `noreply` addresses are always linked to the profile.
fn is_noreply(email: &str) -> bool {
    email_domain(email)
        .is_some_and(|d| d.starts_with("users.noreply.") || d.starts_with("noreply."))
}

fn owner_of(remote: &RemoteUrl) -> Option<String> {
    remote.owner().map(str::to_ascii_lowercase)
}

/// Accounts, providers and identities (from [`Snapshot`]).
#[derive(Debug, Clone, Default)]
pub(crate) struct Directory {
    pub accounts: Vec<Account>,
    pub providers: Vec<Provider>,
    pub identities: Vec<Identity>,
}

impl Directory {
    fn from_snapshot(snap: &Snapshot) -> Self {
        Self {
            accounts: snap.accounts.clone(),
            providers: snap.providers.clone(),
            identities: snap.identities.clone(),
        }
    }

    fn account(&self, id: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.id == id)
    }

    fn provider_of(&self, account: &Account) -> Option<&Provider> {
        self.providers.iter().find(|p| p.id == account.provider_id)
    }

    fn identity_for(&self, account_id: &str, preferred: Option<&str>) -> Option<&Identity> {
        nanomgg_accounts::AccountDirectory {
            accounts: &self.accounts,
            providers: &self.providers,
            identities: &self.identities,
        }
        .identity_for(account_id, preferred)
    }

    fn account_host(&self, account: &Account) -> Option<String> {
        self.provider_of(account)
            .and_then(Provider::host)
            .map(|h| hostname_of(&h))
    }

    fn suggestion(
        &self,
        account: &Account,
        identity_id: Option<&str>,
        reason: SuggestionReason,
    ) -> AccountSuggestion {
        let identity = self.identity_for(&account.id, identity_id);
        AccountSuggestion {
            account_id: account.id.clone(),
            label: label_of(account),
            username: account.username.clone(),
            color: account.color.clone(),
            identity_id: identity.map(|i| i.id.clone()),
            email: identity.map(|i| i.email.clone()),
            reason,
        }
    }

    /// Switch candidates, best first, without `exclude`.
    fn suggestions(
        &self,
        first: Option<(&Resolved, SuggestionReason)>,
        remote: Option<&RemoteUrl>,
        email: Option<&str>,
        exclude: Option<&str>,
    ) -> Vec<AccountSuggestion> {
        let mut out: Vec<AccountSuggestion> = Vec::new();
        let mut push = |s: AccountSuggestion| {
            if Some(s.account_id.as_str()) != exclude
                && !out.iter().any(|o| o.account_id == s.account_id)
            {
                out.push(s);
            }
        };
        if let Some((r, reason)) = first
            && let Some(a) = self.account(&r.account_id)
        {
            push(self.suggestion(a, r.identity_id.as_deref(), reason));
        }
        let host = remote.filter(|r| !r.is_local()).map(|r| r.host.clone());
        let owner = remote.and_then(owner_of);
        let on_host = |a: &Account| host.is_some() && self.account_host(a) == host;
        if let Some(owner) = &owner {
            for a in self.accounts.iter().filter(|a| on_host(a)) {
                if a.username.eq_ignore_ascii_case(owner) {
                    push(self.suggestion(a, None, SuggestionReason::RemoteOwner));
                }
            }
            for a in &self.accounts {
                if let Some(i) = self.org_domain_identity(a, owner) {
                    push(self.suggestion(a, Some(&i.id), SuggestionReason::OrgDomain));
                }
            }
        }
        if let Some(email) = email {
            for i in self
                .identities
                .iter()
                .filter(|i| same_email(&i.email, email))
            {
                if let Some(a) = self.account(&i.account_id) {
                    push(self.suggestion(a, Some(&i.id), SuggestionReason::EmailOwner));
                }
            }
        }
        for a in self.accounts.iter().filter(|a| on_host(a)) {
            push(self.suggestion(a, None, SuggestionReason::SameHost));
        }
        out.truncate(4);
        out
    }

    /// An identity of `account` whose (non-webmail) email domain looks like `owner`
    /// (`acme-corp` ↔ `@acme.dev`, `@acme-corp.com`).
    fn org_domain_identity(&self, account: &Account, owner: &str) -> Option<&Identity> {
        self.identities
            .iter()
            .filter(|i| i.account_id == account.id)
            .find(|i| {
                email_domain(&i.email).is_some_and(|d| {
                    !PERSONAL_DOMAINS.contains(&d.as_str())
                        && !is_noreply(&i.email)
                        && d.split('.')
                            .next()
                            .is_some_and(|label| label.len() >= 3 && owner.contains(label))
                })
            })
    }

    /// Large-file limits of the account's provider (settings), in bytes.
    fn thresholds_for(
        &self,
        account: Option<&Account>,
        remote: Option<&RemoteUrl>,
    ) -> GuardThresholds {
        if let Some(p) = account.and_then(|a| self.provider_of(a)) {
            return GuardThresholds {
                warn_bytes: u64::from(p.large_file_warn_mb) * MIB,
                block_bytes: p.large_file_block_mb.map(|mb| u64::from(mb) * MIB),
            };
        }
        match remote.map(|r| r.host.as_str()) {
            Some("github.com") => GuardThresholds::GITHUB,
            _ => GuardThresholds::GITLAB,
        }
    }
}

/// `git config --get <key>` with the identity of `ctx` injected; `None` when unset.
fn config_get(git: &GitRunner, root: &Path, ctx: &AccountContext, key: &str) -> Option<String> {
    git.run_text(root, ctx, ["config", "--get", key])
        .ok()
        .filter(|v| !v.is_empty())
}

fn config_bool(git: &GitRunner, root: &Path, ctx: &AccountContext, key: &str) -> bool {
    git.run_text(root, ctx, ["config", "--type=bool", "--get", key])
        .is_ok_and(|v| v == "true")
}

/// Identity git will use for a command resolved to `resolved` (account identity, else
/// git's own config).
fn effective_identity(
    st: &AppState,
    root: &Path,
    dir: &Directory,
    resolved: Option<&Resolved>,
) -> Option<GuardIdentity> {
    if let Some(r) = resolved
        && let Some(i) = dir.identity_for(&r.account_id, r.identity_id.as_deref())
    {
        return Some(GuardIdentity {
            name: i.name.clone(),
            email: i.email.clone(),
            source: IdentitySource::Account,
            account_id: Some(r.account_id.clone()),
            signing: i.signing,
        });
    }
    let none = AccountContext::default();
    let email = config_get(&st.git, root, &none, "user.email")?;
    Some(GuardIdentity {
        name: config_get(&st.git, root, &none, "user.name").unwrap_or_default(),
        email,
        source: IdentitySource::GitConfig,
        account_id: resolved.map(|r| r.account_id.clone()),
        signing: SigningMethod::None,
    })
}

fn identity_ctx(identity: Option<&GuardIdentity>) -> AccountContext {
    AccountContext {
        identity: identity
            .filter(|i| i.source == IdentitySource::Account)
            .map(|i| nanomgg_git_engine::Identity {
                name: i.name.clone(),
                email: i.email.clone(),
            }),
        ..Default::default()
    }
}

/// Authenticated API client of an account, when it has a token.
pub(crate) struct ApiAccess {
    pub account_id: String,
    pub account_label: String,
    pub endpoint: ProviderEndpoint,
    pub token: SecretToken,
}

fn api_access(st: &AppState, dir: &Directory, account_id: &str) -> Option<ApiAccess> {
    let account = dir.account(account_id)?;
    let provider = dir.provider_of(account)?;
    let token = account_secret(st, account_id, AccountSecret::Token).ok()??;
    let endpoint = crate::auth::service::endpoint_for(provider).ok()?;
    Some(ApiAccess {
        account_id: account_id.to_owned(),
        account_label: label_of(account),
        endpoint,
        token: SecretToken::from(token),
    })
}

/// Provider emails of the account (10-min cache; `None` when offline or rejected).
pub(crate) async fn cached_emails(
    http: &reqwest::Client,
    access: &ApiAccess,
) -> Option<Vec<ProviderEmail>> {
    {
        let cache = EMAIL_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, list)) = cache.get(&access.account_id)
            && at.elapsed() < EMAIL_CACHE_TTL
        {
            return Some(list.clone());
        }
    }
    let client = ProviderClient::new(http.clone(), access.endpoint.clone(), access.token.clone());
    match tokio::time::timeout(NETWORK_TIMEOUT, client.emails()).await {
        Ok(Ok(list)) => {
            EMAIL_CACHE
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(access.account_id.clone(), (Instant::now(), list.clone()));
            Some(list)
        }
        Ok(Err(e)) => {
            tracing::info!(account = %access.account_id, error = %e, "commit guard: emails unavailable");
            None
        }
        Err(_) => None,
    }
}

// ----- commit guard --------------------------------------------------------------------------

/// Everything the commit report needs, gathered on a blocking thread.
pub(crate) struct CommitFacts {
    pub repo_id: i64,
    pub dir: Directory,
    pub identity: Option<GuardIdentity>,
    /// Effective default resolution (bindings + rules).
    pub bound: Option<Resolved>,
    /// What the rule engine alone would pick (no bindings).
    pub rule: Option<Resolved>,
    /// Primary remote (`origin`, else the first).
    pub remote: Option<RemoteUrl>,
    pub gpg_sign: bool,
    pub signing_key: bool,
    pub api: Option<ApiAccess>,
}

pub(crate) fn commit_facts(st: &AppState, repo_id: i64) -> AppResult<CommitFacts> {
    let root = repo_root(st, repo_id)?;
    let resolution = resolve_repo(st, repo_id, &root)?;
    let remotes = repo_remotes(st, &root)?;
    let snap = Snapshot::load(&st.storage)?;
    let rule_only = nanomgg_accounts::resolve(
        &root.to_string_lossy(),
        &remotes,
        &[],
        &snap.rules,
        &snap.dir(),
    );
    let dir = Directory::from_snapshot(&snap);
    let bound = resolution.default.clone();
    let identity = effective_identity(st, &root, &dir, bound.as_ref());
    let ctx = identity_ctx(identity.as_ref());
    let remote = resolution
        .remotes
        .iter()
        .find(|r| r.remote == "origin")
        .or_else(|| resolution.remotes.first())
        .and_then(|r| r.parsed.clone());
    let api = bound
        .as_ref()
        .filter(|_| {
            identity
                .as_ref()
                .is_some_and(|i| i.source == IdentitySource::Account)
        })
        .and_then(|b| api_access(st, &dir, &b.account_id));
    Ok(CommitFacts {
        repo_id,
        identity,
        bound,
        rule: rule_only.default.filter(|r| r.rule_id.is_some()),
        remote,
        gpg_sign: config_bool(&st.git, &root, &ctx, "commit.gpgsign"),
        signing_key: config_get(&st.git, &root, &ctx, "user.signingkey").is_some(),
        api,
        dir,
    })
}

/// Pure report builder. `provider_emails`: the bound account's provider emails, when fetched.
pub(crate) fn build_commit_report(
    facts: &CommitFacts,
    provider_emails: Option<&[ProviderEmail]>,
) -> CommitGuardReport {
    let dir = &facts.dir;
    let mut findings = Vec::new();
    let email = facts.identity.as_ref().map(|i| i.email.clone());
    match &facts.bound {
        None => findings.push(CommitFinding::Unbound {
            suggestions: dir.suggestions(
                facts
                    .rule
                    .as_ref()
                    .map(|r| (r, SuggestionReason::MatchedRule)),
                facts.remote.as_ref(),
                email.as_deref(),
                None,
            ),
        }),
        Some(bound) => {
            if let Some(email) = &email {
                if let Some(f) = mismatch(facts, bound, email, provider_emails) {
                    findings.push(f);
                }
                let account = dir.account(&bound.account_id);
                if let (Some(list), Some(account)) = (provider_emails, account)
                    && !is_noreply(email)
                    && !list
                        .iter()
                        .any(|e| e.verified && same_email(&e.email, email))
                {
                    findings.push(CommitFinding::EmailNotVerified {
                        email: email.clone(),
                        account_id: account.id.clone(),
                        account_label: label_of(account),
                    });
                }
            }
        }
    }
    if facts.gpg_sign && !facts.signing_key {
        findings.push(CommitFinding::SigningMissing {
            gpg_sign: true,
            identity_signing: facts
                .identity
                .as_ref()
                .map_or(SigningMethod::None, |i| i.signing),
        });
    }
    CommitGuardReport {
        repo_id: facts.repo_id,
        identity: facts.identity.clone(),
        account_id: facts.bound.as_ref().map(|b| b.account_id.clone()),
        findings,
        emails_checked: provider_emails.is_some(),
    }
}

fn mismatch(
    facts: &CommitFacts,
    bound: &Resolved,
    email: &str,
    provider_emails: Option<&[ProviderEmail]>,
) -> Option<CommitFinding> {
    let dir = &facts.dir;
    let remote = facts.remote.as_ref();
    // 1. A rule says another account and that account's identity uses another email.
    if let Some(rule) = &facts.rule
        && rule.account_id != bound.account_id
    {
        let expected = dir.identity_for(&rule.account_id, rule.identity_id.as_deref());
        if expected.is_none_or(|i| !same_email(&i.email, email)) {
            return Some(CommitFinding::EmailMismatch {
                email: email.to_owned(),
                reason: MismatchReason::RuleAccount,
                expected_email: expected.map(|i| i.email.clone()),
                expected_account_id: Some(rule.account_id.clone()),
                suggestions: dir.suggestions(
                    Some((rule, SuggestionReason::MatchedRule)),
                    remote,
                    Some(email),
                    Some(&bound.account_id),
                ),
            });
        }
    }
    // 2. The email is not one of the bound account's addresses.
    let own = dir
        .identities
        .iter()
        .any(|i| i.account_id == bound.account_id && same_email(&i.email, email))
        || provider_emails.is_some_and(|l| l.iter().any(|e| same_email(&e.email, email)));
    if !own {
        let expected = dir.identity_for(&bound.account_id, bound.identity_id.as_deref());
        return Some(CommitFinding::EmailMismatch {
            email: email.to_owned(),
            reason: MismatchReason::NotAccountEmail,
            expected_email: expected.map(|i| i.email.clone()),
            expected_account_id: Some(bound.account_id.clone()),
            suggestions: dir.suggestions(None, remote, Some(email), Some(&bound.account_id)),
        });
    }
    // 3. Webmail on an organisation's repo while another account has a matching domain.
    let domain = email_domain(email)?;
    let owner = remote.and_then(owner_of)?;
    let account = dir.account(&bound.account_id)?;
    if PERSONAL_DOMAINS.contains(&domain.as_str()) && !account.username.eq_ignore_ascii_case(&owner)
    {
        let candidate = dir
            .accounts
            .iter()
            .filter(|a| a.id != bound.account_id)
            .find_map(|a| dir.org_domain_identity(a, &owner).map(|i| (a, i)));
        if let Some((a, i)) = candidate {
            let first = Resolved {
                account_id: a.id.clone(),
                identity_id: Some(i.id.clone()),
                source: nanomgg_accounts::ContextSource::None,
                rule_id: None,
                binding_remote: None,
                from_binding: false,
            };
            return Some(CommitFinding::EmailMismatch {
                email: email.to_owned(),
                reason: MismatchReason::PersonalEmailOnOrgRepo,
                expected_email: Some(i.email.clone()),
                expected_account_id: Some(a.id.clone()),
                suggestions: dir.suggestions(
                    Some((&first, SuggestionReason::OrgDomain)),
                    remote,
                    Some(email),
                    Some(&bound.account_id),
                ),
            });
        }
    }
    None
}

pub(crate) async fn commit_guard_impl(
    facts: CommitFacts,
    http: Option<&reqwest::Client>,
) -> CommitGuardReport {
    let emails = match (&facts.api, http) {
        (Some(api), Some(http)) => cached_emails(http, api).await,
        _ => None,
    };
    build_commit_report(&facts, emails.as_deref())
}

// ----- push guard ----------------------------------------------------------------------------

pub(crate) struct PushFacts {
    pub repo_id: i64,
    pub identity: Option<GuardIdentity>,
    pub unpushed: guard::UnpushedReport,
    pub heavy: Vec<HeavyBlob>,
    pub thresholds: GuardThresholds,
    pub is_current: bool,
    /// Remote URL and the account pushing to it (`None` when unbound).
    pub remote_url: Option<String>,
    pub api: Option<ApiAccess>,
    pub bound: bool,
}

pub(crate) fn push_facts(
    st: &AppState,
    repo_id: i64,
    remote: &str,
    branch: &str,
) -> AppResult<Result<PushFacts, OpError>> {
    let root = repo_root(st, repo_id)?;
    let range = match PushRange::resolve(&st.git, &root, remote, branch) {
        Ok(r) => r,
        Err(e) => return Ok(Err(e)),
    };
    let resolution = resolve_repo(st, repo_id, &root)?;
    let snap = Snapshot::load(&st.storage)?;
    let dir = Directory::from_snapshot(&snap);
    let resolved = resolution.for_remote(Some(remote)).cloned();
    let identity = effective_identity(st, &root, &dir, resolved.as_ref());
    let parsed = resolution
        .remotes
        .iter()
        .find(|r| r.remote == remote)
        .and_then(|r| r.parsed.clone());
    let account = resolved.as_ref().and_then(|r| dir.account(&r.account_id));
    let thresholds = dir.thresholds_for(account, parsed.as_ref());
    let unpushed = match guard::unpushed(&st.git, &root, &range) {
        Ok(u) => u,
        Err(e) => return Ok(Err(e)),
    };
    let heavy = if unpushed.total == 0 {
        Vec::new()
    } else {
        match guard::heavy_blobs(&st.git, &root, &range, thresholds.warn_bytes) {
            Ok(h) => h,
            Err(e) => return Ok(Err(e)),
        }
    };
    let remote_url = repo_remotes(st, &root)?
        .into_iter()
        .find(|r| r.name == remote)
        .map(|r| r.url);
    Ok(Ok(PushFacts {
        repo_id,
        identity,
        is_current: nanomgg_git_engine::ops::current_branch(&st.git, &root).as_deref()
            == Some(branch),
        unpushed,
        heavy,
        thresholds,
        api: resolved
            .as_ref()
            .and_then(|r| api_access(st, &dir, &r.account_id)),
        bound: resolved.is_some(),
        remote_url,
    }))
}

/// Push permission via the provider API; any failure → unknown (never blocks).
pub(crate) async fn check_permission(
    http: &reqwest::Client,
    api: Option<&ApiAccess>,
    remote_url: Option<&str>,
    bound: bool,
) -> PushPermission {
    let Some(api) = api else {
        return PushPermission::unknown(if bound { "noToken" } else { "unbound" });
    };
    let Some(url) = remote_url else {
        return PushPermission::unknown("noRemote");
    };
    let client = ProviderClient::new(http.clone(), api.endpoint.clone(), api.token.clone());
    let result =
        tokio::time::timeout(NETWORK_TIMEOUT, client.repo_permission_for_remote(url)).await;
    let with_account = |mut p: PushPermission| {
        p.account_id = Some(api.account_id.clone());
        p.account_label = Some(api.account_label.clone());
        p
    };
    match result {
        Ok(Ok(perm)) => with_account(PushPermission {
            status: if perm.can_push {
                PermissionStatus::Allowed
            } else {
                PermissionStatus::Denied
            },
            level: Some(perm.level),
            account_id: None,
            account_label: None,
            reason: None,
        }),
        Ok(Err(e)) => {
            tracing::info!(account = %api.account_id, error = %e, "push guard: permission unknown");
            with_account(PushPermission::unknown(e.code()))
        }
        Err(_) => with_account(PushPermission::unknown("timeout")),
    }
}

pub(crate) fn build_push_report(
    facts: PushFacts,
    remote: &str,
    branch: &str,
    permission: PushPermission,
) -> PushGuardReport {
    let mut findings = Vec::new();
    let author_mismatch: Vec<UnpushedCommit> = match &facts.identity {
        Some(id) => guard::author_mismatches(&facts.unpushed.commits, &id.email)
            .into_iter()
            .cloned()
            .collect(),
        None => Vec::new(),
    };
    if let Some(id) = &facts.identity
        && !author_mismatch.is_empty()
    {
        findings.push(PushFinding::AuthorMismatch {
            count: author_mismatch.len() as u32,
            email: id.email.clone(),
        });
    }
    if permission.status == PermissionStatus::Denied {
        findings.push(PushFinding::PermissionDenied {
            account_label: permission.account_label.clone(),
            level: permission.level,
        });
    }
    let t = facts.thresholds;
    let heavy_blobs: Vec<GuardHeavyBlob> = facts
        .heavy
        .into_iter()
        .filter(|b| b.size >= t.warn_bytes)
        .map(|b| GuardHeavyBlob {
            level: if t.block_bytes.is_some_and(|limit| b.size > limit) {
                GuardLevel::Block
            } else {
                GuardLevel::Warn
            },
            oid: b.oid,
            path: b.path,
            size: b.size,
            commits: b.commits,
            in_shared_commit: b.in_shared_commit,
        })
        .collect();
    let (local, shared): (Vec<&GuardHeavyBlob>, Vec<&GuardHeavyBlob>) =
        heavy_blobs.iter().partition(|b| !b.in_shared_commit);
    let count =
        |list: &[&GuardHeavyBlob], level| list.iter().filter(|b| b.level == level).count() as u32;
    if !local.is_empty() {
        findings.push(PushFinding::HeavyBlobs {
            block: count(&local, GuardLevel::Block),
            warn: count(&local, GuardLevel::Warn),
        });
    }
    if !shared.is_empty() {
        findings.push(PushFinding::HeavyBlobsShared {
            count: shared.len() as u32,
        });
    }
    let blocked = permission.status == PermissionStatus::Denied
        || heavy_blobs.iter().any(|b| b.level == GuardLevel::Block);
    PushGuardReport {
        repo_id: facts.repo_id,
        remote: remote.to_owned(),
        branch: branch.to_owned(),
        upstream: facts.unpushed.upstream.clone(),
        identity: facts.identity,
        total_unpushed: facts.unpushed.total,
        rewritable: facts.unpushed.rewritable,
        unpushed: facts.unpushed.commits,
        author_mismatch,
        heavy_blobs,
        thresholds: t,
        permission,
        findings,
        blocked,
        is_current: facts.is_current,
    }
}

// ----- commands ------------------------------------------------------------------------------

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppState) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>()))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
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

fn http_client(app: &AppHandle) -> Option<reqwest::Client> {
    app.try_state::<AuthRuntime>().map(|rt| rt.http.clone())
}

/// Commit Guard report for the repo's default context. Never fails because of the network.
#[tauri::command]
#[specta::specta]
pub async fn commit_guard(app: AppHandle, repo_id: i64) -> AppResult<CommitGuardReport> {
    let facts = blocking(app.clone(), move |st| commit_facts(st, repo_id)).await?;
    let http = http_client(&app);
    Ok(commit_guard_impl(facts, http.as_ref()).await)
}

/// Push Guard report for pushing `branch` to `remote`. A `rejected` outcome means the names
/// are invalid / the branch does not exist.
#[tauri::command]
#[specta::specta]
pub async fn push_guard(
    app: AppHandle,
    repo_id: i64,
    remote: String,
    branch: String,
) -> AppResult<Outcome<PushGuardReport>> {
    let (r, b) = (remote.clone(), branch.clone());
    let facts = match blocking(app.clone(), move |st| push_facts(st, repo_id, &r, &b)).await? {
        Ok(f) => f,
        Err(e) => return outcome(Err(e)),
    };
    let permission = match http_client(&app) {
        Some(http) => {
            check_permission(
                &http,
                facts.api.as_ref(),
                facts.remote_url.as_deref(),
                facts.bound,
            )
            .await
        }
        None => PushPermission::unknown("offline"),
    };
    Ok(Outcome::Done {
        value: build_push_report(facts, &remote, &branch, permission),
    })
}

/// Rewrite context: identity of the account pushing to `remote` (what Push Guard compares).
fn rewrite_setup(
    st: &AppState,
    repo_id: i64,
    remote: &str,
) -> AppResult<(std::path::PathBuf, AccountContext)> {
    let root = repo_root(st, repo_id)?;
    let resolution = resolve_repo(st, repo_id, &root)?;
    let snap = Snapshot::load(&st.storage)?;
    let dir = Directory::from_snapshot(&snap);
    let identity = effective_identity(st, &root, &dir, resolution.for_remote(Some(remote)));
    Ok((root, identity_ctx(identity.as_ref())))
}

pub(crate) fn fix_authors_impl(
    st: &AppState,
    repo_id: i64,
    remote: &str,
    branch: &str,
    mode: FixAuthorMode,
) -> AppResult<Result<RewriteOutcome, OpError>> {
    let (root, ctx) = rewrite_setup(st, repo_id, remote)?;
    let result = PushRange::resolve(&st.git, &root, remote, branch)
        .and_then(|range| guard::fix_authors(&st.git, &root, &ctx, &range, mode));
    if let Ok(out) = &result {
        tracing::info!(repo = repo_id, backup = %out.backup_ref, rewritten = out.rewritten, "push guard: fixed authors");
    }
    Ok(result)
}

/// "Sửa tác giả" for unpushed commits of `branch` (checked out), as the identity that pushes
/// to `remote`. Creates a backup ref first; refuses when a commit is already on a remote.
#[tauri::command]
#[specta::specta]
pub async fn guard_fix_authors(
    app: AppHandle,
    repo_id: i64,
    remote: String,
    branch: String,
    mode: FixAuthorMode,
) -> AppResult<Outcome<RewriteOutcome>> {
    let result = blocking(app, move |st| {
        fix_authors_impl(st, repo_id, &remote, &branch, mode)
    })
    .await?;
    outcome(result)
}

/// "Chuyển sang LFS": `git lfs migrate import` of `paths` over the unpushed commits of
/// `branch` (checked out). Backup ref first; refuses when a commit is already on a remote.
#[tauri::command]
#[specta::specta]
pub async fn guard_lfs_migrate(
    app: AppHandle,
    repo_id: i64,
    remote: String,
    branch: String,
    paths: Vec<String>,
) -> AppResult<Outcome<RewriteOutcome>> {
    let result = blocking(app, move |st| {
        let (root, ctx) = rewrite_setup(st, repo_id, &remote)?;
        let result = PushRange::resolve(&st.git, &root, &remote, &branch)
            .and_then(|range| guard::lfs_migrate_import(&st.git, &root, &ctx, &range, &paths));
        if let Ok(out) = &result {
            tracing::info!(repo = repo_id, backup = %out.backup_ref, "push guard: lfs migrate import");
        }
        Ok(result)
    })
    .await?;
    outcome(result)
}

/// Undo a guard rewrite: restores the backup's branch to `backup_ref` (and backs up the
/// current tip first, so the undo can itself be undone).
#[tauri::command]
#[specta::specta]
pub async fn guard_undo(
    app: AppHandle,
    repo_id: i64,
    backup_ref: String,
) -> AppResult<Outcome<RestoreOutcome>> {
    let result = blocking(app, move |st| {
        let root = repo_root(st, repo_id)?;
        Ok(guard::restore_backup(&st.git, &root, &backup_ref, None))
    })
    .await?;
    outcome(result)
}

/// Backup refs of the repo (`refs/nanomgg/backup/*`), newest first.
#[tauri::command]
#[specta::specta]
pub async fn guard_backups(app: AppHandle, repo_id: i64) -> AppResult<Vec<BackupRef>> {
    blocking(app, move |st| {
        let root = repo_root(st, repo_id)?;
        guard::list_backups(&st.git, &root).map_err(|e| AppError::Git(e.to_string()))
    })
    .await
}

/// Deletes one backup ref.
#[tauri::command]
#[specta::specta]
pub async fn guard_backup_delete(
    app: AppHandle,
    repo_id: i64,
    backup_ref: String,
) -> AppResult<Outcome<()>> {
    let result = blocking(app, move |st| {
        let root = repo_root(st, repo_id)?;
        Ok(guard::delete_backup(&st.git, &root, &backup_ref))
    })
    .await?;
    outcome(result)
}

#[cfg(test)]
mod tests;
