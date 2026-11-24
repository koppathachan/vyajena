-- Change `remove-closed-dm`: a user's private removed-conversation set
-- (message-deletion: Removed-conversation sync). Conversation ids only, never
-- content: the server holds no history. Written by `Dms.RemoveClosedDm`, sent to
-- that user's devices on every connect.
CREATE TABLE removed_conversations (
    username TEXT NOT NULL,
    group_id BLOB NOT NULL,
    PRIMARY KEY (username, group_id)
) STRICT, WITHOUT ROWID;
