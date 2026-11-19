## ADDED Requirements

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
