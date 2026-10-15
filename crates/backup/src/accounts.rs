//! Account configuration export/import between machines (Phase 6.3, `.nanomgg-accounts`).
//!
//! The file carries providers (self-hosted ones with their URLs and OAuth client id), accounts,
//! identities, rules and SSH key metadata. Secrets (token, refresh token, SSH private keys of
//! the app key store and their passphrases) are only included on request.
//!
//! Importing never overwrites: every record gets a new id; an account that already exists here
//! (same provider host + user name) is skipped, or only gets its label/color updated. A failure
//! undoes everything written by that import (rows, keychain entries, key files).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use nanomgg_accounts::{
    Account, AccountSecret, AuthMethod, Identity, Provider, ProviderKind, Rule, SigningMethod,
    SshKey, new_id,
};
use nanomgg_credentials::keychain::normalize_key_path;
use nanomgg_credentials::{SecretKey, SecretKind, SecretStore};
use nanomgg_ssh_keys::KeyStore;
use nanomgg_storage::Storage;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::error::{BackupError, Result};

/// Settings keyed by account / provider id (mirrors the app: `ssh_keys.rs`, `auth/service.rs`).
pub const SSH_DEFAULT_PREFIX: &str = "ssh.defaultKey.";
pub const AUTH_STATUS_PREFIX: &str = "auth.status.";
pub const LOOPBACK_PORT_PREFIX: &str = "auth.loopbackPort.";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsPayload {
    pub providers: Vec<ProviderEntry>,
    pub accounts: Vec<AccountEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEntry {
    pub provider: Provider,
    /// GitLab OAuth loopback port chosen for this provider (`auth.loopbackPort.<id>`).
    #[serde(default)]
    pub loopback_port: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountEntry {
    /// As stored on the source machine (`id`/`providerId` are only references inside the file).
    pub account: Account,
    pub identities: Vec<Identity>,
    pub rules: Vec<Rule>,
    pub ssh_keys: Vec<SshKeyEntry>,
    #[serde(default)]
    pub default_ssh_key: Option<String>,
    /// `auth.status.<id>` (token kind, scopes known…), applied together with the token.
    #[serde(default)]
    pub auth_status: Option<serde_json::Value>,
    /// Present only when the user chose to include sign-in data.
    #[serde(default)]
    pub secrets: Option<AccountSecrets>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSecrets {
    #[serde(default)]
    pub token: Option<Zeroizing<String>>,
    #[serde(default)]
    pub refresh_token: Option<Zeroizing<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshKeyEntry {
    /// Metadata (fingerprint, public key, source path…).
    pub key: SshKey,
    /// OpenSSH private key text (app key store only, with secrets).
    #[serde(default)]
    pub private_key: Option<Zeroizing<String>>,
    #[serde(default)]
    pub public_key_file: Option<String>,
    #[serde(default)]
    pub passphrase: Option<Zeroizing<String>>,
}

fn secret_kind(kind: AccountSecret) -> SecretKind {
    match kind {
        AccountSecret::Token => SecretKind::Token,
        AccountSecret::RefreshToken => SecretKind::RefreshToken,
    }
}

/// The private key path is the app key store's file for this key id.
fn is_managed(store: &KeyStore, key: &SshKey) -> bool {
    match (
        key.private_key_path.as_deref(),
        store.private_key_path(&key.id),
    ) {
        (Some(path), Ok(expected)) => {
            normalize_key_path(path) == normalize_key_path(&expected.to_string_lossy())
        }
        _ => false,
    }
}

/// Builds the payload for `account_ids` (empty = every account).
pub fn export(
    storage: &Storage,
    secrets: &dyn SecretStore,
    store: &KeyStore,
    account_ids: &[String],
    include_secrets: bool,
) -> Result<AccountsPayload> {
    let accounts: Vec<Account> = storage
        .accounts()?
        .into_iter()
        .filter(|a| account_ids.is_empty() || account_ids.contains(&a.id))
        .collect();
    if accounts.is_empty() {
        return Err(BackupError::Invalid("no account to export".into()));
    }
    let rules = storage.rules()?;
    let mut providers: Vec<ProviderEntry> = Vec::new();
    let mut entries = Vec::with_capacity(accounts.len());
    for account in accounts {
        if !providers
            .iter()
            .any(|p| p.provider.id == account.provider_id)
        {
            let provider = storage.provider(&account.provider_id)?.ok_or_else(|| {
                BackupError::Corrupt(format!("provider `{}` is missing", account.provider_id))
            })?;
            let loopback_port =
                storage.get_setting(&format!("{LOOPBACK_PORT_PREFIX}{}", provider.id))?;
            providers.push(ProviderEntry {
                provider,
                loopback_port,
            });
        }
        let mut ssh_keys = Vec::new();
        for key in storage.ssh_keys(Some(&account.id))? {
            let mut entry = SshKeyEntry {
                key,
                private_key: None,
                public_key_file: None,
                passphrase: None,
            };
            if include_secrets
                && is_managed(store, &entry.key)
                && let Ok(text) = store.read_private_key(&entry.key.id)
            {
                entry.private_key = Some(text);
                entry.public_key_file = store
                    .public_key_path(&entry.key.id)
                    .ok()
                    .and_then(|p| std::fs::read_to_string(p).ok());
                if entry.key.has_passphrase {
                    entry.passphrase = secrets.get(&SecretKey::ssh_passphrase(&entry.key.id))?;
                }
            }
            ssh_keys.push(entry);
        }
        let secrets = if include_secrets {
            Some(AccountSecrets {
                token: secrets.get(&SecretKey::account(&account.id, SecretKind::Token))?,
                refresh_token: secrets
                    .get(&SecretKey::account(&account.id, SecretKind::RefreshToken))?,
            })
        } else {
            None
        };
        entries.push(AccountEntry {
            identities: storage.identities(Some(&account.id))?,
            rules: rules
                .iter()
                .filter(|r| r.account_id == account.id)
                .cloned()
                .collect(),
            ssh_keys,
            default_ssh_key: storage.get_setting(&format!("{SSH_DEFAULT_PREFIX}{}", account.id))?,
            auth_status: storage.get_setting(&format!("{AUTH_STATUS_PREFIX}{}", account.id))?,
            secrets,
            account,
        });
    }
    Ok(AccountsPayload {
        providers,
        accounts: entries,
    })
}

// ----- preview -------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct AccountsPreview {
    pub accounts: Vec<AccountPreview>,
    /// At least one account carries a token.
    pub has_secrets: bool,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct AccountPreview {
    /// Id of the account inside the file (what the import selection names).
    pub key: String,
    pub username: String,
    pub display_name: Option<String>,
    pub label: Option<String>,
    pub color: String,
    pub provider_name: String,
    pub provider_kind: ProviderKind,
    pub host: String,
    pub auth_method: AuthMethod,
    pub identities: u32,
    pub rules: u32,
    pub ssh_keys: u32,
    /// SSH private keys included in the file.
    pub ssh_key_files: u32,
    pub has_token: bool,
    /// Same provider host + user name already exists on this machine.
    pub existing_account_id: Option<String>,
    /// The provider (self-hosted instance) is not configured here yet.
    pub new_provider: bool,
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn url_key(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_ascii_lowercase()
}

/// `(host, user name)` in comparison form.
fn account_key(provider: &Provider, username: &str) -> Option<(String, String)> {
    Some((
        provider.host()?.to_ascii_lowercase(),
        username.trim().to_ascii_lowercase(),
    ))
}

/// Accounts already on this machine by `(host, user name)`.
fn existing_accounts(storage: &Storage) -> Result<HashMap<(String, String), String>> {
    let providers: HashMap<String, Provider> = storage
        .providers()?
        .into_iter()
        .map(|p| (p.id.clone(), p))
        .collect();
    Ok(storage
        .accounts()?
        .into_iter()
        .filter_map(|a| {
            let key = account_key(providers.get(&a.provider_id)?, &a.username)?;
            Some((key, a.id))
        })
        .collect())
}

fn provider_of<'a>(payload: &'a AccountsPayload, entry: &AccountEntry) -> Result<&'a Provider> {
    payload
        .providers
        .iter()
        .map(|p| &p.provider)
        .find(|p| p.id == entry.account.provider_id)
        .ok_or_else(|| {
            BackupError::Corrupt(format!(
                "provider `{}` is missing from the file",
                entry.account.provider_id
            ))
        })
}

fn has_token(entry: &AccountEntry) -> bool {
    entry
        .secrets
        .as_ref()
        .is_some_and(|s| s.token.as_ref().is_some_and(|t| !t.trim().is_empty()))
}

pub fn preview(storage: &Storage, payload: &AccountsPayload) -> Result<AccountsPreview> {
    let existing = existing_accounts(storage)?;
    let known_urls: HashSet<String> = storage
        .providers()?
        .iter()
        .map(|p| url_key(&p.base_url))
        .collect();
    let mut accounts = Vec::with_capacity(payload.accounts.len());
    for entry in &payload.accounts {
        let provider = provider_of(payload, entry)?;
        let a = &entry.account;
        accounts.push(AccountPreview {
            key: a.id.clone(),
            username: a.username.clone(),
            display_name: a.display_name.clone(),
            label: a.label.clone(),
            color: a.color.clone(),
            provider_name: provider.name.clone(),
            provider_kind: provider.kind,
            host: provider.host().unwrap_or_else(|| provider.base_url.clone()),
            auth_method: a.auth_method,
            identities: count(entry.identities.len()),
            rules: count(entry.rules.len()),
            ssh_keys: count(entry.ssh_keys.len()),
            ssh_key_files: count(
                entry
                    .ssh_keys
                    .iter()
                    .filter(|k| k.private_key.is_some())
                    .count(),
            ),
            has_token: has_token(entry),
            existing_account_id: account_key(provider, &a.username)
                .and_then(|k| existing.get(&k).cloned()),
            new_provider: !known_urls.contains(&url_key(&provider.base_url)),
        });
    }
    Ok(AccountsPreview {
        has_secrets: payload.accounts.iter().any(has_token),
        accounts,
    })
}

// ----- import --------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct AccountsImportReport {
    pub added: u32,
    /// Existing accounts whose label/color were updated.
    pub updated: u32,
    /// Selected accounts that already existed (left untouched).
    pub skipped: u32,
    pub providers_added: u32,
    /// New accounts whose token came from the file.
    pub tokens: u32,
    pub ssh_keys: u32,
    /// New accounts without a token: they must sign in again (ids on this machine).
    pub need_sign_in: Vec<String>,
    /// Identities whose SSH signing key does not exist here: signing was turned off.
    pub signing_disabled: u32,
}

/// What to undo when an import fails half-way (applied in reverse).
enum Undo {
    Provider(String),
    Account(String),
    Restore(Box<Account>),
    KeyFiles(String),
    KeyRow(String),
    Secret(SecretKey),
    Setting(String),
}

struct Importer<'a> {
    storage: &'a Storage,
    secrets: &'a dyn SecretStore,
    store: &'a KeyStore,
    undo: Vec<Undo>,
    report: AccountsImportReport,
}

impl Importer<'_> {
    fn rollback(&mut self) {
        while let Some(step) = self.undo.pop() {
            let result: Result<()> = match &step {
                Undo::Provider(id) => self
                    .storage
                    .delete_provider(id)
                    .map(drop)
                    .map_err(Into::into),
                Undo::Account(id) => self
                    .storage
                    .delete_account(id)
                    .map(drop)
                    .map_err(Into::into),
                Undo::Restore(a) => self.storage.update_account(a).map(drop).map_err(Into::into),
                Undo::KeyFiles(id) => self.store.remove(id).map(drop).map_err(Into::into),
                Undo::KeyRow(id) => self
                    .storage
                    .delete_ssh_key(id)
                    .map(drop)
                    .map_err(Into::into),
                Undo::Secret(key) => self.secrets.delete(key).map(drop).map_err(Into::into),
                Undo::Setting(key) => self
                    .storage
                    .delete_setting(key)
                    .map(drop)
                    .map_err(Into::into),
            };
            if let Err(e) = result {
                tracing::warn!(error = %e, "could not undo part of a failed account import");
            }
        }
    }

    fn set_setting(&mut self, key: String, value: &serde_json::Value) -> Result<()> {
        self.storage.set_setting(&key, value)?;
        self.undo.push(Undo::Setting(key));
        Ok(())
    }

    fn set_secret(&mut self, key: SecretKey, value: &str) -> Result<()> {
        self.secrets.set(&key, value)?;
        self.undo.push(Undo::Secret(key));
        Ok(())
    }

    /// Provider of this machine with the same base URL, else a new one.
    fn provider_id(
        &mut self,
        entry: &ProviderEntry,
        by_url: &mut HashMap<String, String>,
    ) -> Result<String> {
        let url = url_key(&entry.provider.base_url);
        if let Some(id) = by_url.get(&url) {
            return Ok(id.clone());
        }
        let mut provider = entry.provider.clone();
        provider.id = new_id("prv");
        provider.builtin = false;
        let saved = self.storage.insert_provider(&provider)?;
        self.undo.push(Undo::Provider(saved.id.clone()));
        if let Some(port) = &entry.loopback_port {
            self.set_setting(format!("{LOOPBACK_PORT_PREFIX}{}", saved.id), port)?;
        }
        self.report.providers_added += 1;
        by_url.insert(url, saved.id.clone());
        Ok(saved.id)
    }

    fn update_existing(&mut self, id: &str, from: &Account) -> Result<()> {
        let Some(current) = self.storage.account(id)? else {
            return Ok(());
        };
        let mut next = current.clone();
        next.color = from.color.clone();
        if from.label.is_some() {
            next.label = from.label.clone();
        }
        if next != current {
            self.storage.update_account(&next)?;
            self.undo.push(Undo::Restore(Box::new(current)));
        }
        self.report.updated += 1;
        Ok(())
    }

    /// Copies the entry's SSH keys that come with their private key; returns old → new
    /// `(key id, private path, public path)` for remapping references.
    fn import_keys(
        &mut self,
        account_id: &str,
        entry: &AccountEntry,
    ) -> Result<Vec<(String, String, String, String)>> {
        let known: HashSet<String> = self
            .storage
            .ssh_keys(None)?
            .into_iter()
            .map(|k| k.fingerprint)
            .collect();
        let mut mapped = Vec::new();
        for k in &entry.ssh_keys {
            let Some(private) = &k.private_key else {
                continue;
            };
            if known.contains(&k.key.fingerprint) {
                continue;
            }
            let id = new_id("key");
            let stored =
                self.store
                    .save(&id, private, k.public_key_file.as_deref().map(str::trim))?;
            self.undo.push(Undo::KeyFiles(id.clone()));
            let row = SshKey {
                id: id.clone(),
                account_id: Some(account_id.to_owned()),
                private_key_path: Some(stored.private_key_path.clone()),
                ..k.key.clone()
            };
            self.storage.insert_ssh_key(&row)?;
            self.undo.push(Undo::KeyRow(id.clone()));
            if let Some(pass) = k.passphrase.as_ref().filter(|p| !p.is_empty()) {
                self.set_secret(SecretKey::ssh_passphrase(&id), pass)?;
            }
            self.report.ssh_keys += 1;
            let old_private = k.key.private_key_path.clone().unwrap_or_default();
            mapped.push((k.key.id.clone(), id, old_private, stored.private_key_path));
        }
        Ok(mapped)
    }

    fn import_account(&mut self, provider_id: String, entry: &AccountEntry) -> Result<String> {
        let a = &entry.account;
        let account = Account {
            id: new_id("acc"),
            provider_id,
            secrets: Vec::new(),
            sort_order: 0,
            created_at: 0,
            updated_at: 0,
            ..a.clone()
        };
        let account = self.storage.insert_account(&account)?;
        self.undo.push(Undo::Account(account.id.clone()));

        let keys = self.import_keys(&account.id, entry)?;
        if let Some(old) = &entry.default_ssh_key
            && let Some((_, new, _, _)) = keys.iter().find(|(o, ..)| o == old)
        {
            self.set_setting(
                format!("{SSH_DEFAULT_PREFIX}{}", account.id),
                &serde_json::Value::String(new.clone()),
            )?;
        }

        let mut identity_ids: HashMap<&str, String> = HashMap::new();
        for i in &entry.identities {
            let (signing, signing_key_ref) = self.signing_for(i, &keys);
            let saved = self.storage.insert_identity(&Identity {
                id: new_id("idn"),
                account_id: account.id.clone(),
                signing,
                signing_key_ref,
                created_at: 0,
                ..i.clone()
            })?;
            identity_ids.insert(&i.id, saved.id);
        }

        let mut rules: Vec<&Rule> = entry.rules.iter().collect();
        rules.sort_by_key(|r| (r.priority, r.created_at));
        for r in rules {
            self.storage.insert_rule(&Rule {
                id: new_id("rul"),
                priority: self.storage.next_rule_priority()?,
                account_id: account.id.clone(),
                identity_id: r
                    .identity_id
                    .as_deref()
                    .and_then(|old| identity_ids.get(old).cloned()),
                created_at: 0,
                ..r.clone()
            })?;
        }

        let mut stored = Vec::new();
        if let Some(s) = &entry.secrets {
            for (kind, value) in [
                (AccountSecret::Token, &s.token),
                (AccountSecret::RefreshToken, &s.refresh_token),
            ] {
                if let Some(v) = value.as_ref().map(|v| v.trim()).filter(|v| !v.is_empty()) {
                    self.set_secret(SecretKey::account(&account.id, secret_kind(kind)), v)?;
                    stored.push(kind);
                }
            }
        }
        if stored.contains(&AccountSecret::Token) {
            self.storage.set_account_secrets(&account.id, &stored)?;
            if let Some(status) = &entry.auth_status {
                self.set_setting(format!("{AUTH_STATUS_PREFIX}{}", account.id), status)?;
            }
            self.report.tokens += 1;
        } else {
            // A refresh token alone cannot be used: forget it, sign in again.
            for kind in stored {
                let key = SecretKey::account(&account.id, secret_kind(kind));
                self.secrets.delete(&key)?;
            }
            if account.auth_method != AuthMethod::SshOnly {
                self.report.need_sign_in.push(account.id.clone());
            }
        }
        self.report.added += 1;
        Ok(account.id)
    }

    /// SSH signing keys move with their key; a key path that does not exist here turns
    /// signing off rather than breaking every commit.
    fn signing_for(
        &mut self,
        identity: &Identity,
        keys: &[(String, String, String, String)],
    ) -> (SigningMethod, Option<String>) {
        let (signing, reference) = (identity.signing, identity.signing_key_ref.clone());
        let Some(r) = reference
            .as_deref()
            .filter(|_| signing == SigningMethod::Ssh)
        else {
            return (signing, reference);
        };
        if r.starts_with("key::") {
            return (signing, reference);
        }
        let wanted = normalize_key_path(r);
        for (_, _, old_private, new_private) in keys {
            if old_private.is_empty() {
                continue;
            }
            if wanted == normalize_key_path(old_private) {
                return (signing, Some(new_private.clone()));
            }
            if wanted == normalize_key_path(&format!("{old_private}.pub")) {
                return (signing, Some(format!("{new_private}.pub")));
            }
        }
        if Path::new(r).is_file() {
            return (signing, reference);
        }
        self.report.signing_disabled += 1;
        (SigningMethod::None, None)
    }
}

