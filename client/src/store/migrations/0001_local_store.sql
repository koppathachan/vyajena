-- client-runtime: Local message store; Key storage (client-core design D3).
-- Later task groups add files; an applied migration is never edited.
-- Times are milliseconds since the Unix epoch.

-- One row per message this device sent or received. Received messages are
-- stored before they are acked (`server_seq` is unique, so a redelivery after a
-- crash is seen once). Own messages are stored, with their plaintext, before
-- they are sent: the sender cannot decrypt its own ciphertext.
CREATE TABLE messages (
    local_id      INTEGER PRIMARY KEY,
    -- `<username>@<server_id>` of the sending user.
    sender_user   TEXT NOT NULL,
    -- The sending device's 16-byte device id.
    sender_device BLOB NOT NULL,
    -- The client-chosen 16-byte msg_id (message-delivery: Idempotent sends).
    msg_id        BLOB NOT NULL,
    -- 1 when this device sent it.
    outgoing      INTEGER NOT NULL CHECK (outgoing IN (0, 1)),
    -- The encoded vyajena.v1.Content (plaintext), kept whole so a body this
    -- version does not know can be shown by a later one.
    content       BLOB NOT NULL,
    -- Local clock: when this device stored it.
    created_at    INTEGER NOT NULL,
    -- Set by the Pi; NULL while an own message is sending or failed.
    server_seq    INTEGER UNIQUE,
    accepted_at   INTEGER,
    -- 'received' for incoming; 'sending', 'accepted' or 'failed' for own.
    status        TEXT NOT NULL CHECK (status IN ('received', 'sending', 'accepted', 'failed')),
    -- Why an own message failed.
    failure       TEXT,
    -- Own message in flight: the epoch and exact envelope ciphertext last sent,
    -- so a resend after a crash repeats identical bytes (a retry with other
    -- ciphertext would be REJECTION_REASON_DUPLICATE_MISMATCH). Cleared once
    -- accepted. Ciphertext only; never plaintext or keys.
    sent_epoch    INTEGER,
    ciphertext    BLOB,
    UNIQUE (sender_device, msg_id),
    CHECK ((outgoing = 1) = (status != 'received'))
) STRICT;

-- key-directory: Clients verify the identity chain. Identity keys pinned on
-- first sight. A different key for a pinned user waits in `pending_key` until
-- the user accepts it in the UI.
CREATE TABLE identity_keys (
    user_id       TEXT PRIMARY KEY,
    identity_key  BLOB NOT NULL,
    pinned_at     INTEGER NOT NULL,
    pending_key   BLOB,
    pending_since INTEGER,
    CHECK ((pending_key IS NULL) = (pending_since IS NULL))
) STRICT;

-- OpenMLS group state (design D3): `slot` is an HMAC of the OpenMLS label and
-- key, `value` the serialized value sealed with XChaCha20-Poly1305 under the
-- storage key, which lives in the key store. Unreadable without it.
CREATE TABLE mls_storage (
    slot  BLOB PRIMARY KEY,
    value BLOB NOT NULL
) STRICT, WITHOUT ROWID;
