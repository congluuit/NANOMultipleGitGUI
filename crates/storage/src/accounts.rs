//! Typed CRUD for the multi-account tables (schema v3).
//!
//! Every write validates the record with `nanomgg-accounts` first; constraint violations
//! (duplicate account, identity of another account, provider still in use…) come back as
//! [`StorageError::Conflict`]. Timestamps are set here: `created_at` on insert, `updated_at`
//! on every write. Secrets never pass through this module.

use std::time::{SystemTime, UNIX_EPOCH};

use nanomgg_accounts::{
    Account, AccountSecret, AuthMethod, BindingSource, Identity, Provider, ProviderKind,
    RepoBinding, Rule, SigningMethod, SshKey, Transport, Workspace,
};
use rusqlite::{Connection, ErrorCode, OptionalExtension, Row, Transaction, params};

use crate::{Result, Storage, StorageError};

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Turns constraint violations into [`StorageError::Conflict`] with a readable message.
fn conflict(e: rusqlite::Error) -> StorageError {
    if let rusqlite::Error::SqliteFailure(f, msg) = &e
        && f.code == ErrorCode::ConstraintViolation
    {
        let raw = msg.clone().unwrap_or_default();
        let friendly = if raw.contains("accounts.provider_id, accounts.username") {
            "this account is already added".to_owned()
        } else if raw.contains("identities.account_id, identities.email") {
            "this email is already an identity of the account".to_owned()
        } else if raw.contains("providers.base_url") {
            "a provider with this URL already exists".to_owned()
        } else if raw.contains("ssh_keys.fingerprint") {
            "this SSH key is already known".to_owned()
        } else if raw.contains("workspaces.name") {
            "a workspace with this name already exists".to_owned()
        } else if raw.contains("FOREIGN KEY") {
            "the record is referenced by, or refers to, a missing or protected record".to_owned()
        } else {
            raw
        };
        return StorageError::Conflict(friendly);
    }
    StorageError::Sqlite(e)
}

fn not_found(entity: &'static str, id: impl Into<String>) -> StorageError {
    StorageError::NotFound {
        entity,
        id: id.into(),
    }
}

fn enum_col<T>(idx: usize, raw: String, parse: fn(&str) -> Option<T>) -> rusqlite::Result<T> {
    parse(&raw).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            idx,
            rusqlite::types::Type::Text,
            format!("unknown value `{raw}`").into(),
        )
    })
}

