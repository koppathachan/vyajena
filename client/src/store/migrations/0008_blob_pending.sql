-- media-blobs (owner, queue images offline): an own image message waits in the
-- outbox like text, and its blob is uploaded at send time, before the
-- envelope. `blob_pending` is 1 from the moment the message is stored until
-- the Pi accepted the blob; the outbox sends nothing for it until then.
ALTER TABLE messages ADD COLUMN blob_pending INTEGER NOT NULL DEFAULT 0
    CHECK (blob_pending IN (0, 1));
