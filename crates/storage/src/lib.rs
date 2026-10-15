//! SQLite storage and schema migrations.
//!
//! Holds non-secret app data only: settings, repos, and (schema v3) providers, accounts,
//! identities, SSH key metadata, rules, repo bindings and workspaces — see [`accounts`].
//! Secrets live in the OS keychain and are referenced from here by id.

pub mod accounts;
mod migrations;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

pub use migrations::LATEST_VERSION;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("invalid stored value for `{key}`: {source}")]
    InvalidValue {
        key: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("could not encode value for `{key}`: {source}")]
    Encode {
        key: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("database schema v{found} is newer than this app supports (v{supported})")]
    SchemaTooNew { found: u32, supported: u32 },
    #[error("could not create database directory: {0}")]
    Io(#[from] std::io::Error),
    /// A record failed validation before being written.
    #[error("invalid {0}")]
    Invalid(nanomgg_accounts::ValidationError),
    /// A uniqueness, foreign-key or ownership constraint refused the write.
    #[error("{0}")]
    Conflict(String),
    #[error("{entity} `{id}` not found")]
    NotFound { entity: &'static str, id: String },
}

pub type Result<T, E = StorageError> = std::result::Result<T, E>;

pub struct Storage {
    conn: Mutex<Connection>,
}

impl Storage {
    /// Opens (creating if needed) the database at `path` and applies pending migrations.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrations::migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        // A panic while holding the lock cannot leave SQLite in a torn state
        // (statements are atomic), so recovering from poisoning is safe.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn schema_version(&self) -> Result<u32> {
        migrations::schema_version(&self.conn())
    }

    pub fn get_setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let raw: Option<String> = self
            .conn()
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?;
        raw.map(|s| {
            serde_json::from_str(&s).map_err(|source| StorageError::InvalidValue {
                key: key.to_owned(),
                source,
            })
        })
        .transpose()
    }

    pub fn set_setting<T: Serialize + ?Sized>(&self, key: &str, value: &T) -> Result<()> {
        let json = serde_json::to_string(value).map_err(|source| StorageError::Encode {
            key: key.to_owned(),
            source,
        })?;
        self.conn().execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, unixepoch())
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, json],
        )?;
        Ok(())
    }

    pub fn delete_setting(&self, key: &str) -> Result<bool> {
        Ok(self
            .conn()
            .execute("DELETE FROM settings WHERE key = ?1", params![key])?
            > 0)
    }

    /// All settings as raw JSON values, keyed by name.
    pub fn settings(&self) -> Result<BTreeMap<String, serde_json::Value>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT key, value FROM settings ORDER BY key")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (key, raw) = row?;
            let value =
                serde_json::from_str(&raw).map_err(|source| StorageError::InvalidValue {
                    key: key.clone(),
                    source,
                })?;
            out.insert(key, value);
        }
        Ok(out)
    }

    /// Records that a repository was opened now: inserts it or refreshes path, name and time.
    pub fn touch_repo(
        &self,
        path: &str,
        path_key: &str,
        name: &str,
        now_ms: i64,
    ) -> Result<RepoRecord> {
        Ok(self.conn().query_row(
            "INSERT INTO repos (path, path_key, name, last_opened_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path_key) DO UPDATE
               SET path = excluded.path, name = excluded.name, last_opened_at = excluded.last_opened_at
             RETURNING id, path, name, last_opened_at",
            params![path, path_key, name, now_ms],
            RepoRecord::from_row,
        )?)
    }

    pub fn repo(&self, id: i64) -> Result<Option<RepoRecord>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT id, path, name, last_opened_at FROM repos WHERE id = ?1",
                params![id],
                RepoRecord::from_row,
            )
            .optional()?)
    }

    /// Most recently opened first.
    pub fn recent_repos(&self, limit: u32) -> Result<Vec<RepoRecord>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, path, name, last_opened_at FROM repos
             ORDER BY last_opened_at DESC, id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], RepoRecord::from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn delete_repo(&self, id: i64) -> Result<bool> {
        Ok(self
            .conn()
            .execute("DELETE FROM repos WHERE id = ?1", params![id])?
            > 0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRecord {
    pub id: i64,
    pub path: String,
    pub name: String,
    /// Milliseconds since the Unix epoch.
    pub last_opened_at: i64,
}

impl RepoRecord {
    fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get(0)?,
            path: r.get(1)?,
            name: r.get(2)?,
            last_opened_at: r.get(3)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repos_upsert_by_key_and_order_by_recency() {
        let db = Storage::open_in_memory().unwrap();
        let a = db
            .touch_repo("D:\\Work\\Api", "d:\\work\\api", "Api", 1_000)
            .unwrap();
        let b = db
            .touch_repo("D:\\Work\\Web", "d:\\work\\web", "Web", 2_000)
            .unwrap();
        // Same folder typed with different casing: same row, updated path and time.
        let a2 = db
            .touch_repo("d:\\work\\api", "d:\\work\\api", "api", 3_000)
            .unwrap();
        assert_eq!(a2.id, a.id);
        assert_eq!(a2.path, "d:\\work\\api");

        let recent = db.recent_repos(10).unwrap();
        assert_eq!(
            recent.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![a.id, b.id]
        );
        assert_eq!(db.recent_repos(1).unwrap().len(), 1);
        assert_eq!(db.repo(b.id).unwrap().unwrap().name, "Web");

        assert!(db.delete_repo(b.id).unwrap());
        assert_eq!(db.repo(b.id).unwrap(), None);
    }

    #[test]
    fn upgrades_a_v1_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v1.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(migrations::MIGRATIONS[0]).unwrap();
            conn.pragma_update(None, "user_version", 1).unwrap();
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('theme', '\"dark\"')",
                [],
            )
            .unwrap();
        }
        let db = Storage::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), LATEST_VERSION);
        assert_eq!(
            db.get_setting::<String>("theme").unwrap().as_deref(),
            Some("dark")
        );
        db.touch_repo("/r", "/r", "r", 1).unwrap();
    }

    #[test]
    fn upgrades_a_v3_database_keeping_accounts_and_their_children() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v3.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(None, "foreign_keys", true).unwrap();
            for sql in &migrations::MIGRATIONS[..3] {
                conn.execute_batch(sql).unwrap();
            }
            conn.pragma_update(None, "user_version", 3).unwrap();
            conn.execute_batch(
                "INSERT INTO repos (id, path, path_key, name, last_opened_at) VALUES (7, '/w/api', '/w/api', 'api', 5);
                 INSERT INTO accounts (id, provider_id, username, auth_method, color, secrets, sort_order, created_at, updated_at)
                     VALUES ('acc_1', 'github', 'An-Dev', 'pat', '#336699', '[\"token\"]', 3, 10, 11);
                 INSERT INTO identities (id, account_id, name, email, is_default, created_at)
                     VALUES ('idn_1', 'acc_1', 'An', 'an@x.vn', 1, 12);
                 INSERT INTO rules (id, priority, host, account_id, identity_id, created_at)
                     VALUES ('rul_1', 1, 'github.com', 'acc_1', 'idn_1', 13);
                 INSERT INTO repo_bindings (repo_id, account_id, identity_id, source, updated_at)
                     VALUES (7, 'acc_1', 'idn_1', 'manual', 14);
                 INSERT INTO ssh_keys (id, account_id, key_type, fingerprint, public_key, created_at)
                     VALUES ('key_1', 'acc_1', 'ed25519', 'SHA256:x', 'ssh-ed25519 AAAA', 15);
                 INSERT INTO workspaces (id, name, default_account_id, created_at)
                     VALUES ('wsp_1', 'Work', 'acc_1', 16);",
            )
            .unwrap();
            assert!(
                conn.execute(
                    "INSERT INTO accounts (id, provider_id, username, auth_method, color, created_at, updated_at)
                     VALUES ('acc_2', 'github', 'b', 'imported_cli', '#336699', 0, 0)",
                    [],
                )
                .is_err(),
                "v3 refuses the new auth method"
            );
        }
        let db = Storage::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), LATEST_VERSION);
        use nanomgg_accounts::AccountSecret;
        let acc = db.account("acc_1").unwrap().unwrap();
        assert_eq!(acc.username, "An-Dev");
        assert_eq!(acc.secrets, [AccountSecret::Token]);
        assert_eq!(acc.sort_order, 3);
        assert_eq!(db.identities(Some("acc_1")).unwrap().len(), 1, "no cascade");
        assert_eq!(db.rules().unwrap().len(), 1);
        assert_eq!(db.repo_bindings(7).unwrap().len(), 1);
        assert_eq!(db.ssh_keys(Some("acc_1")).unwrap().len(), 1, "no SET NULL");

        let conn = db.conn();
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1, "foreign keys back on");
        conn.execute(
            "INSERT INTO accounts (id, provider_id, username, auth_method, color, created_at, updated_at)
             VALUES ('acc_2', 'github', 'cli-user', 'imported_cli', '#336699', 0, 0)",
            [],
        )
        .unwrap();
        // Unique (provider, username) index and the provider reference survive the rebuild.
        assert!(
            conn.execute(
                "INSERT INTO accounts (id, provider_id, username, auth_method, color, created_at, updated_at)
                 VALUES ('acc_3', 'github', 'AN-DEV', 'pat', '#336699', 0, 0)",
                [],
            )
            .is_err()
        );
        assert!(
            conn.execute(
                "INSERT INTO accounts (id, provider_id, username, auth_method, color, created_at, updated_at)
                 VALUES ('acc_4', 'nope', 'x', 'pat', '#336699', 0, 0)",
                [],
            )
            .is_err()
        );
        // Children still reference the rebuilt table: deleting the account cascades.
        conn.execute("DELETE FROM accounts WHERE id = 'acc_1'", [])
            .unwrap();
        let left: i64 = conn
            .query_row("SELECT count(*) FROM identities", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0);
        let strict: i64 = conn
            .query_row(
                "SELECT strict FROM pragma_table_list WHERE name = 'accounts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(strict, 1);
    }

    #[test]
    fn upgrades_a_v2_database_keeping_repos_and_adding_account_tables() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v2.db");
        let repo_id: i64;
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(migrations::MIGRATIONS[0]).unwrap();
            conn.execute_batch(migrations::MIGRATIONS[1]).unwrap();
            conn.pragma_update(None, "user_version", 2).unwrap();
            conn.execute(
                "INSERT INTO repos (path, path_key, name, last_opened_at) VALUES ('/w/api', '/w/api', 'api', 5)",
                [],
            )
            .unwrap();
            repo_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('language', '\"vi\"')",
                [],
            )
            .unwrap();
        }
        let db = Storage::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), LATEST_VERSION);
        assert_eq!(db.repo(repo_id).unwrap().unwrap().name, "api");
        assert_eq!(
            db.get_setting::<String>("language").unwrap().as_deref(),
            Some("vi")
        );
        assert_eq!(db.providers().unwrap().len(), 2);
        assert!(db.repo_bindings(repo_id).unwrap().is_empty());

        // Every new table is STRICT and foreign keys are enforced on this connection.
        let conn = db.conn();
        for table in [
            "providers",
            "accounts",
            "identities",
            "ssh_keys",
            "rules",
            "repo_bindings",
            "workspaces",
            "workspace_repos",
        ] {
            let strict: i64 = conn
                .query_row(
                    "SELECT strict FROM pragma_table_list WHERE name = ?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(strict, 1, "{table} must be STRICT");
        }
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
        let err = conn
            .execute(
                "INSERT INTO repo_bindings (repo_id, account_id, source, updated_at) VALUES (?1, 'acc_missing', 'manual', 0)",
                [repo_id],
            )
            .unwrap_err();
        assert!(err.to_string().contains("FOREIGN KEY"), "{err}");
        // STRICT rejects values of the wrong type.
        assert!(
            conn.execute(
                "INSERT INTO workspaces (id, name, created_at) VALUES ('w', 'n', 'yesterday')",
                []
            )
            .is_err()
        );
    }

    #[test]
    fn fresh_database_is_migrated_to_latest() {
        let db = Storage::open_in_memory().unwrap();
        assert_eq!(db.schema_version().unwrap(), LATEST_VERSION);
    }

    #[test]
    fn settings_round_trip_and_overwrite() {
        let db = Storage::open_in_memory().unwrap();
        assert_eq!(db.get_setting::<String>("theme").unwrap(), None);

        db.set_setting("theme", "dark").unwrap();
        assert_eq!(
            db.get_setting::<String>("theme").unwrap().as_deref(),
            Some("dark")
        );

        db.set_setting("theme", "light").unwrap();
        db.set_setting("ui.sidebarWidth", &240).unwrap();
        let all = db.settings().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all["theme"], serde_json::json!("light"));
        assert_eq!(all["ui.sidebarWidth"], serde_json::json!(240));

        assert!(db.delete_setting("theme").unwrap());
        assert!(!db.delete_setting("theme").unwrap());
    }

    #[test]
    fn wrong_type_is_reported_with_key() {
        let db = Storage::open_in_memory().unwrap();
        db.set_setting("count", "not a number").unwrap();
        let err = db.get_setting::<u32>("count").unwrap_err();
        assert!(matches!(err, StorageError::InvalidValue { ref key, .. } if key == "count"));
    }

    #[test]
    fn reopening_a_file_keeps_data_and_does_not_remigrate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("nanomgg.db");
        {
            let db = Storage::open(&path).unwrap();
            db.set_setting("language", "vi").unwrap();
        }
        let db = Storage::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), LATEST_VERSION);
        assert_eq!(
            db.get_setting::<String>("language").unwrap().as_deref(),
            Some("vi")
        );
    }

    #[test]
    fn refuses_database_from_newer_app() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("future.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(None, "user_version", LATEST_VERSION + 1)
                .unwrap();
        }
        let err = Storage::open(&path).err().unwrap();
        assert!(matches!(err, StorageError::SchemaTooNew { .. }));
    }
}
