-- Change `mute-notifications`: a user's private notification mute set
-- (notification-prefs: Mute preference sync). Conversation ids and one flag
-- only, never content: the server holds no history. Written by
-- `Prefs.SetMutePref`, sent to that user's devices on every connect, and
-- consulted before every wake push.

-- The muted conversations of a user (notification-prefs: Mute a conversation).
CREATE TABLE muted_conversations (
    username TEXT NOT NULL,
    group_id BLOB NOT NULL,
    PRIMARY KEY (username, group_id)
) STRICT, WITHOUT ROWID;

-- The whole-app mute flag of a user (notification-prefs: Mute the whole app).
-- A row exists only while the user has `muted_all` set; clearing it deletes the
-- row.
CREATE TABLE muted_all (
    username TEXT NOT NULL PRIMARY KEY
) STRICT, WITHOUT ROWID;
