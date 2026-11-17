-- Change `dms`: the DMs this device is in, messages per chat, read marks
-- and one-time upgrade flags (direct-messages: Several groups in the core;
-- design D9). Times are milliseconds since the Unix epoch.

-- One row per DM this device opened, joined or waits to join. `chat_id` is
-- the chat's id in the API; 0 is the main group, which keeps `group_state`.
-- Group secrets are in `mls_storage`.
CREATE TABLE dm_groups (
    chat_id             INTEGER PRIMARY KEY CHECK (chat_id > 0),
    group_id            BLOB NOT NULL UNIQUE,
    -- `<username>@<server_id>` of the other user.
    peer                TEXT NOT NULL,
    created_at          INTEGER NOT NULL,
    -- 1 once this device has the DM's MLS state (created it or joined from a
    -- Welcome); 0 while it waits for its Welcome.
    joined              INTEGER NOT NULL CHECK (joined IN (0, 1)),
    own_leaf_updated_at INTEGER NOT NULL,
    -- The highest server_seq of this DM durably processed.
    processed_seq       INTEGER NOT NULL DEFAULT 0,
    -- Why this device stopped following the DM, until a Welcome.
    stalled             TEXT,
    -- direct-messages: DM closed on user removal.
    closed_at           INTEGER
) STRICT;

-- At most one open DM per peer.
CREATE UNIQUE INDEX one_open_dm_per_peer ON dm_groups (peer) WHERE closed_at IS NULL;

-- Which chat each message is in: 0 is the main group.
ALTER TABLE messages ADD COLUMN chat_id INTEGER NOT NULL DEFAULT 0;

CREATE INDEX messages_by_chat ON messages (chat_id, server_seq, local_id);

-- app-ui: Chat list. The newest message the user has seen in each chat.
CREATE TABLE chat_reads (
    chat_id    INTEGER PRIMARY KEY,
    read_up_to INTEGER NOT NULL
) STRICT;

-- One-time steps done after an upgrade (direct-messages: Old apps never join
-- a DM: the first run's fresh KeyPackages).
CREATE TABLE core_flags (
    name   TEXT PRIMARY KEY,
    set_at INTEGER NOT NULL
) STRICT;
