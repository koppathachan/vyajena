-- Change `dms`: the Pi holds the main group plus DMs (direct-messages; design
-- D8). Every group-scoped table gains `group_id`; rows from before belong to
-- the main group. Times are Unix milliseconds.

-- group-membership: Membership state on the Pi. One row per group. A DM names
-- its two users (usernames, sorted) and is closed when one is removed.
CREATE TABLE mls_groups (
    group_id        BLOB PRIMARY KEY,
    kind            TEXT NOT NULL CHECK (kind IN ('main', 'dm')),
    epoch           INTEGER NOT NULL,
    -- Position of the Pi in the group context's external_senders list.
    pi_sender_index INTEGER NOT NULL,
    created_at      INTEGER NOT NULL,
    dm_user_a       TEXT,
    dm_user_b       TEXT,
    closed_at       INTEGER,
    CHECK ((kind = 'main') = (dm_user_a IS NULL)),
    CHECK ((dm_user_a IS NULL) = (dm_user_b IS NULL)),
    CHECK (dm_user_a < dm_user_b),
    CHECK (kind = 'dm' OR closed_at IS NULL)
) STRICT;

-- group-membership: Group creation. At most one main group.
CREATE UNIQUE INDEX one_main_group ON mls_groups (kind) WHERE kind = 'main';

-- direct-messages: Opening a DM. At most one open DM per pair.
CREATE UNIQUE INDEX one_open_dm_per_pair ON mls_groups (dm_user_a, dm_user_b)
    WHERE kind = 'dm' AND closed_at IS NULL;

INSERT INTO mls_groups (group_id, kind, epoch, pi_sender_index, created_at)
SELECT group_id, 'main', epoch, pi_sender_index, created_at FROM mls_group;

DROP TABLE mls_group;

-- The OpenMLS PublicGroup storage, per group (design D8, spike S2).
CREATE TABLE mls_public_state_by_group (
    group_id BLOB NOT NULL REFERENCES mls_groups (group_id),
    key      BLOB NOT NULL,
    value    BLOB NOT NULL,
    PRIMARY KEY (group_id, key)
) STRICT, WITHOUT ROWID;

INSERT INTO mls_public_state_by_group (group_id, key, value)
SELECT g.group_id, s.key, s.value FROM mls_public_state s
JOIN mls_groups g ON g.kind = 'main';

DROP TABLE mls_public_state;
ALTER TABLE mls_public_state_by_group RENAME TO mls_public_state;

-- The member devices of each group and their leaf positions, rebuilt from the
-- tree after every accepted commit.
CREATE TABLE mls_members_by_group (
    group_id   BLOB NOT NULL REFERENCES mls_groups (group_id),
    leaf_index INTEGER NOT NULL,
    device_id  BLOB NOT NULL REFERENCES devices (device_id),
    PRIMARY KEY (group_id, leaf_index),
    UNIQUE (group_id, device_id)
) STRICT, WITHOUT ROWID;

INSERT INTO mls_members_by_group (group_id, leaf_index, device_id)
SELECT g.group_id, m.leaf_index, m.device_id FROM mls_members m
JOIN mls_groups g ON g.kind = 'main';

DROP TABLE mls_members;
ALTER TABLE mls_members_by_group RENAME TO mls_members;

CREATE INDEX mls_members_by_device ON mls_members (device_id);

-- message-delivery: Envelope. Each envelope's group; the one sequence stays
-- shared by all groups, so a device receives everything in one order.
ALTER TABLE envelopes ADD COLUMN group_id BLOB NOT NULL DEFAULT x'';
UPDATE envelopes SET group_id = (SELECT group_id FROM mls_groups WHERE kind = 'main');

-- message-delivery: Expiry and gap notice, per device and group.
CREATE TABLE gaps (
    device_id         BLOB NOT NULL REFERENCES devices (device_id),
    group_id          BLOB NOT NULL,
    newest_expired_at INTEGER NOT NULL,
    PRIMARY KEY (device_id, group_id)
) STRICT, WITHOUT ROWID;

INSERT INTO gaps (device_id, group_id, newest_expired_at)
SELECT c.device_id, g.group_id, c.gap_expired_at FROM cursors c
JOIN mls_groups g ON g.kind = 'main'
WHERE c.gap_expired_at IS NOT NULL;

ALTER TABLE cursors DROP COLUMN gap_expired_at;

-- group-membership: Inactive devices are kept out. The epoch of the Pi's
-- latest external Remove of an inactive device's leaf, per group. Deleted when
-- the leaf leaves the tree.
CREATE TABLE pi_removes (
    device_id      BLOB NOT NULL REFERENCES devices (device_id),
    group_id       BLOB NOT NULL REFERENCES mls_groups (group_id),
    proposed_epoch INTEGER NOT NULL,
    PRIMARY KEY (device_id, group_id)
) STRICT, WITHOUT ROWID;

INSERT INTO pi_removes (device_id, group_id, proposed_epoch)
SELECT i.device_id, g.group_id, i.proposed_epoch FROM inactive_devices i
JOIN mls_groups g ON g.kind = 'main'
WHERE i.proposed_epoch IS NOT NULL;

ALTER TABLE inactive_devices DROP COLUMN proposed_epoch;

-- direct-messages: Old apps never join a DM. Whether a KeyPackage's
-- capabilities list the DM extension 0xF0A2. Everything uploaded before is
-- from a 1.0.0 core.
ALTER TABLE key_packages ADD COLUMN dm_capable INTEGER NOT NULL DEFAULT 0
    CHECK (dm_capable IN (0, 1));

-- direct-messages: DM closed on user removal. A `DmClosed` each listed device
-- still has to acknowledge.
CREATE TABLE dm_closed_notices (
    device_id BLOB NOT NULL REFERENCES devices (device_id),
    group_id  BLOB NOT NULL REFERENCES mls_groups (group_id),
    PRIMARY KEY (device_id, group_id)
) STRICT, WITHOUT ROWID;
