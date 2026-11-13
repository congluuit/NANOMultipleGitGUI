-- v5: `providers.kind` also accepts 'gitea', 'bitbucket' and 'azure_devops'.
-- Never edit once shipped; add a new migration instead.
-- SQLite cannot alter a CHECK constraint, so the table is rebuilt like `accounts` in v4
-- (`migrate` turns foreign keys off around the transaction; `accounts.provider_id` references
-- `providers` by name and keeps pointing at the rebuilt table).

CREATE TABLE providers_v5 (
    id                  TEXT PRIMARY KEY NOT NULL,
    kind                TEXT NOT NULL
                        CHECK (kind IN ('github', 'github_enterprise', 'gitlab', 'gitlab_self_hosted',
                                        'gitea', 'bitbucket', 'azure_devops')),
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

INSERT INTO providers_v5
    (id, kind, name, base_url, api_url, oauth_client_id, large_file_warn_mb, large_file_block_mb,
     builtin, created_at)
SELECT id, kind, name, base_url, api_url, oauth_client_id, large_file_warn_mb, large_file_block_mb,
       builtin, created_at
FROM providers;

DROP TABLE providers;
ALTER TABLE providers_v5 RENAME TO providers;
