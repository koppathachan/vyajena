-- Change `mute-notifications`: this device's copy of the user's mute set
-- (notification-prefs: Mute preference sync). Adopted wholesale from each
-- `MutePrefs` snapshot and updated on the user's own `SetMutePref`. Conversation
-- ids and one flag only.
CREATE TABLE muted_conversations (
    group_id BLOB NOT NULL PRIMARY KEY
) STRICT, WITHOUT ROWID;

-- The whole-app mute flag: the single row (id = 1) exists only while it is set.
CREATE TABLE muted_all (
    id INTEGER NOT NULL PRIMARY KEY CHECK (id = 1)
) STRICT, WITHOUT ROWID;
