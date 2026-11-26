-- Change `manual-presence`: this device's view of manual presence
-- (manual-presence: Setting your status; Latest value wins).
--
-- `presence_status` wire values: 1 = ONLINE, 2 = OFFLINE.

-- This user's own manual flag as this device holds it, and whether a change the
-- user made has not yet been sent to the server (set while offline). A single
-- row (id = 1). The default is Offline (the first-launch default,
-- `protocol::presence::DEFAULT_PRESENCE`); `dirty` 1 means "send on next
-- connect", so an offline toggle still reaches the server, while a mere
-- reconnect does not re-assert and clobber another device's newer change.
CREATE TABLE self_presence (
    id      INTEGER PRIMARY KEY CHECK (id = 1),
    status  INTEGER NOT NULL DEFAULT 2 CHECK (status IN (1, 2)),
    dirty   INTEGER NOT NULL DEFAULT 0 CHECK (dirty IN (0, 1))
) STRICT;
INSERT INTO self_presence (id, status, dirty) VALUES (1, 2, 0);

-- The latest manual flag this device has learned for each other user, from a
-- `PresenceUpdate` (a live change or the on-connect snapshot). The client keeps
-- only the latest value per user and applies a `PresenceUpdate` only if its
-- `updated_at` is newer than the one stored here (manual-presence: Latest value
-- wins). A user absent from this table has an unknown status and shows no dot.
CREATE TABLE peer_presence (
    user_id         TEXT PRIMARY KEY,
    status          INTEGER NOT NULL CHECK (status IN (1, 2)),
    updated_at_ms   INTEGER NOT NULL
) STRICT, WITHOUT ROWID;
