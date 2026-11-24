## ADDED Requirements

### Requirement: Video messages
A `Video` body SHALL carry a 16-byte `blob_id`, a 256-bit `blob_key`, the
`blob_size`, the `duration_ms` (≤ `protocol::video::MAX_VIDEO_DURATION_MS`), the
pixel `width`/`height`, and a first-frame `thumbhash` (≤
`protocol::blob_store::MAX_THUMBHASH_BYTES`). It SHALL reference exactly one MP4
clip (H.264 video + AAC audio) stored as an encrypted blob (blob-store); there SHALL
be no multiple clips and no separate poster blob. The `blob_key` SHALL travel only
inside the encrypted `Content`. A video SHALL be sendable in the lobby, named groups
and DMs, like an image. A client built before `Video` SHALL show the unknown-body
placeholder (message-content: Inner content format). The client enforces this.

#### Scenario: Alice sends a video
- **WHEN** Alice sends a 12-second clip in the lobby
- **THEN** her core encrypts one `Content` whose `Video` body holds the blob id, the
  blob key, the size, `duration_ms` 12000, the dimensions and a first-frame
  ThumbHash, and no key appears outside the ciphertext

#### Scenario: Old client receives a video
- **WHEN** a client that predates `Video` decrypts a video message
- **THEN** it shows the "needs a newer app version" placeholder and does not fail

### Requirement: Recording and import
The app SHALL offer two sources for a video: in-app recording configured to H.264/MP4
and bounded to 30 seconds at capture (no transcode), and a gallery pick. A gallery
clip over the limit SHALL be trimmed with a 30-second slider, never rejected outright.
Both sources SHALL be normalized to canonical H.264/MP4 at the 480p/~1 Mbps target
within the advertised size limit before the core encrypts them. The app enforces
this.

#### Scenario: Record a clip
- **WHEN** Bob records a video in the app
- **THEN** the camera captures H.264/MP4 bounded to 30 seconds, and it is sent with
  no transcode

#### Scenario: Trim an over-limit gallery clip
- **WHEN** Alice picks a 2-minute clip from the gallery
- **THEN** the app shows a 30-second trim slider and sends the trimmed, normalized clip

### Requirement: Config-driven limit
The server SHALL read a maximum video blob size from its configuration at startup,
SHALL clamp it to `[the image cap, protocol::video::MAX_VIDEO_BYTES]`, SHALL fall
back to `protocol::video::DEFAULT_MAX_VIDEO_BYTES` when it is unset or out of range,
and SHALL enforce it as the `Blobs.UploadBlob` size cap. The server SHALL advertise
the value in `ServerInfo.Limits.max_video_blob_bytes`. Clients SHALL fetch it and
refuse a video whose encrypted blob would exceed it, and SHALL enforce the 30-second
duration client-side (the server never sees duration). Changing the limit SHALL be a
config edit and restart, with no rebuild. The server and the client enforce this.

#### Scenario: Operator raises the limit
- **WHEN** the owner edits `max_video_blob_bytes` in the server config and restarts
- **THEN** `GetServerInfo` advertises the new value and the server enforces it as the
  upload size cap, with no rebuild

#### Scenario: Config beyond the ceiling
- **WHEN** the config sets a value above `MAX_VIDEO_BYTES`
- **THEN** the server clamps it to `MAX_VIDEO_BYTES` and advertises the clamped value

### Requirement: Video stays encrypted
A video reference and its ThumbHash SHALL travel only inside the encrypted `Content`;
the blob the server stores SHALL be ciphertext with no key. The server SHALL send the
same content-free wake ping for a video message as for any other, with the fixed text
"New message". The client and the server enforce this.

#### Scenario: Video while Bob is offline
- **WHEN** Alice sends a video while Bob's phone has no open stream
- **THEN** the server queues one `APPLICATION` envelope and sends Bob's phone one wake
  ping with the text "New message", the same as for a plain message
