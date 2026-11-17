-- mentions D5: a received message from another user that mentions this
-- device's user or everyone is stored with mention_unseen = 1 until the app
-- marks it seen. Existing messages count as seen.
ALTER TABLE messages ADD COLUMN mention_unseen INTEGER NOT NULL DEFAULT 0
    CHECK (mention_unseen IN (0, 1));
CREATE INDEX messages_mention_unseen ON messages (mention_unseen) WHERE mention_unseen = 1;