/// Imports the accounts of `payload` whose file id is in `selected`. All or nothing.
pub fn import(
    storage: &Storage,
    secrets: &dyn SecretStore,
    store: &KeyStore,
    payload: &AccountsPayload,
    selected: &[String],
    update_existing: bool,
) -> Result<AccountsImportReport> {
    let mut existing = existing_accounts(storage)?;
    let mut by_url: HashMap<String, String> = storage
        .providers()?
        .into_iter()
        .map(|p| (url_key(&p.base_url), p.id))
        .collect();
    let mut importer = Importer {
        storage,
        secrets,
        store,
        undo: Vec::new(),
        report: AccountsImportReport::default(),
    };
    let result = (|| -> Result<()> {
        for entry in payload
            .accounts
            .iter()
            .filter(|e| selected.contains(&e.account.id))
        {
            let provider = provider_of(payload, entry)?;
            let key = account_key(provider, &entry.account.username).ok_or_else(|| {
                BackupError::Corrupt(format!("invalid provider URL `{}`", provider.base_url))
            })?;
            if let Some(id) = existing.get(&key).cloned() {
                if update_existing {
                    importer.update_existing(&id, &entry.account)?;
                } else {
                    importer.report.skipped += 1;
                }
                continue;
            }
            let provider_entry = payload
                .providers
                .iter()
                .find(|p| p.provider.id == provider.id)
                .ok_or_else(|| BackupError::Corrupt("provider missing".into()))?;
            let provider_id = importer.provider_id(provider_entry, &mut by_url)?;
            let id = importer.import_account(provider_id, entry)?;
            existing.insert(key, id);
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(importer.report),
        Err(e) => {
            importer.rollback();
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests;
