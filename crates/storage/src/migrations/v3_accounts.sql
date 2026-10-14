-- v3: providers, accounts, identities, SSH key metadata, rules, repo bindings, workspaces.
-- Never edit once shipped; add a new migration instead.
-- Booleans are INTEGER 0/1, timestamps are milliseconds since the Unix epoch.

CREATE TABLE providers (
    id                  TEXT PRIMARY KEY NOT NULL,
    kind                TEXT NOT NULL
                        CHECK (kind IN ('github', 'github_enterprise', 'gitlab', 'gitlab_self_hosted')),
    name                TEXT NOT NULL,
    base_url            TEXT NOT NULL COLLATE NOCASE UNIQUE,
    api_url             TEXT NOT NULL,
    oauth_client_id     TEXT,
    large_file_warn_mb  INTEGER NOT NULL CHECK (large_file_warn_mb > 0),
    large_file_block_mb INTEGER
                        CHECK (large_file_block_mb IS NULL OR large_file_block_mb >= large_file_warn_mb),
    builtin             INTEGER NOT NULL DEFAULT 0 CHECK (builtin IN (0, 1)),
    created_at          INTEGER NOT NULL
) STRICT;

INSERT INTO providers
    (id, kind, name, base_url, api_url, oauth_client_id, large_file_warn_mb, large_file_block_mb, builtin, created_at)
VALUES
    ('github', 'github', 'GitHub', 'https://github.com', 'https://api.github.com', NULL, 50, 100, 1, 0),
    ('gitlab', 'gitlab', 'GitLab.com', 'https://gitlab.com', 'https://gitlab.com/api/v4', NULL, 50, NULL, 1, 0);

