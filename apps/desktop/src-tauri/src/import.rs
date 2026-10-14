//! "Import cấu hình có sẵn" (PLAN §4.2): first-run wizard backend.
//!
//! [`import_scan`] reads `~/.gitconfig` (+ includes), `~/.ssh/config` and the gh / glab configs
//! of the real environment with `nanomgg_import::scan` (read-only, tokens never returned).
//! [`import_apply`] turns the suggestions the user ticked into identities, rules and SSH key
//! imports (keys are **copied** into the app key store). gh/glab accounts are never created
//! here — they need a fresh sign-in, so they are handed back for the add-account flow.
//!
//! The wizard opens by itself only on first run: no account yet and the setting
//! `onboarding.importDone` not set ("Bỏ qua" / finishing sets it).

use std::path::{Path, PathBuf};

use nanomgg_import::cli_hosts::{CliTool, TokenStorage};
use nanomgg_import::gitconfig::{SuggestedIdentity, SuggestedRule};
use nanomgg_import::ssh_config::SuggestedKeyFile;
use nanomgg_import::{ImportWarning, ScanEnv, SourceRef, SuggestionNote};
use nanomgg_ssh_keys::KeyStore;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use zeroize::Zeroizing;

use crate::accounts::commands::{IdentityInput, RuleInput, identity_save_impl, rule_save_impl};
use crate::accounts::storage_err;
use crate::error::{AppError, AppResult};
use crate::ssh_keys::{SshImportResult, import_impl};
use crate::state::AppState;

/// Setting: the import wizard was completed or skipped.
pub const KEY_IMPORT_DONE: &str = "onboarding.importDone";

// ----- DTOs ----------------------------------------------------------------------------------

/// `nanomgg_import::ProviderKind` under a TypeScript name that does not clash with the accounts
/// model's `ProviderKind` (specta exports types by their Rust name).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ImportProviderKind {
    Github,
    GithubEnterprise,
    Gitlab,
    GitlabSelfHosted,
    Unknown,
}

impl From<nanomgg_import::ProviderKind> for ImportProviderKind {
    fn from(k: nanomgg_import::ProviderKind) -> Self {
        use nanomgg_import::ProviderKind as P;
        match k {
            P::Github => Self::Github,
            P::GithubEnterprise => Self::GithubEnterprise,
            P::Gitlab => Self::Gitlab,
            P::GitlabSelfHosted => Self::GitlabSelfHosted,
            P::Unknown => Self::Unknown,
        }
    }
}

/// `nanomgg_import::ssh_config::SuggestedSshHost` with [`ImportProviderKind`].
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportSshHost {
    pub alias: String,
    pub host_name: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_files: Vec<SuggestedKeyFile>,
    pub identities_only: Option<bool>,
    pub provider: ImportProviderKind,
    pub is_alias: bool,
    pub wildcard: bool,
    pub notes: Vec<SuggestionNote>,
    pub source: SourceRef,
}

/// `nanomgg_import::cli_hosts::SuggestedAccount` with [`ImportProviderKind`].
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportCliAccount {
    pub tool: CliTool,
    pub provider: ImportProviderKind,
    pub host: String,
    pub api_host: Option<String>,
    pub username: Option<String>,
    pub active: bool,
    pub git_protocol: Option<String>,
    pub token_storage: TokenStorage,
    pub notes: Vec<SuggestionNote>,
    pub source: SourceRef,
}

/// [`nanomgg_import::ImportReport`] as sent to the wizard.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub identities: Vec<SuggestedIdentity>,
    pub rules: Vec<SuggestedRule>,
    pub ssh_hosts: Vec<ImportSshHost>,
    pub accounts: Vec<ImportCliAccount>,
    pub warnings: Vec<ImportWarning>,
    pub scanned_files: Vec<String>,
}

