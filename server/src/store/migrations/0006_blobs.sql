-- media-blobs task 3.1: blob metadata. The ciphertext itself is held by the
-- configured blob-store backend, never in this database (blob-store: Pluggable
-- storage backend). Times are Unix milliseconds.

-- blob-store: Upload; Expiry and ack-delete. One row per stored blob.
-- `sha256` is the hash of the ciphertext: it makes a re-upload idempotent and
-- lets a download notice bytes the backend changed or lost.
CREATE TABLE blobs (
    blob_id            BLOB PRIMARY KEY CHECK (length(blob_id) = 16),
    group_id           BLOB NOT NULL,
    uploader_device_id BLOB NOT NULL REFERENCES devices (device_id),
    size               INTEGER NOT NULL CHECK (size > 0),
    sha256             BLOB NOT NULL CHECK (length(sha256) = 32),
    uploaded_at        INTEGER NOT NULL,
    expires_at         INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

CREATE INDEX blobs_by_expiry ON blobs (expires_at);

-- blob-store: Expiry and ack-delete. The member devices that reported they no
-- longer need the blob. The uploader never has a row: it counts as acked.
CREATE TABLE blob_acks (
    blob_id   BLOB NOT NULL REFERENCES blobs (blob_id) ON DELETE CASCADE,
    device_id BLOB NOT NULL,
    PRIMARY KEY (blob_id, device_id)
) STRICT, WITHOUT ROWID;
