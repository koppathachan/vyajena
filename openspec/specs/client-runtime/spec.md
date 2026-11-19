# client-runtime Specification

## Purpose
TBD - created by archiving change client-core. Update Purpose after archive.
## Requirements
### Requirement: Key storage
The core SHALL keep the user identity key, the device auth key and the MLS
signature key only in a key store: the OS keychain on desktop (macOS Keychain,
Linux Secret Service, Windows Credential Manager), Android Keystore or iOS
Keychain on phones. Only when the user explicitly opts in on a machine without
a keychain SHALL it use a key file with mode 0600. It SHALL never write these
keys to SQLite, logs or any other file. OpenMLS group state (epoch secrets, leaf
encryption keys, the secret tree) SHALL be stored in SQLite only encrypted with a
storage key that itself lives in the key store; messages stay unencrypted.

#### Scenario: Copied database
- **WHEN** someone copies Alice's SQLite file but not her key store
- **THEN** they can read her stored messages but cannot read her MLS group state,
  so they cannot decrypt new group traffic

#### Scenario: Laptop with a keychain
- **WHEN** Alice registers from her laptop's terminal client
- **THEN** her identity, auth and MLS signature keys are in the macOS Keychain and
  appear in no file or database row

#### Scenario: Headless machine without opt-in
- **WHEN** the core starts on a machine with no keychain and no file opt-in
- **THEN** it refuses to create keys and tells the user how to opt in

### Requirement: Local message store
The core SHALL store messages in a local SQLite database. It SHALL store the
plaintext of a message the user sends before sending it, because the sender
cannot decrypt its own ciphertext, and SHALL store each received message before
acknowledging it to the Pi.

#### Scenario: Own message survives a crash
- **WHEN** Alice sends "hi" and her laptop crashes before `Accepted` arrives
- **THEN** after restart "hi" is in her history marked as sending, and the core
  resends it with the same `msg_id`

#### Scenario: Crash before ack
- **WHEN** Bob's core crashes after storing a received message but before acking
- **THEN** the Pi redelivers it and Bob sees it once

### Requirement: Connection lifecycle
The core SHALL connect to the relay address from the invite, accept the Pi only
by its pinned TLS key, authenticate with its device auth key, renew its session
token before it expires, keep one `Connect` stream open with heartbeats, and
reconnect with backoff when the connection drops. When told the network is back
(`reconnect_now`), the core SHALL skip its current backoff wait. When the Pi
answers `Authenticate` or the stream with `PERMISSION_DENIED`, the core SHALL emit
`Removed` once and stop connecting.

#### Scenario: Wrong pin
- **WHEN** the Pi presents a TLS key that does not match the invite's pin
- **THEN** the core does not connect and reports a server identity error to the UI

#### Scenario: Network drop
- **WHEN** Bob's laptop loses the network for a minute
- **THEN** the core reconnects on its own and catches up in order

#### Scenario: Relay without a tunnel
- **WHEN** the Pi's tunnel is down, so the relay refuses Bob's TCP connection on
  443 or accepts it and closes it at once, and Alice sends a message meanwhile
- **THEN** Bob's core treats it as a network drop, keeps retrying with backoff,
  and once the tunnel is back connects with the same pin and receives Alice's
  message

#### Scenario: Reconnect now
- **WHEN** Bob's core is waiting 30 seconds before its next retry and the app
  calls `reconnect_now`
- **THEN** the core connects at once; a call while connected does nothing

#### Scenario: Revoked device
- **WHEN** the admin removes Bob and Bob's phone gets `PERMISSION_DENIED` from
  `Devices.Authenticate`
- **THEN** the core emits `Removed`, stops retrying, and Bob's history stays
  readable

### Requirement: Send status
The core SHALL report each sent message as sending until the Pi accepts it, then
as accepted. On `STALE_EPOCH` or a reconnect it SHALL process pending commits,
re-encrypt and resend with the same `msg_id`; on any other rejection it SHALL
report the message as failed with the reason.

#### Scenario: Message sent across a commit
- **WHEN** Alice's message is rejected with `STALE_EPOCH` because the admin just
  added Carol
- **THEN** the core applies the commit, resends the message in the new epoch, and
  Alice sees it go from sending to accepted

#### Scenario: Message typed while removed for inactivity
- **WHEN** Bob sends "back" while his laptop is removed for inactivity
- **THEN** it stays sending, and after the re-add the core sends it in the new
  epoch with the same `msg_id` and Bob sees it go to accepted

