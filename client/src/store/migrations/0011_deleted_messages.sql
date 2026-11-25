-- message-deletion: Tombstone, not erasure (change `delete-messages`). A
-- honoured delete marks its target message a tombstone in place; the row stays
-- so ordering and reply threads are intact. For a DM the original plaintext is
-- kept in `content` so the receiver may Reveal it; for the lobby and named
-- groups the plaintext is replaced by the Delete body (no reveal).
ALTER TABLE messages ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0
    CHECK (deleted IN (0, 1));

-- 1 when an admin deleted another user's message in the lobby or a named group,
-- so the tombstone reads "This message was deleted by the admin".
ALTER TABLE messages ADD COLUMN deleted_by_admin INTEGER NOT NULL DEFAULT 0
    CHECK (deleted_by_admin IN (0, 1));

-- 1 for a stored `Content.Delete` message whose target this device already
-- holds: the target row carries the tombstone instead, so this delete row is
-- kept (for dedup) but not shown in the history. A delete whose target this
-- device never received stays 0 and is itself the tombstone.
ALTER TABLE messages ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0
    CHECK (hidden IN (0, 1));
