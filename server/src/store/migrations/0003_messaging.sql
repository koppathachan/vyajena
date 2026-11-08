-- pi-server-core tasks 6.4-9.1: re-invitable usernames, the messaging write
-- path (sequence, envelopes, per-device deliveries, dedup), presence,
-- inactivity. Times are Unix milliseconds.

-- group-membership: Removing a user. Set by Admin.RemoveUser; a removed user's
-- username may be invited again, and the next signup replaces the identity key.
ALTER TABLE users ADD COLUMN removed_at INTEGER;

-- message-delivery: Envelope. The group's single sequence; the one row holds
-- the last server_seq handed out.
CREATE TABLE sequence (
    id              INTEGER PRIMARY KEY CHECK (id = 1),
    last_server_seq INTEGER NOT NULL
) STRICT;

INSERT INTO sequence (id, last_server_seq) VALUES (1, 0);

-- message-delivery: Per-device queues; Commit log. One row per accepted
-- envelope, the Pi's own external Remove proposals included
-- (sender_device_id NULL). `welcome` holds the Welcome of a COMMIT that adds
-- devices. A row is deleted once no delivery references it.
CREATE TABLE envelopes (
    server_seq       INTEGER PRIMARY KEY,
    msg_id           BLOB NOT NULL CHECK (length(msg_id) = 16),
    sender_device_id BLOB REFERENCES devices (device_id),
    -- vyajena.v1.Envelope.Kind: 1 APPLICATION, 2 COMMIT, 4 PROPOSAL.
    kind             INTEGER NOT NULL CHECK (kind IN (1, 2, 4)),
    epoch            INTEGER NOT NULL,
    ciphertext       BLOB NOT NULL,
    welcome          BLOB,
    accepted_at      INTEGER NOT NULL
) STRICT;

-- What each device still has to receive: its application queue entries and
-- its commit-log entries (COMMIT, PROPOSAL, and WELCOME in place of the commit
-- when `welcome` = 1). Deleted when the device acknowledges them; application
-- entries also on expiry.
CREATE TABLE deliveries (
    device_id  BLOB NOT NULL REFERENCES devices (device_id),
    server_seq INTEGER NOT NULL REFERENCES envelopes (server_seq),
    welcome    INTEGER NOT NULL CHECK (welcome IN (0, 1)),
    PRIMARY KEY (device_id, server_seq)
) STRICT, WITHOUT ROWID;

CREATE INDEX deliveries_by_seq ON deliveries (server_seq);

-- message-delivery: Acknowledgement. The delivery cursor per device: the
-- highest server_seq it acknowledged. `gap_expired_at` is the accepted_at of
-- the newest application entry that expired before the device was told
-- (Expiry and gap notice).
CREATE TABLE cursors (
    device_id      BLOB PRIMARY KEY REFERENCES devices (device_id),
    acked_seq      INTEGER NOT NULL,
    gap_expired_at INTEGER
) STRICT;

-- message-delivery: Idempotent sends. One row per accepted client envelope,
-- kept at least 7 days.
CREATE TABLE dedup (
    sender_device_id  BLOB NOT NULL,
    msg_id            BLOB NOT NULL,
    server_seq        INTEGER NOT NULL,
    accepted_at       INTEGER NOT NULL,
    ciphertext_sha256 BLOB NOT NULL,
    -- SHA-256 of the encoded WelcomeBundle, NULL when there was none.
    welcome_sha256    BLOB,
    PRIMARY KEY (sender_device_id, msg_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX dedup_by_time ON dedup (accepted_at);

-- group-membership: Inactive devices are kept out. When each device last had
-- an open Connect stream (or was added to the group). A device without a row
-- counts from its registration.
CREATE TABLE presence (
    device_id    BLOB PRIMARY KEY REFERENCES devices (device_id),
    last_seen_at INTEGER NOT NULL
) STRICT;

-- group-membership: Inactive devices are kept out; Automatic re-add. A row
-- per device marked inactive, deleted when a commit adds it again.
CREATE TABLE inactive_devices (
    device_id      BLOB PRIMARY KEY REFERENCES devices (device_id),
    since          INTEGER NOT NULL,
    -- RemovedNotice.missed_since: accepted_at of the oldest application entry
    -- deleted from its queue, else `since`.
    missed_since   INTEGER NOT NULL,
    -- The epoch of the Pi's latest external Remove proposal for its leaf.
    proposed_epoch INTEGER,
    -- When it first reconnected after being marked inactive.
    returned_at    INTEGER
) STRICT;