impl From<nanomgg_import::ImportReport> for ImportReport {
    fn from(r: nanomgg_import::ImportReport) -> Self {
        Self {
            identities: r.identities,
            rules: r.rules,
            ssh_hosts: r
                .ssh_hosts
                .into_iter()
                .map(|h| ImportSshHost {
                    alias: h.alias,
                    host_name: h.host_name,
                    user: h.user,
                    port: h.port,
                    identity_files: h.identity_files,
                    identities_only: h.identities_only,
                    provider: h.provider.into(),
                    is_alias: h.is_alias,
                    wildcard: h.wildcard,
                    notes: h.notes,
                    source: h.source,
                })
                .collect(),
            accounts: r
                .accounts
                .into_iter()
                .map(|a| ImportCliAccount {
                    tool: a.tool,
                    provider: a.provider.into(),
                    host: a.host,
                    api_host: a.api_host,
                    username: a.username,
                    active: a.active,
                    git_protocol: a.git_protocol,
                    token_storage: a.token_storage,
                    notes: a.notes,
                    source: a.source,
                })
                .collect(),
            warnings: r.warnings,
            scanned_files: r.scanned_files,
        }
    }
}

/// Scan result plus what the app already knows (to pre-untick duplicates).
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportScan {
    pub report: ImportReport,
    /// Home directory that was scanned.
    pub home: String,
    /// Fingerprints of SSH keys already in the app.
    pub known_fingerprints: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IdentityChoice {
    pub account_id: String,
    pub name: String,
    pub email: String,
    #[serde(default)]
    #[specta(optional)]
    pub make_default: bool,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RuleChoice {
    pub account_id: String,
    #[serde(default)]
    #[specta(optional)]
    pub path_glob: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub host: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub owner_or_group: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub remote_regex: Option<String>,
    /// Identity of the same account to use (created by this apply or existing), by email.
    #[serde(default)]
    #[specta(optional)]
    pub identity_email: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeyChoice {
    /// Private key file (`IdentityFile`), copied into the app key store.
    pub path: String,
    /// `None` imports the key unassigned (an account can adopt it later).
    #[serde(default)]
    #[specta(optional)]
    pub account_id: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub passphrase: Option<String>,
}

/// A gh/glab account the user wants to sign in to again.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountChoice {
    pub host: String,
    #[serde(default)]
    #[specta(optional)]
    pub username: Option<String>,
}

/// What the user ticked in the wizard.
#[derive(Debug, Clone, Default, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportSelection {
    #[serde(default)]
    #[specta(optional)]
    pub identities: Vec<IdentityChoice>,
    #[serde(default)]
    #[specta(optional)]
    pub rules: Vec<RuleChoice>,
    #[serde(default)]
    #[specta(optional)]
    pub ssh_keys: Vec<KeyChoice>,
    #[serde(default)]
    #[specta(optional)]
    pub accounts: Vec<AccountChoice>,
    /// Set `onboarding.importDone`.
    #[serde(default)]
    #[specta(optional)]
    pub mark_done: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ApplyKind {
    Identity,
    Rule,
    SshKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ApplyStatus {
    Created,
    /// Already in the app; nothing changed.
    Existing,
    Failed,
}

/// Result of one ticked suggestion.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApplyItem {
    pub kind: ApplyKind,
    /// Email / rule condition / key path.
    pub label: String,
    pub status: ApplyStatus,
    /// Machine code for failures the UI translates: `passphraseRequired`, `wrongPassphrase`,
    /// `keyOfAnotherAccount`, `identityNotFound`, `error`.
    pub code: Option<String>,
    /// Technical detail (English).
    pub message: Option<String>,
    /// Id of the created / existing record.
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportApplyResult {
    pub items: Vec<ApplyItem>,
    /// gh/glab accounts to add through the sign-in flow (never created automatically).
    pub sign_in: Vec<AccountChoice>,
    pub import_done: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingStatus {
    pub import_done: bool,
    pub account_count: u32,
    /// First run: no account and the wizard was neither completed nor skipped.
    pub should_open_wizard: bool,
}

// ----- implementations -----------------------------------------------------------------------

pub(crate) fn onboarding_status_impl(st: &AppState) -> AppResult<OnboardingStatus> {
    let import_done = st
        .storage
        .get_setting::<bool>(KEY_IMPORT_DONE)?
        .unwrap_or(false);
    let account_count = st.storage.accounts().map_err(storage_err)?.len() as u32;
    Ok(OnboardingStatus {
        import_done,
        account_count,
        should_open_wizard: !import_done && account_count == 0,
    })
}

pub(crate) fn set_import_done_impl(st: &AppState, done: bool) -> AppResult<OnboardingStatus> {
    if done {
        st.storage.set_setting(KEY_IMPORT_DONE, &true)?;
    } else {
        st.storage.delete_setting(KEY_IMPORT_DONE)?;
    }
    onboarding_status_impl(st)
}

/// Scan with explicit locations (tests use temporary directories).
pub(crate) fn scan_impl(
    st: &AppState,
    home: &Path,
    appdata: Option<&Path>,
    env: &ScanEnv,
) -> AppResult<ImportScan> {
    let report = nanomgg_import::scan(home, appdata, env).into();
    let known_fingerprints = st
        .storage
        .ssh_keys(None)
        .map_err(storage_err)?
        .into_iter()
        .map(|k| k.fingerprint)
        .collect();
    Ok(ImportScan {
        report,
        home: home.to_string_lossy().into_owned(),
        known_fingerprints,
    })
}

fn trimmed(s: &Option<String>) -> Option<String> {
    s.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn failed(kind: ApplyKind, label: String, code: &str, message: impl Into<String>) -> ApplyItem {
    ApplyItem {
        kind,
        label,
        status: ApplyStatus::Failed,
        code: Some(code.to_owned()),
        message: Some(message.into()),
        id: None,
    }
}

fn ok(kind: ApplyKind, label: String, status: ApplyStatus, id: String) -> ApplyItem {
    ApplyItem {
        kind,
        label,
        status,
        code: None,
        message: None,
        id: Some(id),
    }
}

fn apply_identity(st: &AppState, c: &IdentityChoice) -> ApplyItem {
    let label = c.email.trim().to_owned();
    let existing = st
        .storage
        .identities(Some(&c.account_id))
        .map_err(storage_err)
        .map(|list| {
            list.into_iter()
                .find(|i| i.email.eq_ignore_ascii_case(c.email.trim()))
        });
    match existing {
        Ok(Some(i)) => return ok(ApplyKind::Identity, label, ApplyStatus::Existing, i.id),
        Ok(None) => {}
        Err(e) => return failed(ApplyKind::Identity, label, "error", e.to_string()),
    }
    match identity_save_impl(
        st,
        IdentityInput {
            id: None,
            account_id: c.account_id.clone(),
            name: c.name.clone(),
            email: c.email.clone(),
            signing: Default::default(),
            signing_key_ref: None,
            is_default: c.make_default,
        },
    ) {
        Ok(i) => ok(ApplyKind::Identity, label, ApplyStatus::Created, i.id),
        Err(e) => failed(ApplyKind::Identity, label, "error", e.to_string()),
    }
}

fn rule_label(c: &RuleChoice) -> String {
    let mut parts = Vec::new();
    if let Some(p) = trimmed(&c.path_glob) {
        parts.push(p);
    }
    match (trimmed(&c.host), trimmed(&c.owner_or_group)) {
        (Some(h), Some(o)) => parts.push(format!("{h}/{o}")),
        (Some(h), None) => parts.push(h),
        (None, Some(o)) => parts.push(o),
        (None, None) => {}
    }
    if parts.is_empty()
        && let Some(r) = trimmed(&c.remote_regex)
    {
        parts.push(r);
    }
    parts.join(" · ")
}

fn apply_rule(st: &AppState, c: &RuleChoice) -> ApplyItem {
    let label = rule_label(c);
    let identity_id = match trimmed(&c.identity_email) {
        Some(email) => match st.storage.identities(Some(&c.account_id)) {
            Ok(list) => match list
                .into_iter()
                .find(|i| i.email.eq_ignore_ascii_case(&email))
            {
                Some(i) => Some(i.id),
                None => {
                    return failed(
                        ApplyKind::Rule,
                        label,
                        "identityNotFound",
                        format!("no identity {email} on the account"),
                    );
                }
            },
            Err(e) => return failed(ApplyKind::Rule, label, "error", e.to_string()),
        },
        None => None,
    };
    let host = trimmed(&c.host).map(|h| h.to_ascii_lowercase());
    let (path_glob, owner, regex) = (
        trimmed(&c.path_glob),
        trimmed(&c.owner_or_group),
        trimmed(&c.remote_regex),
    );
    match st.storage.rules() {
        Ok(rules) => {
            if let Some(r) = rules.into_iter().find(|r| {
                r.account_id == c.account_id
                    && r.path_glob == path_glob
                    && r.host == host
                    && r.owner_or_group == owner
                    && r.remote_regex == regex
            }) {
                return ok(ApplyKind::Rule, label, ApplyStatus::Existing, r.id);
            }
        }
        Err(e) => return failed(ApplyKind::Rule, label, "error", e.to_string()),
    }
    match rule_save_impl(
        st,
        RuleInput {
            id: None,
            priority: None,
            host,
            owner_or_group: owner,
            path_glob,
            remote_regex: regex,
            account_id: c.account_id.clone(),
            identity_id,
            enabled: true,
        },
    ) {
        Ok(r) => ok(ApplyKind::Rule, label, ApplyStatus::Created, r.id),
        Err(e) => failed(ApplyKind::Rule, label, "error", e.to_string()),
    }
}

fn apply_key(st: &AppState, store: &KeyStore, c: &KeyChoice) -> ApplyItem {
    let label = c.path.clone();
    let account = trimmed(&c.account_id);
    let passphrase = c.passphrase.clone().map(Zeroizing::new);
    match import_impl(
        st,
        store,
        account.as_deref(),
        &PathBuf::from(&c.path),
        passphrase,
    ) {
        Ok(SshImportResult::Imported { key, .. }) => {
            ok(ApplyKind::SshKey, label, ApplyStatus::Created, key.id)
        }
        Ok(SshImportResult::AlreadyKnown { key_id, account_id }) => {
            if account.is_none() || account == account_id {
                ok(ApplyKind::SshKey, label, ApplyStatus::Existing, key_id)
            } else {
                failed(
                    ApplyKind::SshKey,
                    label,
                    "keyOfAnotherAccount",
                    "the key already belongs to another account",
                )
            }
        }
        Ok(SshImportResult::PassphraseRequired) => failed(
            ApplyKind::SshKey,
            label,
            "passphraseRequired",
            "the private key is protected by a passphrase",
        ),
        Ok(SshImportResult::WrongPassphrase) => failed(
            ApplyKind::SshKey,
            label,
            "wrongPassphrase",
            "wrong passphrase",
        ),
        Err(e) => failed(ApplyKind::SshKey, label, "error", e.to_string()),
    }
}

/// Applies every ticked suggestion; one failure does not stop the others.
pub(crate) fn apply_impl(
    st: &AppState,
    store: &KeyStore,
    selection: ImportSelection,
) -> AppResult<ImportApplyResult> {
    let mut items = Vec::new();
    for c in &selection.identities {
        items.push(apply_identity(st, c));
    }
    for c in &selection.rules {
        items.push(apply_rule(st, c));
    }
    for c in &selection.ssh_keys {
        items.push(apply_key(st, store, c));
    }
    if selection.mark_done {
        st.storage.set_setting(KEY_IMPORT_DONE, &true)?;
    }
    let import_done = onboarding_status_impl(st)?.import_done;
    tracing::info!(
        items = items.len(),
        created = items
            .iter()
            .filter(|i| i.status == ApplyStatus::Created)
            .count(),
        "import wizard applied"
    );
    Ok(ImportApplyResult {
        items,
        sign_in: selection.accounts,
        import_done,
    })
}

// ----- commands ------------------------------------------------------------------------------

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppHandle, &AppState) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || f(&app, &app.state::<AppState>()))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Scans the real environment: home, `%APPDATA%` (Windows), `XDG_CONFIG_HOME`,
/// `GIT_CONFIG_GLOBAL`, `GH_CONFIG_DIR`, `GLAB_CONFIG_DIR`. Read-only.
#[tauri::command]
#[specta::specta]
pub async fn import_scan(app: AppHandle) -> AppResult<ImportScan> {
    blocking(app, |app, st| {
        let home = app
            .path()
            .home_dir()
            .map_err(|e| AppError::Internal(e.to_string()))?;
        let appdata = if cfg!(windows) {
            std::env::var_os("APPDATA")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .or_else(|| app.path().config_dir().ok())
        } else {
            None
        };
        scan_impl(st, &home, appdata.as_deref(), &ScanEnv::from_process())
    })
    .await
}

/// Creates the ticked identities / rules / SSH key imports.
#[tauri::command]
#[specta::specta]
pub async fn import_apply(
    app: AppHandle,
    selection: ImportSelection,
) -> AppResult<ImportApplyResult> {
    blocking(app, move |app, st| {
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|e| AppError::Internal(e.to_string()))?;
        apply_impl(st, &KeyStore::new(&dir), selection)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn onboarding_status(app: AppHandle) -> AppResult<OnboardingStatus> {
    blocking(app, |_, st| onboarding_status_impl(st)).await
}

/// "Bỏ qua" / finish (`true`), or show the wizard again on next start (`false`).
#[tauri::command]
#[specta::specta]
pub async fn onboarding_set_import_done(app: AppHandle, done: bool) -> AppResult<OnboardingStatus> {
    blocking(app, move |_, st| set_import_done_impl(st, done)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::commands::{AccountInput, account_save_impl};
    use nanomgg_accounts::AuthMethod;
    use nanomgg_credentials::MemorySecretStore;
    use nanomgg_storage::Storage;
    use std::sync::Arc;

    fn state(tmp: &Path) -> AppState {
        let storage = Arc::new(Storage::open(&tmp.join("app.db")).unwrap());
        let rt = crate::accounts::AccountsRuntime::new(
            None,
            None,
            Arc::new(MemorySecretStore::default()),
        );
        AppState::with_accounts(storage, rt)
    }

    fn account(st: &AppState, user: &str) -> String {
        account_save_impl(
            st,
            AccountInput {
                id: None,
                provider_id: "github".into(),
                username: user.into(),
                display_name: None,
                avatar_url: None,
                auth_method: AuthMethod::Pat,
                scopes: vec![],
                token_expires_at: None,
                color: None,
                label: None,
                default_clone_dir: None,
            },
        )
        .unwrap()
        .id
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn onboarding_flag_and_first_run_detection() {
        let tmp = tempfile::tempdir().unwrap();
        let st = state(tmp.path());
        let s = onboarding_status_impl(&st).unwrap();
        assert!(s.should_open_wizard && !s.import_done && s.account_count == 0);
        let s = set_import_done_impl(&st, true).unwrap();
        assert!(s.import_done && !s.should_open_wizard);
        let s = set_import_done_impl(&st, false).unwrap();
        assert!(s.should_open_wizard);
        account(&st, "alice");
        let s = onboarding_status_impl(&st).unwrap();
        assert!(!s.should_open_wizard, "accounts exist → no auto-open");
    }

    #[test]
    fn scan_reads_temp_home_without_tokens() {
        let tmp = tempfile::tempdir().unwrap();
        let st = state(tmp.path());
        let home = tmp.path().join("home");
        write(
            &home.join(".gitconfig"),
            "[user]\n\tname = An\n\temail = an@personal.dev\n[includeIf \"gitdir:~/work/\"]\n\tpath = .gitconfig-work\n",
        );
        write(
            &home.join(".gitconfig-work"),
            "[user]\n\tname = An (ACME)\n\temail = an@acme.vn\n",
        );
        write(
            &home.join(".ssh").join("config"),
            "Host github.com-work\n  HostName github.com\n  IdentityFile ~/.ssh/id_work\n",
        );
        let appdata = tmp.path().join("appdata");
        write(
            &appdata.join("GitHub CLI").join("hosts.yml"),
            "github.com:\n    oauth_token: gho_SECRET_TOKEN_123\n    user: an-dev\n    git_protocol: ssh\n",
        );
        let env = ScanEnv {
            gh_config_dir: Some(appdata.join("GitHub CLI")),
            ..ScanEnv::default()
        };
        let scan = scan_impl(&st, &home, Some(&appdata), &env).unwrap();
        let r = &scan.report;
        assert!(
            r.identities
                .iter()
                .any(|i| i.email.as_deref() == Some("an@acme.vn"))
        );
        assert_eq!(r.rules.len(), 1);
        assert!(r.rules[0].path_glob.as_deref().unwrap().contains("work"));
        assert!(
            r.ssh_hosts
                .iter()
                .any(|h| h.alias == "github.com-work" && h.is_alias)
        );
        assert_eq!(r.accounts.len(), 1);
        assert_eq!(r.accounts[0].username.as_deref(), Some("an-dev"));
        let json = serde_json::to_string(&scan).unwrap();
        assert!(!json.contains("gho_SECRET_TOKEN_123"));
    }

    #[test]
    fn apply_creates_identities_rules_and_key_imports() {
        let tmp = tempfile::tempdir().unwrap();
        let st = state(tmp.path());
        let store = KeyStore::new(&tmp.path().join("appdata"));
        let work = account(&st, "an-acme");
        let personal = account(&st, "an-dev");

        let ssh = tmp.path().join("home").join(".ssh");
        std::fs::create_dir_all(&ssh).unwrap();
        let plain = nanomgg_ssh_keys::generate_ed25519("work", None).unwrap();
        std::fs::write(ssh.join("id_work"), plain.private_key.as_bytes()).unwrap();
        let locked = nanomgg_ssh_keys::generate_ed25519("locked", Some("pw")).unwrap();
        std::fs::write(ssh.join("id_locked"), locked.private_key.as_bytes()).unwrap();
        let original = std::fs::read(ssh.join("id_work")).unwrap();

        let selection = ImportSelection {
            identities: vec![
                IdentityChoice {
                    account_id: work.clone(),
                    name: "An (ACME)".into(),
                    email: "an@acme.vn".into(),
                    make_default: true,
                },
                IdentityChoice {
                    account_id: personal.clone(),
                    name: "An".into(),
                    email: "an@personal.dev".into(),
                    make_default: false,
                },
            ],
            rules: vec![
                RuleChoice {
                    account_id: work.clone(),
                    path_glob: Some("C:/Users/an/work/**".into()),
                    host: None,
                    owner_or_group: None,
                    remote_regex: None,
                    identity_email: Some("AN@acme.vn".into()),
                },
                RuleChoice {
                    account_id: work.clone(),
                    path_glob: Some("D:/Acme/**".into()),
                    host: None,
                    owner_or_group: None,
                    remote_regex: None,
                    identity_email: Some("nobody@acme.vn".into()),
                },
            ],
            ssh_keys: vec![
                KeyChoice {
                    path: ssh.join("id_work").to_string_lossy().into_owned(),
                    account_id: Some(work.clone()),
                    passphrase: None,
                },
                KeyChoice {
                    path: ssh.join("id_locked").to_string_lossy().into_owned(),
                    account_id: None,
                    passphrase: None,
                },
            ],
            accounts: vec![AccountChoice {
                host: "github.com".into(),
                username: Some("an-dev".into()),
            }],
            mark_done: true,
        };
        let r = apply_impl(&st, &store, selection.clone()).unwrap();
        let statuses: Vec<_> = r.items.iter().map(|i| (i.kind, i.status)).collect();
        assert_eq!(
            statuses,
            [
                (ApplyKind::Identity, ApplyStatus::Created),
                (ApplyKind::Identity, ApplyStatus::Created),
                (ApplyKind::Rule, ApplyStatus::Created),
                (ApplyKind::Rule, ApplyStatus::Failed),
                (ApplyKind::SshKey, ApplyStatus::Created),
                (ApplyKind::SshKey, ApplyStatus::Failed),
            ]
        );
        assert_eq!(r.items[3].code.as_deref(), Some("identityNotFound"));
        assert_eq!(r.items[5].code.as_deref(), Some("passphraseRequired"));
        assert!(r.import_done);
        assert_eq!(r.sign_in.len(), 1);

        let ids = st.storage.identities(Some(&work)).unwrap();
        assert_eq!(ids.len(), 1);
        let rules = st.storage.rules().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].identity_id.as_deref(), Some(ids[0].id.as_str()));
        let keys = st.storage.ssh_keys(Some(&work)).unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].fingerprint, plain.info.fingerprint);
        assert!(
            Path::new(keys[0].private_key_path.as_deref().unwrap()).starts_with(store.dir()),
            "copied into the app store"
        );
        assert_eq!(std::fs::read(ssh.join("id_work")).unwrap(), original);
        assert_eq!(
            crate::ssh_keys::default_key_path(&st.storage, &work).unwrap(),
            keys[0].private_key_path.as_deref().map(PathBuf::from)
        );

        // Applying the same selection again changes nothing.
        let again = apply_impl(&st, &store, selection).unwrap();
        let statuses: Vec<_> = again.items.iter().map(|i| i.status).collect();
        assert_eq!(
            statuses,
            [
                ApplyStatus::Existing,
                ApplyStatus::Existing,
                ApplyStatus::Existing,
                ApplyStatus::Failed,
                ApplyStatus::Existing,
                ApplyStatus::Failed,
            ]
        );
        assert_eq!(st.storage.rules().unwrap().len(), 1);
        assert_eq!(st.storage.ssh_keys(None).unwrap().len(), 1);
    }
}
