-- client-core task groups 4 and 5: this device's account, its view of the
-- group, KeyPackage bookkeeping and the own commit in flight. No keys and no
-- plaintext: keys live in the key store, group secrets in `mls_storage`.
-- Times are milliseconds since the Unix epoch.

-- device-identity: the account this device registered or was linked into
-- (one row). The relay address and TLS pin come from the invite (or the link
-- approval); the certificate is this device's, signed by the user identity key.
CREATE TABLE account (
    id                    INTEGER PRIMARY KEY CHECK (id = 1),
    relay_address         TEXT NOT NULL,
    tls_pin               BLOB NOT NULL CHECK (length(tls_pin) = 32),
    username              TEXT NOT NULL,
    server_id             TEXT NOT NULL,
    device_id             BLOB NOT NULL CHECK (length(device_id) = 16),
    -- SignedDeviceCertificate.certificate and .signature, as carried.
    certificate           BLOB NOT NULL,
    certificate_signature BLOB NOT NULL,
    -- 1 when Devices.GetSelf said this user is the admin.
    is_admin              INTEGER NOT NULL CHECK (is_admin IN (0, 1)),
    -- ServerInfo.external_sender: the Pi's MLS external sender entry.
    pi_sender_key         BLOB NOT NULL,
    pi_sender_credential  BLOB NOT NULL,
    created_at            INTEGER NOT NULL
) STRICT;

-- group-membership: this device's bookkeeping for the one group (one row once
-- it created or joined it). Group secrets are in `mls_storage`.
CREATE TABLE group_state (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    group_id            BLOB NOT NULL,
    -- Own leaf rotation: when this device's leaf was last replaced (group
    -- creation, joining, or an own commit with an update path).
    own_leaf_updated_at INTEGER NOT NULL,
    -- message-delivery: Acknowledgement. The highest server_seq this device has
    -- durably processed; a redelivery at or below it is acked and skipped.
    processed_seq       INTEGER NOT NULL DEFAULT 0,
    -- Set when this device refused a commit the Pi sequenced (a failed
    -- certificate, leaf-key or commit-authority check): it cannot follow the
    -- group until it is re-added. Why, for the UI.
    stalled             TEXT
) STRICT;

-- message-delivery: Idempotent sends. The own COMMIT in flight, exactly as
-- sent, so a resend after a reconnect or crash repeats identical bytes. At most
-- one: OpenMLS keeps one pending commit. `purpose` says what to finish after it
-- is accepted (e.g. Admin.RemoveUser).
CREATE TABLE outgoing_commit (
    id       INTEGER PRIMARY KEY CHECK (id = 1),
    msg_id   BLOB NOT NULL CHECK (length(msg_id) = 16),
    envelope BLOB NOT NULL,
    purpose  TEXT NOT NULL
) STRICT;

-- key-directory: KeyPackage upload; Last-resort KeyPackage refresh. Every
-- last-resort KeyPackage uploaded, newest last. Its private init key stays in
-- OpenMLS storage until it is two refreshes old (a fetched copy may still be
-- on its way to a re-adder).
CREATE TABLE last_resort_key_packages (
    hash_ref    BLOB PRIMARY KEY,
    uploaded_at INTEGER NOT NULL
) STRICT;

-- device-identity: Device linking. Devices this device certified with
-- Devices.LinkDevice that are not in the group yet; it adds each one as soon
-- as the new device has uploaded KeyPackages.
CREATE TABLE pending_links (
    device_id  BLOB PRIMARY KEY CHECK (length(device_id) = 16),
    created_at INTEGER NOT NULL
) STRICT;
