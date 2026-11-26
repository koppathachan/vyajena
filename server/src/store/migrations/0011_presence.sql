-- Change `manual-presence`: a user's manual online/offline flag
-- (manual-presence: Durable per-user status storage). A single two-value flag
-- per user, stored on the users row like any other small per-user marker. The
-- server writes it only from a `SetPresence` by that user's authenticated device
-- (or the first-launch default) and never derives it from connection,
-- heartbeats, reads or any activity (manual-presence: Manual status, never
-- inferred). It is never cleared when the user's last device disconnects.

-- The wire `PresenceStatus` value: 1 = ONLINE, 2 = OFFLINE. The column default
-- is the owner-chosen first-launch default (D2 = Offline), so a user who has
-- never sent a `SetPresence` is stored and reported Offline
-- (`protocol::presence::DEFAULT_PRESENCE`).
ALTER TABLE users ADD COLUMN manual_presence INTEGER NOT NULL DEFAULT 2
    CHECK (manual_presence IN (1, 2));

-- When the server last accepted a `SetPresence` for this user (ms since the Unix
-- epoch), server-stamped. 0 for a user who never set one, so any real
-- `SetPresence` supersedes it under the latest-`updated_at`-wins rule
-- (manual-presence: Latest value wins).
ALTER TABLE users ADD COLUMN presence_updated_at INTEGER NOT NULL DEFAULT 0;
