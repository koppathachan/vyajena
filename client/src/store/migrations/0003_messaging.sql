-- client-core task group 6: what kind of body each message holds, and the
-- state that bodies change (message-content). Times are milliseconds since
-- the Unix epoch.

-- 'chat' for Text and bodies this version does not know (shown in the
-- history), 'profile' and 'settings' for Profile and GroupSettings (which
-- change state instead).
ALTER TABLE messages ADD COLUMN kind TEXT NOT NULL DEFAULT 'chat'
    CHECK (kind IN ('chat', 'profile', 'settings'));

-- message-content: User-owned profiles. The last Profile each user's devices
-- sent; applied only to the user whose device sent it.
CREATE TABLE profiles (
    user_id      TEXT PRIMARY KEY,
    display_name TEXT,
    about        TEXT,
    updated_at   INTEGER NOT NULL
) STRICT;

-- message-content: Group settings travel encrypted. The last GroupSettings an
-- admin device sent (one row).
CREATE TABLE group_settings (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    name       TEXT,
    updated_at INTEGER NOT NULL
) STRICT;

-- The engine looks for own messages still sending on every tick.
CREATE INDEX messages_sending ON messages (local_id) WHERE status = 'sending';
