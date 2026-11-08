-- pi-server-core tasks 4-6: devices and authentication, the KeyPackage
-- directory, and the group's public state. Times are Unix milliseconds.

-- device-identity: User identity key and device certificates. One row per
-- device; `device_id` is derived from the auth key, so it is unique across
-- users. A revoked device keeps its row so it can never authenticate again.
CREATE TABLE devices (
    device_id                BLOB PRIMARY KEY CHECK (length(device_id) = 16),
    username                 TEXT NOT NULL REFERENCES users (username),
    auth_public_key          BLOB NOT NULL,
    mls_signature_public_key BLOB NOT NULL,
    -- SignedDeviceCertificate.certificate and .signature, exactly as received.
    certificate              BLOB NOT NULL,
    certificate_signature    BLOB NOT NULL,
    created_at               INTEGER NOT NULL,
    revoked_at               INTEGER,
    -- device-identity: Push token registration. Both set or both NULL.
    push_platform            INTEGER,
    push_token               TEXT,
    CHECK ((push_platform IS NULL) = (push_token IS NULL))
) STRICT;

CREATE INDEX devices_by_username ON devices (username);

-- wire-transport: Session authentication. Challenge nonces: single use (the
-- row is deleted when answered) and valid until `expires_at`.
CREATE TABLE challenges (
    nonce      BLOB PRIMARY KEY,
    issued_at  INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
) STRICT;

-- wire-transport: Session authentication. Only SHA-256 of each token is stored.
CREATE TABLE sessions (
    token_sha256 BLOB PRIMARY KEY,
    device_id    BLOB NOT NULL REFERENCES devices (device_id),
    created_at   INTEGER NOT NULL,
    expires_at   INTEGER NOT NULL
) STRICT;

CREATE INDEX sessions_by_device ON sessions (device_id);

-- key-directory: KeyPackage upload and fetch. One-time rows are deleted when
-- handed out; each device has at most one last-resort row.
CREATE TABLE key_packages (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id   BLOB NOT NULL REFERENCES devices (device_id),
    key_package BLOB NOT NULL,
    last_resort INTEGER NOT NULL CHECK (last_resort IN (0, 1)),
    uploaded_at INTEGER NOT NULL
) STRICT;

CREATE INDEX key_packages_by_device ON key_packages (device_id, last_resort, id);
CREATE UNIQUE INDEX key_packages_one_last_resort ON key_packages (device_id)
    WHERE last_resort = 1;

-- group-membership: Membership state on the Pi. The MVP Pi holds at most one
-- group (Group creation), so the single row has id 1.
CREATE TABLE mls_group (
    id              INTEGER PRIMARY KEY CHECK (id = 1),
    group_id        BLOB NOT NULL UNIQUE,
    epoch           INTEGER NOT NULL,
    -- Position of the Pi in the group context's external_senders list.
    pi_sender_index INTEGER NOT NULL,
    created_at      INTEGER NOT NULL
) STRICT;

-- The OpenMLS PublicGroup storage (ratchet tree, group context, confirmation
-- tag, interim transcript hash, pending proposals) as OpenMLS writes it.
CREATE TABLE mls_public_state (
    key   BLOB PRIMARY KEY,
    value BLOB NOT NULL
) STRICT;

-- The member devices and their leaf positions, rebuilt from the tree after
-- every accepted commit.
CREATE TABLE mls_members (
    leaf_index INTEGER PRIMARY KEY,
    device_id  BLOB NOT NULL UNIQUE REFERENCES devices (device_id)
) STRICT;
