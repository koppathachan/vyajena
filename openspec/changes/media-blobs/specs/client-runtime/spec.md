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
store the message's plaintext and WebP in the device media cache first, upload
the blob (idempotent on `blob_id`), and only after the upload is accepted send
the `APPLICATION` envelope carrying the `Image` body. On a resend it SHALL reuse
the same `msg_id` and `blob_id`. The core enforces this.

#### Scenario: Upload precedes the message
- **WHEN** Alice sends an image
- **THEN** the core uploads the blob and, only after `Blobs.UploadBlob` is
  accepted, sends the `Content{Image}` envelope

#### Scenario: Crash mid-send resumes
- **WHEN** Alice's phone crashes after storing the image but before `Accepted`
- **THEN** after restart the image is marked sending, the blob re-uploads under
  the same `blob_id`, and the envelope resends under the same `msg_id`

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
message expired and SHALL keep showing its ThumbHash placeholder, which lives in
the message, not the blob. The sender's core, which keeps its own plaintext,
SHALL offer to resend the image as a new message with a fresh `blob_id` and key.
The core enforces this.

#### Scenario: Receiver opens an expired image
- **WHEN** Bob opens an image whose blob the Pi has deleted
- **THEN** the core reports it expired and the blur stays visible

#### Scenario: Sender can resend
- **WHEN** Alice's image has expired on the Pi and she still has it cached
- **THEN** her core can resend it as a new image with a new blob
