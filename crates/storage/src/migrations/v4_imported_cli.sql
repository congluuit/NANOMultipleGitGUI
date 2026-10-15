-- v4: `accounts.auth_method` accepts 'imported_cli' (token copied from the gh / glab CLI).
-- Never edit once shipped; add a new migration instead.
-- SQLite cannot alter a CHECK constraint, so the table is rebuilt (sqlite.org/lang_altertable.html
-- "Making Other Kinds Of Table Schema Changes"). `migrate` turns foreign keys off around the
-- transaction: with them on, dropping `accounts` would cascade-delete identities, rules and
-- bindings. Child tables reference `accounts` by name and keep pointing at the rebuilt table.

CREATE TABLE accounts_v4 (
    id                TEXT PRIMARY KEY NOT NULL,
    -- RESTRICT: deleting a provider must not silently delete its accounts.
    provider_id       TEXT NOT NULL REFERENCES providers (id) ON DELETE RESTRICT,
    username          TEXT NOT NULL CHECK (username <> ''),
    display_name      TEXT,
    avatar_url        TEXT,
    auth_method       TEXT NOT NULL
                      CHECK (auth_method IN ('oauth', 'pat', 'ssh_only', 'imported_cli')),
    scopes            TEXT NOT NULL DEFAULT '[]'
                      CHECK (json_valid(scopes) AND json_type(scopes) = 'array'),
    token_expires_at  INTEGER,
    color             TEXT NOT NULL
                      CHECK (color GLOB '#[0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]'),
    label             TEXT,
    default_clone_dir TEXT,
    -- Names of the keychain entries that exist (`token`, `refresh_token`), never values.
    secrets           TEXT NOT NULL DEFAULT '[]'
                      CHECK (json_valid(secrets) AND json_type(secrets) = 'array'),
    sort_order        INTEGER NOT NULL DEFAULT 0,
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL
) STRICT;

INSERT INTO accounts_v4
    (id, provider_id, username, display_name, avatar_url, auth_method, scopes, token_expires_at,
     color, label, default_clone_dir, secrets, sort_order, created_at, updated_at)
SELECT id, provider_id, username, display_name, avatar_url, auth_method, scopes, token_expires_at,
       color, label, default_clone_dir, secrets, sort_order, created_at, updated_at
FROM accounts;

DROP TABLE accounts;
ALTER TABLE accounts_v4 RENAME TO accounts;
CREATE UNIQUE INDEX accounts_provider_user ON accounts (provider_id, username COLLATE NOCASE);
