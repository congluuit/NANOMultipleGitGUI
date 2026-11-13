use rusqlite::{Connection, Transaction};

use crate::Result;

/// Ordered schema migrations. Index `i` upgrades the schema from version `i` to `i + 1`.
/// Never edit or reorder an entry that has shipped; append a new one instead.
pub(crate) const MIGRATIONS: &[&str] = &[
    // v1: key/value settings, values stored as JSON.
    "CREATE TABLE settings (
        key        TEXT PRIMARY KEY NOT NULL,
        value      TEXT NOT NULL CHECK (json_valid(value)),
        updated_at INTEGER NOT NULL DEFAULT (unixepoch())
    ) STRICT;",
    // v2: repositories the user has opened. `path_key` is the comparison form of `path`
    // (case-folded on case-insensitive file systems) so one folder maps to one row.
    "CREATE TABLE repos (
        id             INTEGER PRIMARY KEY,
        path           TEXT NOT NULL,
        path_key       TEXT NOT NULL UNIQUE,
        name           TEXT NOT NULL,
        last_opened_at INTEGER NOT NULL
    ) STRICT;
    CREATE INDEX repos_last_opened ON repos (last_opened_at DESC);",
    // v3: multi-account model (PLAN §4.1). Secrets are never stored here: `accounts.secrets`
    // only lists which keychain entries exist. Text ids are random (`acc_…`) so a keychain
    // entry can never be re-paired with a different account after a database reset.
    include_str!("migrations/v3_accounts.sql"),
    // v4: `auth_method = 'imported_cli'` (tokens copied from gh / glab, PLAN 3B.1). Rebuilds
    // `accounts`; needs foreign keys off (see `migrate`).
    include_str!("migrations/v4_imported_cli.sql"),
    // v5: provider kinds 'gitea', 'bitbucket' and 'azure_devops'. Rebuilds `providers`.
    include_str!("migrations/v5_more_providers.sql"),
];

pub const LATEST_VERSION: u32 = MIGRATIONS.len() as u32;

pub(crate) fn schema_version(conn: &Connection) -> Result<u32> {
    Ok(conn.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

/// Applies every pending migration in a single transaction.
pub(crate) fn migrate(conn: &mut Connection) -> Result<u32> {
    let current = schema_version(conn)?;
    if current > LATEST_VERSION {
        return Err(crate::StorageError::SchemaTooNew {
            found: current,
            supported: LATEST_VERSION,
        });
    }
    if current == LATEST_VERSION {
        return Ok(current);
    }
    // Table rebuilds (v4) drop and recreate a parent table: with foreign keys on, the DROP
    // would run the children's ON DELETE actions. The pragma is a no-op inside a transaction,
    // so it is switched around it and the references are checked before committing.
    let fk_on: bool = conn.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
    conn.pragma_update(None, "foreign_keys", false)?;
    let result = (|| -> Result<()> {
        let tx = conn.transaction()?;
        apply(&tx, current)?;
        tx.commit()?;
        Ok(())
    })();
    conn.pragma_update(None, "foreign_keys", fk_on)?;
    result?;
    Ok(LATEST_VERSION)
}

fn apply(tx: &Transaction<'_>, from: u32) -> Result<()> {
    for sql in &MIGRATIONS[from as usize..] {
        tx.execute_batch(sql)?;
    }
    let broken: i64 = tx.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
        r.get(0)
    })?;
    if broken > 0 {
        return Err(crate::StorageError::Conflict(format!(
            "migration left {broken} broken foreign key reference(s)"
        )));
    }
    tx.pragma_update(None, "user_version", LATEST_VERSION)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A database as version 4 of the schema left it, with a provider and an account in it.
    fn v4_database() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        for sql in &MIGRATIONS[..4] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 4).unwrap();
        conn.execute_batch(
            "INSERT INTO providers (id, kind, name, base_url, api_url, large_file_warn_mb, builtin, created_at)
               VALUES ('prv_1', 'gitlab_self_hosted', 'GitLab acme', 'https://git.acme.vn', 'https://git.acme.vn/api/v4', 50, 0, 7);
             INSERT INTO accounts (id, provider_id, username, auth_method, color, created_at, updated_at)
               VALUES ('acc_1', 'prv_1', 'lan', 'pat', '#1c7ed6', 1, 1);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn v5_accepts_the_new_provider_kinds_and_keeps_existing_rows() {
        let mut conn = v4_database();
        // Before: the old CHECK refuses the new kinds.
        let insert = "INSERT INTO providers (id, kind, name, base_url, api_url, large_file_warn_mb, builtin, created_at)
                      VALUES (?1, ?2, ?1, ?3, ?3, 50, 0, 0)";
        assert!(
            conn.execute(insert, ["prv_bb", "bitbucket", "https://bitbucket.org"])
                .is_err()
        );

        assert_eq!(migrate(&mut conn).unwrap(), LATEST_VERSION);
        for (id, kind, url) in [
            ("prv_bb", "bitbucket", "https://bitbucket.org"),
            ("prv_az", "azure_devops", "https://dev.azure.com"),
            ("prv_gt", "gitea", "https://codeberg.org"),
        ] {
            conn.execute(insert, [id, kind, url]).unwrap();
        }
        assert!(
            conn.execute(insert, ["prv_x", "svn", "https://x.test"])
                .is_err()
        );

        // Rows, the built-in providers and the account's reference survived the rebuild.
        let (kind, created): (String, i64) = conn
            .query_row(
                "SELECT kind, created_at FROM providers WHERE id = 'prv_1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((kind.as_str(), created), ("gitlab_self_hosted", 7));
        let builtin: i64 = conn
            .query_row(
                "SELECT count(*) FROM providers WHERE builtin = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(builtin, 2);
        let owner: String = conn
            .query_row(
                "SELECT p.name FROM accounts a JOIN providers p ON p.id = a.provider_id WHERE a.id = 'acc_1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(owner, "GitLab acme");
        // Foreign keys are back on and still protect a provider that has accounts.
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
        assert!(
            conn.execute("DELETE FROM providers WHERE id = 'prv_1'", [])
                .is_err()
        );
        // The base URL stays unique whatever the case.
        assert!(
            conn.execute(insert, ["prv_dup", "gitea", "HTTPS://CODEBERG.ORG"])
                .is_err()
        );
    }
}
