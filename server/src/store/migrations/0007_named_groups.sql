-- Change `lobby-multi-group`: the main group becomes the system lobby (kind
-- `system`) and the admin can create named groups (kind `named`). SQLite cannot
-- change a CHECK, so `mls_groups` is rebuilt; the runner turns foreign keys off
-- around migrations and checks them before the commit (design D9).

CREATE TABLE mls_groups_new (
    group_id        BLOB PRIMARY KEY,
    kind            TEXT NOT NULL CHECK (kind IN ('system', 'named', 'dm')),
    epoch           INTEGER NOT NULL,
    pi_sender_index INTEGER NOT NULL,
    created_at      INTEGER NOT NULL,
    dm_user_a       TEXT,
    dm_user_b       TEXT,
    closed_at       INTEGER,
    CHECK ((kind = 'dm') = (dm_user_a IS NOT NULL)),
    CHECK ((dm_user_a IS NULL) = (dm_user_b IS NULL)),
    CHECK (dm_user_a < dm_user_b),
    CHECK (kind <> 'system' OR closed_at IS NULL)
) STRICT;

INSERT INTO mls_groups_new (group_id, kind, epoch, pi_sender_index, created_at,
                            dm_user_a, dm_user_b, closed_at)
SELECT group_id, CASE kind WHEN 'main' THEN 'system' ELSE kind END, epoch,
       pi_sender_index, created_at, dm_user_a, dm_user_b, closed_at
FROM mls_groups;

DROP TABLE mls_groups;
ALTER TABLE mls_groups_new RENAME TO mls_groups;

-- group-membership: Group creation. At most one system group.
CREATE UNIQUE INDEX one_system_group ON mls_groups (kind) WHERE kind = 'system';

-- direct-messages: Opening a DM. At most one open DM per pair.
CREATE UNIQUE INDEX one_open_dm_per_pair ON mls_groups (dm_user_a, dm_user_b)
    WHERE kind = 'dm' AND closed_at IS NULL;

-- multi-group: Named groups reach only v3 apps. Whether a KeyPackage's
-- capabilities list 0xF0A3. Everything uploaded before is from an older core.
ALTER TABLE key_packages ADD COLUMN named_group_capable INTEGER NOT NULL DEFAULT 0
    CHECK (named_group_capable IN (0, 1));
