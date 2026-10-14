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
    let tx = conn.transaction()?;
    apply(&tx, current)?;
    tx.commit()?;
    Ok(LATEST_VERSION)
}

fn apply(tx: &Transaction<'_>, from: u32) -> Result<()> {
    for sql in &MIGRATIONS[from as usize..] {
        tx.execute_batch(sql)?;
    }
    tx.pragma_update(None, "user_version", LATEST_VERSION)?;
    Ok(())
}