### Requirement: Events in order
The core SHALL deliver events to the UI in `server_seq` order through one event
stream: received messages, profile and group-setting changes (with who changed
the group settings), gap notices, removal and rejoin notices, membership changes
(devices added and removed, and the committer's user), the removed-device event,
and identity-key-changed warnings.

#### Scenario: Back from holiday
- **WHEN** Bob's laptop returns after 10 days and is re-added
- **THEN** the UI gets a removed-for-inactivity event with the time messages stop,
  then the messages from after the re-add, in order

#### Scenario: Tablet added
- **WHEN** Bob's phone commits the Add of Bob's tablet
- **THEN** every member's core emits `MembersChanged` with the tablet in `added`
  and `bob@pi` as the committer's user

### Requirement: Image encode pipeline
The core SHALL own every image transform: given a picked image's raw bytes it
SHALL decode it, apply its EXIF orientation and then drop all metadata (EXIF,
GPS included), downscale so the long edge is at most
`protocol::blob_store::MAX_IMAGE_EDGE` (2048) without upscaling, and encode WebP
at `protocol::blob_store::WEBP_QUALITY` (~80) within
`protocol::blob_store::MAX_IMAGE_BYTES` (2 MB), lowering quality and then size
before refusing. WebP SHALL be the only output. An animated input SHALL be
reduced to its first frame; an input the core cannot decode SHALL be refused with
a clear error. The core SHALL compute the ThumbHash and record the WebP
`width`/`height`. Dart SHALL never transform image pixels for the wire. The core
enforces this.

#### Scenario: Photo is transcoded
- **WHEN** Alice picks a 4000×3000 JPEG with GPS EXIF
- **THEN** the core sends a WebP at most 2048 px on the long edge, at quality
  ~80, under 2 MB, with no EXIF, shown upright

#### Scenario: Undecodable input
- **WHEN** Alice picks a file the core cannot decode as an image
- **THEN** the core refuses it with an error and sends nothing

### Requirement: Blob crypto and send order
The core SHALL encrypt each image's WebP with a fresh
`protocol::blob_store::BLOB_KEY_LEN` (32) byte key under XChaCha20-Poly1305, with
a fresh `protocol::blob_store::BLOB_ID_LEN` (16) byte `blob_id` as additional
authenticated data, reusing neither key nor id across sends (no dedup). It SHALL
derive the nonce from the key and the `blob_id`, so a retried upload is
byte-identical. When the user sends an image the core SHALL keep its WebP in the
device media cache and store the message, waiting for its blob, at once and
whether or not it is connected, like a text message. When the outbox reaches the
message with a connection the core SHALL upload the blob (idempotent on
`blob_id`) first and send the `APPLICATION` envelope only after the Pi accepted
the blob, keeping the chat's later messages behind it until then. It SHALL try
again an upload that failed in a way that can pass (the Pi's blob store
unavailable, an expired session, the network), SHALL fail the message with a
reason when the Pi refuses the blob for good or the picture is no longer in the
cache, and SHALL fail a message still waiting `protocol::blob_store::BLOB_TTL`
after it was written. A resend of the stored message SHALL reuse the same
`msg_id`. The core enforces this.

#### Scenario: Upload precedes the message
- **WHEN** Alice sends an image while connected
- **THEN** the core uploads the blob and, only after `Blobs.UploadBlob` is
  accepted, sends the `Content{Image}` envelope

#### Scenario: Sent offline
- **WHEN** Alice sends an image, then the text "and a line after it", while her
  device is not connected
- **THEN** both are stored as sending at once with no error; when she is back
  online the blob uploads, then the image goes out, then the text, in that order

#### Scenario: Crash while queued
- **WHEN** Alice's phone restarts with a queued image not yet sent
- **THEN** the image is still queued, its blob uploads and it goes out once

#### Scenario: Crash after the upload
- **WHEN** Alice's phone crashes after the blob upload but before `Accepted`
- **THEN** after restart the envelope resends under the same `msg_id` and the
  blob is not uploaded again

#### Scenario: The Pi's drive is away
- **WHEN** Alice sends an image while the Pi's blob store answers `UNAVAILABLE`
- **THEN** nothing is sent for it, it stays sending, and it goes out after the
  blob store is back

#### Scenario: The picture was cleared
- **WHEN** the media cache is cleared while an image waits to be sent
- **THEN** the image fails with "the picture is no longer on this device",
  nothing is uploaded, and the text queued behind it is sent

#### Scenario: Waited too long
- **WHEN** an image has waited seven days in the outbox
- **THEN** it fails with "the picture could not be sent in time" and nothing is
  uploaded

### Requirement: Image download and cache
On the user opening an image the core SHALL call `Blobs.DownloadBlob`, decrypt
with the `blob_key` (checking the `blob_id` AAD), decode the WebP, cache the
plaintext keyed by `blob_id`, and then `Blobs.AckBlob`. The media cache SHALL be
separate from the SQLite message store, protected by OS file-based encryption,
excluded from OS backup, and bounded by an LRU size cap; an evicted received
image SHALL be re-fetchable within `BLOB_TTL`. The per-blob key SHALL stay with
the message, never in the cache. The core enforces this.

#### Scenario: Open and cache
- **WHEN** Bob taps an image for the first time
- **THEN** the core downloads and decrypts the blob, caches the WebP, shows it,
  and acks the blob

#### Scenario: Re-open after eviction
- **WHEN** an image was evicted from the cache and Bob opens it again within 7
  days
- **THEN** the core re-downloads and re-decrypts it from the blob store

### Requirement: Expired media
When `Blobs.DownloadBlob` answers `NOT_FOUND`, the core SHALL mark that image
message expired, report it as expired from then on without asking the Pi again,
and SHALL keep showing its ThumbHash placeholder, which lives in the message,
not the blob. The core that sent an image, which keeps its plaintext in the media
cache, SHALL resend it on request as a new message with a fresh `blob_id`, key and
`msg_id`; it SHALL refuse to resend a message another device sent, or one whose
picture is no longer in its cache. The core enforces this.

#### Scenario: Receiver opens an expired image
- **WHEN** Bob opens an image whose blob the Pi has deleted
- **THEN** the core reports it expired and the blur stays visible

#### Scenario: Sender can resend
- **WHEN** Alice's image has expired on the Pi and she still has it cached
- **THEN** her core can resend it as a new image with a new blob