CREATE TABLE accounts (
    id                TEXT PRIMARY KEY NOT NULL,
    -- RESTRICT: deleting a provider must not silently delete its accounts.
    provider_id       TEXT NOT NULL REFERENCES providers (id) ON DELETE RESTRICT,
    username          TEXT NOT NULL CHECK (username <> ''),
    display_name      TEXT,
    avatar_url        TEXT,
    auth_method       TEXT NOT NULL CHECK (auth_method IN ('oauth', 'pat', 'ssh_only')),
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
CREATE UNIQUE INDEX accounts_provider_user ON accounts (provider_id, username COLLATE NOCASE);

CREATE TABLE identities (
    id              TEXT PRIMARY KEY NOT NULL,
    account_id      TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    name            TEXT NOT NULL CHECK (name <> ''),
    email           TEXT NOT NULL CHECK (email LIKE '_%@_%'),
    signing         TEXT NOT NULL DEFAULT 'none' CHECK (signing IN ('none', 'gpg', 'ssh')),
    signing_key_ref TEXT,
    is_default      INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    created_at      INTEGER NOT NULL,
    CHECK (signing = 'none' OR signing_key_ref IS NOT NULL)
) STRICT;
CREATE UNIQUE INDEX identities_account_email ON identities (account_id, email COLLATE NOCASE);
CREATE UNIQUE INDEX identities_one_default ON identities (account_id) WHERE is_default = 1;

CREATE TABLE ssh_keys (
    id                   TEXT PRIMARY KEY NOT NULL,
    -- SET NULL: the key file outlives the account; the user can reassign it.
    account_id           TEXT REFERENCES accounts (id) ON DELETE SET NULL,
    key_type             TEXT NOT NULL,
    fingerprint          TEXT NOT NULL UNIQUE,
    public_key           TEXT NOT NULL,
    private_key_path     TEXT,
    has_passphrase       INTEGER NOT NULL DEFAULT 0 CHECK (has_passphrase IN (0, 1)),
    uploaded_to_provider INTEGER NOT NULL DEFAULT 0 CHECK (uploaded_to_provider IN (0, 1)),
    provider_key_id      TEXT,
    comment              TEXT,
    created_at           INTEGER NOT NULL
) STRICT;
CREATE INDEX ssh_keys_account ON ssh_keys (account_id);

CREATE TABLE rules (
    id             TEXT PRIMARY KEY NOT NULL,
    priority       INTEGER NOT NULL,
    host           TEXT,
    owner_or_group TEXT,
    path_glob      TEXT,
    remote_regex   TEXT,
    account_id     TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    identity_id    TEXT REFERENCES identities (id) ON DELETE SET NULL,
    enabled        INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at     INTEGER NOT NULL,
    CHECK (COALESCE(host, owner_or_group, path_glob, remote_regex) IS NOT NULL)
) STRICT;
CREATE INDEX rules_order ON rules (priority, created_at);
CREATE INDEX rules_account ON rules (account_id);
CREATE INDEX rules_identity ON rules (identity_id);

-- `remote_name = ''` is the repo-level default (NULL would break the primary key).
CREATE TABLE repo_bindings (
    repo_id     INTEGER NOT NULL REFERENCES repos (id) ON DELETE CASCADE,
    remote_name TEXT NOT NULL DEFAULT '',
    account_id  TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    identity_id TEXT REFERENCES identities (id) ON DELETE SET NULL,
    transport   TEXT CHECK (transport IS NULL OR transport IN ('https', 'ssh')),
    source      TEXT NOT NULL CHECK (source IN ('auto_rule', 'manual', 'clone')),
    locked      INTEGER NOT NULL DEFAULT 0 CHECK (locked IN (0, 1)),
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (repo_id, remote_name)
) STRICT;
CREATE INDEX repo_bindings_account ON repo_bindings (account_id);
CREATE INDEX repo_bindings_identity ON repo_bindings (identity_id);

-- An identity named by a rule or binding must belong to that rule's/binding's account.
CREATE TRIGGER rules_identity_owner_insert BEFORE INSERT ON rules
WHEN NEW.identity_id IS NOT NULL AND NOT EXISTS
    (SELECT 1 FROM identities WHERE id = NEW.identity_id AND account_id = NEW.account_id)
BEGIN
    SELECT RAISE(ABORT, 'identity does not belong to the account');
END;
CREATE TRIGGER rules_identity_owner_update BEFORE UPDATE OF identity_id, account_id ON rules
WHEN NEW.identity_id IS NOT NULL AND NOT EXISTS
    (SELECT 1 FROM identities WHERE id = NEW.identity_id AND account_id = NEW.account_id)
BEGIN
    SELECT RAISE(ABORT, 'identity does not belong to the account');
END;
CREATE TRIGGER repo_bindings_identity_owner_insert BEFORE INSERT ON repo_bindings
WHEN NEW.identity_id IS NOT NULL AND NOT EXISTS
    (SELECT 1 FROM identities WHERE id = NEW.identity_id AND account_id = NEW.account_id)
BEGIN
    SELECT RAISE(ABORT, 'identity does not belong to the account');
END;
CREATE TRIGGER repo_bindings_identity_owner_update BEFORE UPDATE OF identity_id, account_id ON repo_bindings
WHEN NEW.identity_id IS NOT NULL AND NOT EXISTS
    (SELECT 1 FROM identities WHERE id = NEW.identity_id AND account_id = NEW.account_id)
BEGIN
    SELECT RAISE(ABORT, 'identity does not belong to the account');
END;

CREATE TABLE workspaces (
    id                 TEXT PRIMARY KEY NOT NULL,
    name               TEXT NOT NULL CHECK (name <> ''),
    default_account_id TEXT REFERENCES accounts (id) ON DELETE SET NULL,
    created_at         INTEGER NOT NULL
) STRICT;
CREATE UNIQUE INDEX workspaces_name ON workspaces (name COLLATE NOCASE);

CREATE TABLE workspace_repos (
    workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    repo_id      INTEGER NOT NULL REFERENCES repos (id) ON DELETE CASCADE,
    position     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (workspace_id, repo_id)
) STRICT;
CREATE INDEX workspace_repos_repo ON workspace_repos (repo_id);