fn json_col<T: serde::de::DeserializeOwned>(idx: usize, raw: String) -> rusqlite::Result<T> {
    serde_json::from_str(&raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn to_json<T: serde::Serialize>(key: &str, value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|source| StorageError::Encode {
        key: key.to_owned(),
        source,
    })
}

const PROVIDER_COLS: &str = "id, kind, name, base_url, api_url, oauth_client_id, \
     large_file_warn_mb, large_file_block_mb, builtin";

fn provider_row(r: &Row<'_>) -> rusqlite::Result<Provider> {
    Ok(Provider {
        id: r.get(0)?,
        kind: enum_col(1, r.get(1)?, ProviderKind::parse)?,
        name: r.get(2)?,
        base_url: r.get(3)?,
        api_url: r.get(4)?,
        oauth_client_id: r.get(5)?,
        large_file_warn_mb: r.get(6)?,
        large_file_block_mb: r.get(7)?,
        builtin: r.get(8)?,
    })
}

const ACCOUNT_COLS: &str = "id, provider_id, username, display_name, avatar_url, auth_method, \
     scopes, token_expires_at, color, label, default_clone_dir, secrets, sort_order, created_at, \
     updated_at";

fn account_row(r: &Row<'_>) -> rusqlite::Result<Account> {
    Ok(Account {
        id: r.get(0)?,
        provider_id: r.get(1)?,
        username: r.get(2)?,
        display_name: r.get(3)?,
        avatar_url: r.get(4)?,
        auth_method: enum_col(5, r.get(5)?, AuthMethod::parse)?,
        scopes: json_col(6, r.get(6)?)?,
        token_expires_at: r.get(7)?,
        color: r.get(8)?,
        label: r.get(9)?,
        default_clone_dir: r.get(10)?,
        secrets: json_col::<Vec<String>>(11, r.get(11)?)?
            .iter()
            .filter_map(|s| AccountSecret::parse(s))
            .collect(),
        sort_order: r.get(12)?,
        created_at: r.get(13)?,
        updated_at: r.get(14)?,
    })
}

const IDENTITY_COLS: &str =
    "id, account_id, name, email, signing, signing_key_ref, is_default, created_at";

fn identity_row(r: &Row<'_>) -> rusqlite::Result<Identity> {
    Ok(Identity {
        id: r.get(0)?,
        account_id: r.get(1)?,
        name: r.get(2)?,
        email: r.get(3)?,
        signing: enum_col(4, r.get(4)?, SigningMethod::parse)?,
        signing_key_ref: r.get(5)?,
        is_default: r.get(6)?,
        created_at: r.get(7)?,
    })
}

const SSH_KEY_COLS: &str = "id, account_id, key_type, fingerprint, public_key, private_key_path, \
     has_passphrase, uploaded_to_provider, provider_key_id, comment, created_at";

fn ssh_key_row(r: &Row<'_>) -> rusqlite::Result<SshKey> {
    Ok(SshKey {
        id: r.get(0)?,
        account_id: r.get(1)?,
        key_type: r.get(2)?,
        fingerprint: r.get(3)?,
        public_key: r.get(4)?,
        private_key_path: r.get(5)?,
        has_passphrase: r.get(6)?,
        uploaded_to_provider: r.get(7)?,
        provider_key_id: r.get(8)?,
        comment: r.get(9)?,
        created_at: r.get(10)?,
    })
}

const RULE_COLS: &str = "id, priority, host, owner_or_group, path_glob, remote_regex, account_id, \
     identity_id, enabled, created_at";

fn rule_row(r: &Row<'_>) -> rusqlite::Result<Rule> {
    Ok(Rule {
        id: r.get(0)?,
        priority: r.get(1)?,
        host: r.get(2)?,
        owner_or_group: r.get(3)?,
        path_glob: r.get(4)?,
        remote_regex: r.get(5)?,
        account_id: r.get(6)?,
        identity_id: r.get(7)?,
        enabled: r.get(8)?,
        created_at: r.get(9)?,
    })
}

const BINDING_COLS: &str =
    "repo_id, remote_name, account_id, identity_id, transport, source, locked, updated_at";

fn binding_row(r: &Row<'_>) -> rusqlite::Result<RepoBinding> {
    let remote: String = r.get(1)?;
    let transport: Option<String> = r.get(4)?;
    Ok(RepoBinding {
        repo_id: r.get(0)?,
        remote_name: (!remote.is_empty()).then_some(remote),
        account_id: r.get(2)?,
        identity_id: r.get(3)?,
        transport: transport
            .map(|t| enum_col(4, t, Transport::parse))
            .transpose()?,
        source: enum_col(5, r.get(5)?, BindingSource::parse)?,
        locked: r.get(6)?,
        updated_at: r.get(7)?,
    })
}

fn query_all<T>(
    conn: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
    f: fn(&Row<'_>) -> rusqlite::Result<T>,
) -> Result<Vec<T>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params, f)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn query_one<T>(
    conn: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
    f: fn(&Row<'_>) -> rusqlite::Result<T>,
) -> Result<Option<T>> {
    Ok(conn.query_row(sql, params, f).optional()?)
}

fn invalid(e: nanomgg_accounts::ValidationError) -> StorageError {
    StorageError::Invalid(e)
}

fn workspace_repo_ids(conn: &Connection, id: &str) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT repo_id FROM workspace_repos WHERE workspace_id = ?1 ORDER BY position, repo_id",
    )?;
    let rows = stmt.query_map(params![id], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn write_workspace_repos(tx: &Transaction<'_>, id: &str, repo_ids: &[i64]) -> Result<()> {
    tx.execute(
        "DELETE FROM workspace_repos WHERE workspace_id = ?1",
        params![id],
    )?;
    for (pos, repo) in repo_ids.iter().enumerate() {
        tx.execute(
            "INSERT OR IGNORE INTO workspace_repos (workspace_id, repo_id, position) VALUES (?1, ?2, ?3)",
            params![id, repo, pos as i64],
        )
        .map_err(conflict)?;
    }
    Ok(())
}

/// Assigns `priority`/`sort_order` 0.. to `ids` in order; rows not listed keep their relative
/// order after them.
fn reorder(tx: &Transaction<'_>, table: &str, column: &str, ids: &[String]) -> Result<()> {
    let existing: Vec<String> = {
        let mut stmt = tx.prepare(&format!(
            "SELECT id FROM {table} ORDER BY {column}, created_at, id"
        ))?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for id in ids {
        if !existing.contains(id) {
            return Err(not_found(
                if table == "rules" { "rule" } else { "account" },
                id.clone(),
            ));
        }
    }
    let order = ids
        .iter()
        .chain(existing.iter().filter(|id| !ids.contains(id)));
    for (pos, id) in order.enumerate() {
        tx.execute(
            &format!("UPDATE {table} SET {column} = ?1 WHERE id = ?2"),
            params![pos as i64, id],
        )?;
    }
    Ok(())
}

impl Storage {
    // ----- providers -----------------------------------------------------------------------

    /// Built-in providers first, then by name.
    pub fn providers(&self) -> Result<Vec<Provider>> {
        query_all(
            &self.conn(),
            &format!(
                "SELECT {PROVIDER_COLS} FROM providers ORDER BY builtin DESC, name COLLATE NOCASE, id"
            ),
            [],
            provider_row,
        )
    }

    pub fn provider(&self, id: &str) -> Result<Option<Provider>> {
        query_one(
            &self.conn(),
            &format!("SELECT {PROVIDER_COLS} FROM providers WHERE id = ?1"),
            params![id],
            provider_row,
        )
    }

    pub fn insert_provider(&self, p: &Provider) -> Result<Provider> {
        let mut p = p.clone();
        p.builtin = false;
        p.validate().map_err(invalid)?;
        self.conn()
            .execute(
                "INSERT INTO providers (id, kind, name, base_url, api_url, oauth_client_id,
                     large_file_warn_mb, large_file_block_mb, builtin, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9)",
                params![
                    p.id,
                    p.kind.as_str(),
                    p.name,
                    p.base_url,
                    p.api_url,
                    p.oauth_client_id,
                    p.large_file_warn_mb,
                    p.large_file_block_mb,
                    now_ms()
                ],
            )
            .map_err(conflict)?;
        self.provider(&p.id)?
            .ok_or_else(|| not_found("provider", &p.id))
    }

    /// Built-in providers only accept a new name, OAuth client id and thresholds.
    pub fn update_provider(&self, p: &Provider) -> Result<Provider> {
        let existing = self
            .provider(&p.id)?
            .ok_or_else(|| not_found("provider", &p.id))?;
        let mut p = p.clone();
        p.builtin = existing.builtin;
        if existing.builtin
            && (p.kind != existing.kind
                || p.base_url != existing.base_url
                || p.api_url != existing.api_url)
        {
            return Err(StorageError::Conflict(
                "the URL and kind of a built-in provider cannot be changed".into(),
            ));
        }
        p.validate().map_err(invalid)?;
        self.conn()
            .execute(
                "UPDATE providers SET kind = ?2, name = ?3, base_url = ?4, api_url = ?5,
                     oauth_client_id = ?6, large_file_warn_mb = ?7, large_file_block_mb = ?8
                 WHERE id = ?1",
                params![
                    p.id,
                    p.kind.as_str(),
                    p.name,
                    p.base_url,
                    p.api_url,
                    p.oauth_client_id,
                    p.large_file_warn_mb,
                    p.large_file_block_mb
                ],
            )
            .map_err(conflict)?;
        self.provider(&p.id)?
            .ok_or_else(|| not_found("provider", &p.id))
    }

    /// Refuses built-in providers and providers that still have accounts.
    pub fn delete_provider(&self, id: &str) -> Result<bool> {
        let conn = self.conn();
        let builtin: Option<bool> = conn
            .query_row(
                "SELECT builtin FROM providers WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?;
        match builtin {
            None => Ok(false),
            Some(true) => Err(StorageError::Conflict(
                "built-in providers cannot be deleted".into(),
            )),
            Some(false) => {
                let used: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM accounts WHERE provider_id = ?1",
                    params![id],
                    |r| r.get(0),
                )?;
                if used > 0 {
                    return Err(StorageError::Conflict(
                        "remove the accounts of this provider first".into(),
                    ));
                }
                Ok(conn
                    .execute("DELETE FROM providers WHERE id = ?1", params![id])
                    .map_err(conflict)?
                    > 0)
            }
        }
    }

    // ----- accounts ------------------------------------------------------------------------

    /// In switcher order.
    pub fn accounts(&self) -> Result<Vec<Account>> {
        query_all(
            &self.conn(),
            &format!("SELECT {ACCOUNT_COLS} FROM accounts ORDER BY sort_order, created_at, id"),
            [],
            account_row,
        )
    }

    pub fn account(&self, id: &str) -> Result<Option<Account>> {
        query_one(
            &self.conn(),
            &format!("SELECT {ACCOUNT_COLS} FROM accounts WHERE id = ?1"),
            params![id],
            account_row,
        )
    }

    /// Inserts at the end of the switcher. `secrets` starts empty (see
    /// [`set_account_secrets`](Self::set_account_secrets)).
    pub fn insert_account(&self, a: &Account) -> Result<Account> {
        a.validate().map_err(invalid)?;
        let now = now_ms();
        self.conn()
            .execute(
                "INSERT INTO accounts (id, provider_id, username, display_name, avatar_url,
                     auth_method, scopes, token_expires_at, color, label, default_clone_dir,
                     secrets, sort_order, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, '[]',
                     (SELECT COALESCE(MAX(sort_order) + 1, 0) FROM accounts), ?12, ?12)",
                params![
                    a.id,
                    a.provider_id,
                    a.username,
                    a.display_name,
                    a.avatar_url,
                    a.auth_method.as_str(),
                    to_json("scopes", &a.scopes)?,
                    a.token_expires_at,
                    a.color,
                    a.label,
                    a.default_clone_dir,
                    now
                ],
            )
            .map_err(conflict)?;
        self.account(&a.id)?
            .ok_or_else(|| not_found("account", &a.id))
    }

    /// Updates profile fields; `secrets`, `sort_order` and `created_at` are left alone.
    pub fn update_account(&self, a: &Account) -> Result<Account> {
        a.validate().map_err(invalid)?;
        let n = self
            .conn()
            .execute(
                "UPDATE accounts SET provider_id = ?2, username = ?3, display_name = ?4,
                     avatar_url = ?5, auth_method = ?6, scopes = ?7, token_expires_at = ?8,
                     color = ?9, label = ?10, default_clone_dir = ?11, updated_at = ?12
                 WHERE id = ?1",
                params![
                    a.id,
                    a.provider_id,
                    a.username,
                    a.display_name,
                    a.avatar_url,
                    a.auth_method.as_str(),
                    to_json("scopes", &a.scopes)?,
                    a.token_expires_at,
                    a.color,
                    a.label,
                    a.default_clone_dir,
                    now_ms()
                ],
            )
            .map_err(conflict)?;
        if n == 0 {
            return Err(not_found("account", &a.id));
        }
        self.account(&a.id)?
            .ok_or_else(|| not_found("account", &a.id))
    }

    /// Records which keychain entries exist for the account (called after writing/deleting
    /// them in the keychain).
    pub fn set_account_secrets(&self, id: &str, secrets: &[AccountSecret]) -> Result<Account> {
        let mut names: Vec<&str> = secrets.iter().map(|s| s.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        let n = self.conn().execute(
            "UPDATE accounts SET secrets = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, to_json("secrets", &names)?, now_ms()],
        )?;
        if n == 0 {
            return Err(not_found("account", id));
        }
        self.account(id)?.ok_or_else(|| not_found("account", id))
    }

    /// Deletes the account with its identities, rules and bindings (cascade). SSH keys are
    /// kept unassigned. The caller removes its keychain entries.
    pub fn delete_account(&self, id: &str) -> Result<bool> {
        Ok(self
            .conn()
            .execute("DELETE FROM accounts WHERE id = ?1", params![id])
            .map_err(conflict)?
            > 0)
    }

    pub fn reorder_accounts(&self, ids: &[String]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        reorder(&tx, "accounts", "sort_order", ids)?;
        tx.commit()?;
        Ok(())
    }

    // ----- identities ----------------------------------------------------------------------

    /// Identities of one account (or all), default first.
    pub fn identities(&self, account_id: Option<&str>) -> Result<Vec<Identity>> {
        let conn = self.conn();
        match account_id {
            Some(acc) => query_all(
                &conn,
                &format!(
                    "SELECT {IDENTITY_COLS} FROM identities WHERE account_id = ?1
                     ORDER BY is_default DESC, created_at, id"
                ),
                params![acc],
                identity_row,
            ),
            None => query_all(
                &conn,
                &format!(
                    "SELECT {IDENTITY_COLS} FROM identities
                     ORDER BY account_id, is_default DESC, created_at, id"
                ),
                [],
                identity_row,
            ),
        }
    }

    pub fn identity(&self, id: &str) -> Result<Option<Identity>> {
        query_one(
            &self.conn(),
            &format!("SELECT {IDENTITY_COLS} FROM identities WHERE id = ?1"),
            params![id],
            identity_row,
        )
    }

    /// The first identity of an account becomes its default automatically.
    pub fn insert_identity(&self, i: &Identity) -> Result<Identity> {
        i.validate().map_err(invalid)?;
        {
            let mut conn = self.conn();
            let tx = conn.transaction()?;
            let has_any: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM identities WHERE account_id = ?1)",
                params![i.account_id],
                |r| r.get(0),
            )?;
            let make_default = i.is_default || !has_any;
            if make_default {
                tx.execute(
                    "UPDATE identities SET is_default = 0 WHERE account_id = ?1",
                    params![i.account_id],
                )?;
            }
            tx.execute(
                "INSERT INTO identities (id, account_id, name, email, signing, signing_key_ref,
                     is_default, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    i.id,
                    i.account_id,
                    i.name.trim(),
                    i.email.trim(),
                    i.signing.as_str(),
                    i.signing_key_ref,
                    make_default,
                    now_ms()
                ],
            )
            .map_err(conflict)?;
            tx.commit()?;
        }
        self.identity(&i.id)?
            .ok_or_else(|| not_found("identity", &i.id))
    }

    /// Updates name, email and signing. The owning account cannot change; use
    /// [`set_default_identity`](Self::set_default_identity) for the default flag.
    pub fn update_identity(&self, i: &Identity) -> Result<Identity> {
        i.validate().map_err(invalid)?;
        let existing = self
            .identity(&i.id)?
            .ok_or_else(|| not_found("identity", &i.id))?;
        if existing.account_id != i.account_id {
            return Err(StorageError::Conflict(
                "an identity cannot be moved to another account".into(),
            ));
        }
        self.conn()
            .execute(
                "UPDATE identities SET name = ?2, email = ?3, signing = ?4, signing_key_ref = ?5
                 WHERE id = ?1",
                params![
                    i.id,
                    i.name.trim(),
                    i.email.trim(),
                    i.signing.as_str(),
                    i.signing_key_ref
                ],
            )
            .map_err(conflict)?;
        if i.is_default && !existing.is_default {
            self.set_default_identity(&i.id)?;
        }
        self.identity(&i.id)?
            .ok_or_else(|| not_found("identity", &i.id))
    }

    pub fn set_default_identity(&self, id: &str) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let account: String = tx
            .query_row(
                "SELECT account_id FROM identities WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| not_found("identity", id))?;
        tx.execute(
            "UPDATE identities SET is_default = (id = ?2) WHERE account_id = ?1",
            params![account, id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Deleting the default identity promotes the oldest remaining one. Rules and bindings that
    /// named it fall back to the account default (`ON DELETE SET NULL`).
    pub fn delete_identity(&self, id: &str) -> Result<bool> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let Some((account, was_default)): Option<(String, bool)> = tx
            .query_row(
                "SELECT account_id, is_default FROM identities WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
        else {
            return Ok(false);
        };
        tx.execute("DELETE FROM identities WHERE id = ?1", params![id])
            .map_err(conflict)?;
        if was_default {
            tx.execute(
                "UPDATE identities SET is_default = 1 WHERE id = (
                     SELECT id FROM identities WHERE account_id = ?1 ORDER BY created_at, id LIMIT 1)",
                params![account],
            )?;
        }
        tx.commit()?;
        Ok(true)
    }

    // ----- SSH keys ------------------------------------------------------------------------

    pub fn ssh_keys(&self, account_id: Option<&str>) -> Result<Vec<SshKey>> {
        let conn = self.conn();
        match account_id {
            Some(acc) => query_all(
                &conn,
                &format!(
                    "SELECT {SSH_KEY_COLS} FROM ssh_keys WHERE account_id = ?1 ORDER BY created_at, id"
                ),
                params![acc],
                ssh_key_row,
            ),
            None => query_all(
                &conn,
                &format!("SELECT {SSH_KEY_COLS} FROM ssh_keys ORDER BY created_at, id"),
                [],
                ssh_key_row,
            ),
        }
    }

    pub fn ssh_key(&self, id: &str) -> Result<Option<SshKey>> {
        query_one(
            &self.conn(),
            &format!("SELECT {SSH_KEY_COLS} FROM ssh_keys WHERE id = ?1"),
            params![id],
            ssh_key_row,
        )
    }

    pub fn insert_ssh_key(&self, k: &SshKey) -> Result<SshKey> {
        k.validate().map_err(invalid)?;
        self.conn()
            .execute(
                "INSERT INTO ssh_keys (id, account_id, key_type, fingerprint, public_key,
                     private_key_path, has_passphrase, uploaded_to_provider, provider_key_id,
                     comment, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    k.id,
                    k.account_id,
                    k.key_type,
                    k.fingerprint,
                    k.public_key,
                    k.private_key_path,
                    k.has_passphrase,
                    k.uploaded_to_provider,
                    k.provider_key_id,
                    k.comment,
                    now_ms()
                ],
            )
            .map_err(conflict)?;
        self.ssh_key(&k.id)?
            .ok_or_else(|| not_found("SSH key", &k.id))
    }

    pub fn update_ssh_key(&self, k: &SshKey) -> Result<SshKey> {
        k.validate().map_err(invalid)?;
        let n = self
            .conn()
            .execute(
                "UPDATE ssh_keys SET account_id = ?2, key_type = ?3, fingerprint = ?4,
                     public_key = ?5, private_key_path = ?6, has_passphrase = ?7,
                     uploaded_to_provider = ?8, provider_key_id = ?9, comment = ?10
                 WHERE id = ?1",
                params![
                    k.id,
                    k.account_id,
                    k.key_type,
                    k.fingerprint,
                    k.public_key,
                    k.private_key_path,
                    k.has_passphrase,
                    k.uploaded_to_provider,
                    k.provider_key_id,
                    k.comment
                ],
            )
            .map_err(conflict)?;
        if n == 0 {
            return Err(not_found("SSH key", &k.id));
        }
        self.ssh_key(&k.id)?
            .ok_or_else(|| not_found("SSH key", &k.id))
    }

    pub fn delete_ssh_key(&self, id: &str) -> Result<bool> {
        Ok(self
            .conn()
            .execute("DELETE FROM ssh_keys WHERE id = ?1", params![id])?
            > 0)
    }

    // ----- rules ---------------------------------------------------------------------------

    /// In evaluation order (priority, then creation).
    pub fn rules(&self) -> Result<Vec<Rule>> {
        query_all(
            &self.conn(),
            &format!("SELECT {RULE_COLS} FROM rules ORDER BY priority, created_at, id"),
            [],
            rule_row,
        )
    }

    pub fn rule(&self, id: &str) -> Result<Option<Rule>> {
        query_one(
            &self.conn(),
            &format!("SELECT {RULE_COLS} FROM rules WHERE id = ?1"),
            params![id],
            rule_row,
        )
    }

    /// Priority one past the current last rule.
    pub fn next_rule_priority(&self) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COALESCE(MAX(priority) + 1, 0) FROM rules",
            [],
            |r| r.get(0),
        )?)
    }

    pub fn insert_rule(&self, rule: &Rule) -> Result<Rule> {
        rule.validate().map_err(invalid)?;
        self.conn()
            .execute(
                "INSERT INTO rules (id, priority, host, owner_or_group, path_glob, remote_regex,
                     account_id, identity_id, enabled, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    rule.id,
                    rule.priority,
                    rule.host,
                    rule.owner_or_group,
                    rule.path_glob,
                    rule.remote_regex,
                    rule.account_id,
                    rule.identity_id,
                    rule.enabled,
                    now_ms()
                ],
            )
            .map_err(conflict)?;
        self.rule(&rule.id)?
            .ok_or_else(|| not_found("rule", &rule.id))
    }

    pub fn update_rule(&self, rule: &Rule) -> Result<Rule> {
        rule.validate().map_err(invalid)?;
        let n = self
            .conn()
            .execute(
                "UPDATE rules SET priority = ?2, host = ?3, owner_or_group = ?4, path_glob = ?5,
                     remote_regex = ?6, account_id = ?7, identity_id = ?8, enabled = ?9
                 WHERE id = ?1",
                params![
                    rule.id,
                    rule.priority,
                    rule.host,
                    rule.owner_or_group,
                    rule.path_glob,
                    rule.remote_regex,
                    rule.account_id,
                    rule.identity_id,
                    rule.enabled
                ],
            )
            .map_err(conflict)?;
        if n == 0 {
            return Err(not_found("rule", &rule.id));
        }
        self.rule(&rule.id)?
            .ok_or_else(|| not_found("rule", &rule.id))
    }

    pub fn delete_rule(&self, id: &str) -> Result<bool> {
        Ok(self
            .conn()
            .execute("DELETE FROM rules WHERE id = ?1", params![id])?
            > 0)
    }

    /// Renumbers priorities to follow `ids` (unlisted rules keep their order after them).
    pub fn reorder_rules(&self, ids: &[String]) -> Result<Vec<Rule>> {
        {
            let mut conn = self.conn();
            let tx = conn.transaction()?;
            reorder(&tx, "rules", "priority", ids)?;
            tx.commit()?;
        }
        self.rules()
    }

    // ----- repo bindings -------------------------------------------------------------------

    /// Bindings of a repo: repo-level default first, then per remote by name.
    pub fn repo_bindings(&self, repo_id: i64) -> Result<Vec<RepoBinding>> {
        query_all(
            &self.conn(),
            &format!(
                "SELECT {BINDING_COLS} FROM repo_bindings WHERE repo_id = ?1 ORDER BY remote_name"
            ),
            params![repo_id],
            binding_row,
        )
    }

    pub fn repo_binding(&self, repo_id: i64, remote: Option<&str>) -> Result<Option<RepoBinding>> {
        query_one(
            &self.conn(),
            &format!(
                "SELECT {BINDING_COLS} FROM repo_bindings WHERE repo_id = ?1 AND remote_name = ?2"
            ),
            params![repo_id, remote.unwrap_or_default()],
            binding_row,
        )
    }

    /// Inserts or replaces the binding for `(repo_id, remote_name)`.
    pub fn set_repo_binding(&self, b: &RepoBinding) -> Result<RepoBinding> {
        b.validate().map_err(invalid)?;
        let remote = b.remote_name.clone().unwrap_or_default();
        self.conn()
            .execute(
                "INSERT INTO repo_bindings (repo_id, remote_name, account_id, identity_id,
                     transport, source, locked, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT (repo_id, remote_name) DO UPDATE SET
                     account_id = excluded.account_id, identity_id = excluded.identity_id,
                     transport = excluded.transport, source = excluded.source,
                     locked = excluded.locked, updated_at = excluded.updated_at",
                params![
                    b.repo_id,
                    remote,
                    b.account_id,
                    b.identity_id,
                    b.transport.map(Transport::as_str),
                    b.source.as_str(),
                    b.locked,
                    now_ms()
                ],
            )
            .map_err(conflict)?;
        self.repo_binding(b.repo_id, b.remote_name.as_deref())?
            .ok_or_else(|| not_found("binding", format!("{}/{remote}", b.repo_id)))
    }

    pub fn clear_repo_binding(&self, repo_id: i64, remote: Option<&str>) -> Result<bool> {
        Ok(self.conn().execute(
            "DELETE FROM repo_bindings WHERE repo_id = ?1 AND remote_name = ?2",
            params![repo_id, remote.unwrap_or_default()],
        )? > 0)
    }

    pub fn clear_repo_bindings(&self, repo_id: i64) -> Result<usize> {
        Ok(self.conn().execute(
            "DELETE FROM repo_bindings WHERE repo_id = ?1",
            params![repo_id],
        )?)
    }

    // ----- workspaces ----------------------------------------------------------------------

    pub fn workspaces(&self) -> Result<Vec<Workspace>> {
        let conn = self.conn();
        let mut list = query_all(
            &conn,
            "SELECT id, name, default_account_id, created_at FROM workspaces
             ORDER BY name COLLATE NOCASE, id",
            [],
            |r| {
                Ok(Workspace {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    repo_ids: Vec::new(),
                    default_account_id: r.get(2)?,
                    created_at: r.get(3)?,
                })
            },
        )?;
        for w in &mut list {
            w.repo_ids = workspace_repo_ids(&conn, &w.id)?;
        }
        Ok(list)
    }

    pub fn workspace(&self, id: &str) -> Result<Option<Workspace>> {
        Ok(self.workspaces()?.into_iter().find(|w| w.id == id))
    }

    pub fn insert_workspace(&self, w: &Workspace) -> Result<Workspace> {
        w.validate().map_err(invalid)?;
        {
            let mut conn = self.conn();
            let tx = conn.transaction()?;
            tx.execute(
                "INSERT INTO workspaces (id, name, default_account_id, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![w.id, w.name.trim(), w.default_account_id, now_ms()],
            )
            .map_err(conflict)?;
            write_workspace_repos(&tx, &w.id, &w.repo_ids)?;
            tx.commit()?;
        }
        self.workspace(&w.id)?
            .ok_or_else(|| not_found("workspace", &w.id))
    }

    /// Replaces name, default account and the ordered repo list.
    pub fn update_workspace(&self, w: &Workspace) -> Result<Workspace> {
        w.validate().map_err(invalid)?;
        {
            let mut conn = self.conn();
            let tx = conn.transaction()?;
            let n = tx
                .execute(
                    "UPDATE workspaces SET name = ?2, default_account_id = ?3 WHERE id = ?1",
                    params![w.id, w.name.trim(), w.default_account_id],
                )
                .map_err(conflict)?;
            if n == 0 {
                return Err(not_found("workspace", &w.id));
            }
            write_workspace_repos(&tx, &w.id, &w.repo_ids)?;
            tx.commit()?;
        }
        self.workspace(&w.id)?
            .ok_or_else(|| not_found("workspace", &w.id))
    }

    /// Appends a repo to a workspace (no-op if already there).
    pub fn add_repo_to_workspace(&self, workspace_id: &str, repo_id: i64) -> Result<()> {
        self.conn()
            .execute(
                "INSERT OR IGNORE INTO workspace_repos (workspace_id, repo_id, position)
                 VALUES (?1, ?2, (SELECT COALESCE(MAX(position) + 1, 0) FROM workspace_repos
                                  WHERE workspace_id = ?1))",
                params![workspace_id, repo_id],
            )
            .map_err(conflict)?;
        Ok(())
    }

    pub fn remove_repo_from_workspace(&self, workspace_id: &str, repo_id: i64) -> Result<bool> {
        Ok(self.conn().execute(
            "DELETE FROM workspace_repos WHERE workspace_id = ?1 AND repo_id = ?2",
            params![workspace_id, repo_id],
        )? > 0)
    }

    pub fn delete_workspace(&self, id: &str) -> Result<bool> {
        Ok(self
            .conn()
            .execute("DELETE FROM workspaces WHERE id = ?1", params![id])?
            > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Storage {
        Storage::open_in_memory().unwrap()
    }

    fn account(id: &str, provider: &str, user: &str) -> Account {
        Account {
            id: id.into(),
            provider_id: provider.into(),
            username: user.into(),
            display_name: Some(user.to_uppercase()),
            avatar_url: None,
            auth_method: AuthMethod::Pat,
            scopes: vec!["repo".into(), "read:org".into()],
            token_expires_at: Some(1_900_000_000_000),
            color: "#1c7ed6".into(),
            label: Some("Công ty".into()),
            default_clone_dir: Some("D:/Work".into()),
            secrets: vec![],
            sort_order: 0,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn identity(id: &str, account: &str, email: &str) -> Identity {
        Identity {
            id: id.into(),
            account_id: account.into(),
            name: "Alice".into(),
            email: email.into(),
            signing: SigningMethod::None,
            signing_key_ref: None,
            is_default: false,
            created_at: 0,
        }
    }

    fn rule(id: &str, account: &str, identity: Option<&str>) -> Rule {
        Rule {
            id: id.into(),
            priority: 0,
            host: Some("github.com".into()),
            owner_or_group: Some("acme".into()),
            path_glob: None,
            remote_regex: None,
            account_id: account.into(),
            identity_id: identity.map(Into::into),
            enabled: true,
            created_at: 0,
        }
    }

    fn repo(db: &Storage, path: &str) -> i64 {
        db.touch_repo(path, path, "r", 1).unwrap().id
    }

    #[test]
    fn builtin_providers_are_seeded_and_protected() {
        let db = db();
        let ids: Vec<_> = db.providers().unwrap().into_iter().map(|p| p.id).collect();
        assert_eq!(ids, ["github", "gitlab"]);
        assert_eq!(
            db.provider("github").unwrap().unwrap(),
            Provider::github_com()
        );
        assert_eq!(
            db.provider("gitlab").unwrap().unwrap(),
            Provider::gitlab_com()
        );
        assert!(matches!(
            db.delete_provider("github"),
            Err(StorageError::Conflict(_))
        ));

        let mut gh = Provider::github_com();
        gh.oauth_client_id = Some("Iv1.abc".into());
        gh.large_file_warn_mb = 40;
        assert_eq!(
            db.update_provider(&gh).unwrap().oauth_client_id.as_deref(),
            Some("Iv1.abc")
        );
        gh.base_url = "https://evil.example".into();
        assert!(matches!(
            db.update_provider(&gh),
            Err(StorageError::Conflict(_))
        ));
    }

    #[test]
    fn custom_provider_crud() {
        let db = db();
        let p = Provider {
            id: "prv_acme".into(),
            kind: ProviderKind::GitlabSelfHosted,
            name: "Acme GitLab".into(),
            base_url: "https://gitlab.acme.dev".into(),
            api_url: "https://gitlab.acme.dev/api/v4".into(),
            oauth_client_id: None,
            large_file_warn_mb: 50,
            large_file_block_mb: None,
            builtin: true, // ignored on insert
        };
        let saved = db.insert_provider(&p).unwrap();
        assert!(!saved.builtin);
        let mut dup = p.clone();
        dup.id = "prv_dup".into();
        dup.base_url = "HTTPS://GITLAB.ACME.DEV".into();
        assert!(
            matches!(db.insert_provider(&dup), Err(StorageError::Conflict(m)) if m.contains("URL"))
        );
        let mut bad = p.clone();
        bad.id = "prv_bad".into();
        bad.base_url = "nope".into();
        assert!(matches!(
            db.insert_provider(&bad),
            Err(StorageError::Invalid(_))
        ));

        db.insert_account(&account("acc_1", "prv_acme", "alice"))
            .unwrap();
        assert!(matches!(
            db.delete_provider("prv_acme"),
            Err(StorageError::Conflict(_))
        ));
        db.delete_account("acc_1").unwrap();
        assert!(db.delete_provider("prv_acme").unwrap());
        assert!(!db.delete_provider("prv_acme").unwrap());
    }

    #[test]
    fn accounts_crud_order_and_secrets() {
        let db = db();
        let a = db
            .insert_account(&account("acc_a", "github", "alice"))
            .unwrap();
        assert!(a.created_at > 0 && a.updated_at == a.created_at);
        assert_eq!(a.scopes, ["repo", "read:org"]);
        assert_eq!(a.label.as_deref(), Some("Công ty"));
        db.insert_account(&account("acc_b", "github", "bob"))
            .unwrap();
        db.insert_account(&account("acc_c", "gitlab", "alice"))
            .unwrap();
        // Same user on the same provider (case-insensitive) is a duplicate.
        assert!(matches!(
            db.insert_account(&account("acc_d", "github", "ALICE")),
            Err(StorageError::Conflict(m)) if m.contains("already added")
        ));
        assert!(matches!(
            db.insert_account(&account("acc_e", "no-such-provider", "x")),
            Err(StorageError::Conflict(_))
        ));
        let order = |db: &Storage| {
            db.accounts()
                .unwrap()
                .into_iter()
                .map(|a| a.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(order(&db), ["acc_a", "acc_b", "acc_c"]);
        db.reorder_accounts(&["acc_c".into()]).unwrap();
        assert_eq!(order(&db), ["acc_c", "acc_a", "acc_b"]);
        assert!(db.reorder_accounts(&["acc_zzz".into()]).is_err());

        let with = db
            .set_account_secrets(
                "acc_a",
                &[
                    AccountSecret::RefreshToken,
                    AccountSecret::Token,
                    AccountSecret::Token,
                ],
            )
            .unwrap();
        assert_eq!(
            with.secrets,
            [AccountSecret::RefreshToken, AccountSecret::Token]
        );
        assert!(with.has_secret(AccountSecret::Token));

        let mut edited = with.clone();
        edited.label = None;
        edited.color = "#7048e8".into();
        edited.secrets = vec![];
        let edited = db.update_account(&edited).unwrap();
        assert_eq!(edited.color, "#7048e8");
        assert_eq!(edited.secrets.len(), 2, "update must not touch secrets");
        assert!(matches!(
            db.update_account(&account("acc_x", "github", "x")),
            Err(StorageError::NotFound { .. })
        ));
    }

    #[test]
    fn identities_defaults_and_ownership() {
        let db = db();
        db.insert_account(&account("acc_a", "github", "alice"))
            .unwrap();
        db.insert_account(&account("acc_b", "github", "bob"))
            .unwrap();
        let first = db
            .insert_identity(&identity("idn_1", "acc_a", "alice@acme.dev"))
            .unwrap();
        assert!(first.is_default, "first identity becomes default");
        let second = db
            .insert_identity(&identity("idn_2", "acc_a", "alice@gmail.com"))
            .unwrap();
        assert!(!second.is_default);
        assert!(matches!(
            db.insert_identity(&identity("idn_3", "acc_a", "ALICE@acme.dev")),
            Err(StorageError::Conflict(_))
        ));
        let mut bad = identity("idn_4", "acc_a", "not-an-email");
        assert!(matches!(
            db.insert_identity(&bad),
            Err(StorageError::Invalid(_))
        ));
        bad.email = "x@y.z".into();
        bad.signing = SigningMethod::Gpg;
        assert!(matches!(
            db.insert_identity(&bad),
            Err(StorageError::Invalid(_))
        ));

        db.set_default_identity("idn_2").unwrap();
        let ids = db.identities(Some("acc_a")).unwrap();
        assert_eq!(ids[0].id, "idn_2");
        assert_eq!(ids.iter().filter(|i| i.is_default).count(), 1);

        let mut moved = second.clone();
        moved.account_id = "acc_b".into();
        assert!(matches!(
            db.update_identity(&moved),
            Err(StorageError::Conflict(_))
        ));
        let mut renamed = second.clone();
        renamed.name = "Alice N.".into();
        renamed.signing = SigningMethod::Ssh;
        renamed.signing_key_ref = Some("~/.ssh/id_ed25519.pub".into());
        assert_eq!(db.update_identity(&renamed).unwrap().name, "Alice N.");

        assert!(db.delete_identity("idn_2").unwrap());
        assert!(
            db.identity("idn_1").unwrap().unwrap().is_default,
            "promoted"
        );
        assert!(!db.delete_identity("idn_2").unwrap());
        assert_eq!(db.identities(None).unwrap().len(), 1);
    }

    #[test]
    fn rules_crud_reorder_and_identity_trigger() {
        let db = db();
        db.insert_account(&account("acc_a", "github", "alice"))
            .unwrap();
        db.insert_account(&account("acc_b", "github", "bob"))
            .unwrap();
        db.insert_identity(&identity("idn_a", "acc_a", "a@x.dev"))
            .unwrap();
        db.insert_identity(&identity("idn_b", "acc_b", "b@x.dev"))
            .unwrap();

        assert_eq!(db.next_rule_priority().unwrap(), 0);
        db.insert_rule(&rule("r1", "acc_a", Some("idn_a"))).unwrap();
        let mut r2 = rule("r2", "acc_b", None);
        r2.priority = db.next_rule_priority().unwrap();
        db.insert_rule(&r2).unwrap();
        // Identity of another account is refused by the trigger.
        assert!(matches!(
            db.insert_rule(&rule("r3", "acc_a", Some("idn_b"))),
            Err(StorageError::Conflict(m)) if m.contains("does not belong")
        ));
        let mut moved = r2.clone();
        moved.identity_id = Some("idn_a".into());
        assert!(matches!(
            db.update_rule(&moved),
            Err(StorageError::Conflict(_))
        ));
        let mut empty = rule("r4", "acc_a", None);
        empty.host = None;
        empty.owner_or_group = None;
        assert!(matches!(
            db.insert_rule(&empty),
            Err(StorageError::Invalid(_))
        ));

        let ids = |db: &Storage| {
            db.rules()
                .unwrap()
                .into_iter()
                .map(|r| r.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&db), ["r1", "r2"]);
        db.reorder_rules(&["r2".into(), "r1".into()]).unwrap();
        assert_eq!(ids(&db), ["r2", "r1"]);
        assert_eq!(db.rule("r1").unwrap().unwrap().priority, 1);

        // Deleting the identity keeps the rule, falling back to the account default.
        db.delete_identity("idn_a").unwrap();
        assert_eq!(db.rule("r1").unwrap().unwrap().identity_id, None);
        // Deleting the account removes its rules.
        db.delete_account("acc_b").unwrap();
        assert_eq!(ids(&db), ["r1"]);
        assert!(db.delete_rule("r1").unwrap());
    }

    #[test]
    fn bindings_per_remote_and_cascades() {
        let db = db();
        db.insert_account(&account("acc_a", "github", "alice"))
            .unwrap();
        db.insert_account(&account("acc_b", "gitlab", "bob"))
            .unwrap();
        db.insert_identity(&identity("idn_a", "acc_a", "a@x.dev"))
            .unwrap();
        db.insert_identity(&identity("idn_b", "acc_b", "b@x.dev"))
            .unwrap();
        let repo_id = repo(&db, "/r");

        let mut b = RepoBinding {
            repo_id,
            remote_name: None,
            account_id: "acc_a".into(),
            identity_id: Some("idn_a".into()),
            transport: Some(Transport::Https),
            source: BindingSource::Clone,
            locked: false,
            updated_at: 0,
        };
        db.set_repo_binding(&b).unwrap();
        b.remote_name = Some("upstream".into());
        b.account_id = "acc_b".into();
        b.identity_id = Some("idn_b".into());
        b.transport = None;
        b.source = BindingSource::Manual;
        b.locked = true;
        db.set_repo_binding(&b).unwrap();
        let all = db.repo_bindings(repo_id).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].remote_name, None);
        assert_eq!(all[0].transport, Some(Transport::Https));
        assert_eq!(all[1].remote_name.as_deref(), Some("upstream"));
        assert!(all[1].locked);

        // Upsert replaces; identity must belong to the account.
        b.identity_id = Some("idn_a".into());
        assert!(matches!(
            db.set_repo_binding(&b),
            Err(StorageError::Conflict(_))
        ));
        b.identity_id = None;
        b.locked = false;
        let saved = db.set_repo_binding(&b).unwrap();
        assert!(!saved.locked && saved.updated_at > 0);
        assert_eq!(db.repo_bindings(repo_id).unwrap().len(), 2);

        assert!(db.clear_repo_binding(repo_id, Some("upstream")).unwrap());
        assert!(!db.clear_repo_binding(repo_id, Some("upstream")).unwrap());
        // Deleting the identity keeps the binding; deleting the account or repo drops it.
        db.delete_identity("idn_a").unwrap();
        assert_eq!(
            db.repo_binding(repo_id, None).unwrap().unwrap().identity_id,
            None
        );
        db.delete_account("acc_a").unwrap();
        assert!(db.repo_bindings(repo_id).unwrap().is_empty());

        b.remote_name = None;
        db.set_repo_binding(&b).unwrap();
        db.delete_repo(repo_id).unwrap();
        assert!(db.repo_bindings(repo_id).unwrap().is_empty());
        assert_eq!(db.clear_repo_bindings(repo_id).unwrap(), 0);
    }

    #[test]
    fn ssh_keys_survive_account_deletion_unassigned() {
        let db = db();
        db.insert_account(&account("acc_a", "github", "alice"))
            .unwrap();
        let key = SshKey {
            id: "key_1".into(),
            account_id: Some("acc_a".into()),
            key_type: "ed25519".into(),
            fingerprint: "SHA256:abc".into(),
            public_key: "ssh-ed25519 AAAA alice".into(),
            private_key_path: Some("C:/keys/alice".into()),
            has_passphrase: true,
            uploaded_to_provider: false,
            provider_key_id: None,
            comment: None,
            created_at: 0,
        };
        db.insert_ssh_key(&key).unwrap();
        let mut dup = key.clone();
        dup.id = "key_2".into();
        assert!(matches!(
            db.insert_ssh_key(&dup),
            Err(StorageError::Conflict(_))
        ));
        let mut up = key.clone();
        up.uploaded_to_provider = true;
        up.provider_key_id = Some("123".into());
        assert!(db.update_ssh_key(&up).unwrap().uploaded_to_provider);
        assert_eq!(db.ssh_keys(Some("acc_a")).unwrap().len(), 1);
        db.delete_account("acc_a").unwrap();
        let k = db.ssh_key("key_1").unwrap().unwrap();
        assert_eq!(k.account_id, None);
        assert!(db.ssh_keys(Some("acc_a")).unwrap().is_empty());
        assert_eq!(db.ssh_keys(None).unwrap().len(), 1);
        assert!(db.delete_ssh_key("key_1").unwrap());
    }

    #[test]
    fn workspaces_group_repos() {
        let db = db();
        db.insert_account(&account("acc_a", "github", "alice"))
            .unwrap();
        let (r1, r2, r3) = (repo(&db, "/a"), repo(&db, "/b"), repo(&db, "/c"));
        let w = Workspace {
            id: "wsp_1".into(),
            name: "Acme".into(),
            repo_ids: vec![r2, r1],
            default_account_id: Some("acc_a".into()),
            created_at: 0,
        };
        let saved = db.insert_workspace(&w).unwrap();
        assert_eq!(saved.repo_ids, [r2, r1]);
        let mut dup = w.clone();
        dup.id = "wsp_2".into();
        dup.name = "acme".into();
        assert!(matches!(
            db.insert_workspace(&dup),
            Err(StorageError::Conflict(_))
        ));

        db.add_repo_to_workspace("wsp_1", r3).unwrap();
        db.add_repo_to_workspace("wsp_1", r3).unwrap();
        assert_eq!(
            db.workspace("wsp_1").unwrap().unwrap().repo_ids,
            [r2, r1, r3]
        );
        assert!(db.remove_repo_from_workspace("wsp_1", r1).unwrap());
        db.delete_repo(r2).unwrap();
        db.delete_account("acc_a").unwrap();
        let w = db.workspace("wsp_1").unwrap().unwrap();
        assert_eq!(w.repo_ids, [r3]);
        assert_eq!(w.default_account_id, None);

        let mut renamed = w.clone();
        renamed.name = "Acme 2".into();
        renamed.repo_ids = vec![r3, r1];
        assert_eq!(db.update_workspace(&renamed).unwrap().repo_ids, [r3, r1]);
        assert!(db.delete_workspace("wsp_1").unwrap());
        assert!(db.workspaces().unwrap().is_empty());
    }
}
