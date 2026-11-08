-- pi-runtime and device-identity: what the first start needs. Later tasks add
-- devices, KeyPackages, the group, queues and the commit log in new files.
-- Times are milliseconds since the Unix epoch.

-- Facts fixed at first start, such as the server id every user id contains.
CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;

-- One row per user. The user id is `<username>@<server_id>`; the server id is
-- the same for every row (meta 'server_id').
CREATE TABLE users (
    username            TEXT PRIMARY KEY,
    identity_public_key BLOB NOT NULL,
    created_at          INTEGER NOT NULL
) STRICT;

-- device-identity: Invites; Bootstrap admin. Single use: `used_at` or
-- `revoked_at` set means it can no longer be used.
CREATE TABLE invites (
    code       TEXT PRIMARY KEY,
    username   TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    used_at    INTEGER,
    revoked_at INTEGER,
    bootstrap  INTEGER NOT NULL DEFAULT 0 CHECK (bootstrap IN (0, 1))
) STRICT;

CREATE INDEX invites_by_username ON invites (username);
