## Why

The chat carries text only. The owner put images first in the media block of
work: members want to send a photo. Images are too large for the 65,536-byte
application ciphertext cap, so they cannot ride inside an MLS message the way
text, mentions and replies do; they need a store. This change adds image
messages end to end while keeping the invariant that the Pi and the relay see
ciphertext only.

## What Changes

- **Image messages (proto, core, app):** `Content` gains an `Image` body. A
  sent image is transcoded to WebP in the Rust core, encrypted with a fresh
  per-image key, and uploaded as a blob to the Pi; the MLS message carries the
  blob id, that key, the pixel dimensions, a ThumbHash blur placeholder and an
  optional caption. The Pi and the relay never see the image or its key.
- **WebP is the only wire format (core):** the core decodes the picked image,
  strips EXIF, downscales to at most 2048 px on the long edge, and encodes WebP
  at quality ~80 within a 2 MB cap. No other format travels; the Pi stores
  opaque ciphertext.
- **Blob store on the Pi (proto, Pi, deploy):** a new `Blobs` service stores,
  serves and expires encrypted blobs. Blobs live on an external thumb drive
  (ext4, mounted by UUID); the service depends on the mount and degrades
  gracefully (blob RPCs return `UNAVAILABLE`, text messaging is unaffected) when
  it is absent. The database and keys stay on the Pi's main storage. Blobs are
  not backed up.
- **Blob crypto (core):** a fresh 256-bit key per blob, XChaCha20-Poly1305,
  random 16-byte blob id, no dedup. The key travels only inside the MLS
  ciphertext. Upload is idempotent on the blob id; a resent message reuses its
  `msg_id` (existing envelope dedup).
- **Expiry and resend (Pi, core, app):** a blob expires 7 days after upload and
  is deleted earlier once every member device has acked it. The sender keeps its
  own plaintext. A receiver that opens an image whose blob is gone sees a
  "media expired — resend" placeholder; the ThumbHash blur still shows.
- **No thumbnails; download on tap (core, app):** the bubble shows the ThumbHash
  blur at the image's aspect ratio; the full image downloads and decrypts on
  tap, then is cached on the device (OS file-based encryption, excluded from
  backup, LRU-capped).
- **Optional caption with mentions (proto, core, app):** an image may carry a
  caption that reuses the mentions mechanism (`@name`, `@everyone`). No reply
  reference in this change.
- **Old clients:** the `Image` body is an unknown oneof field to a pre-media
  client, which shows the existing "needs a newer app version" placeholder
  (message-content: Inner content format). Nothing new leaks to the Pi, relay or
  push; the wake-ping text stays "New message" (push invariant).
- **Out of scope (owner, media decisions):** stickers, cutout and transforms
  (the two later changes), albums, GIF/animated images, video, editing images.
- Details and decisions: `design.md`.

## Open decisions resolved

- Image wire format: WebP only, transcoded in the core, EXIF stripped, ≤2048 px
  long edge, q~80, ≤2 MB (owner, media decisions).
- Delivery model: encrypted blobs on the Pi referenced from the MLS message;
  no thumbnails, full image on tap (owner).
- Blob crypto: fresh 256-bit key per blob, XChaCha20-Poly1305, random blob id,
  no dedup, key in MLS; idempotent upload, send-dedup on `msg_id` (owner).
- Blob lifetime: 7 days + ack-delete; sender keeps plaintext; "media expired —
  resend" placeholder (owner).
- Scope: one static image per message, optional caption reusing mentions
  (owner).
- Pi storage: blobs on an external thumb drive (ext4, mount by UUID), service
  depends on the mount and degrades gracefully; DB + keys on main storage; not
  backed up (owner). Mount changes go through `deploy/pi/` only.

## Open decisions for the owner (see design.md "Open Questions")

- **Blob-store API shape:** a new `Blobs` gRPC service (recommended) versus
  extending `Messaging`; unary versus streaming upload/download; group-bound
  authorization with per-blob acks (recommended) versus an anonymous
  capability model keyed on the blob id. Design D4.
- **Thumb-drive at-rest encryption and mount mechanics:** plain ext4 (recommended;
  blobs are already client-side ciphertext) versus LUKS; mount path; fstab
  `nofail` by UUID versus a systemd `.mount` unit. Design D8.
- **Caption length:** reuse the text-message limit (recommended) versus a
  smaller cap. Design D3.

## Proto files touched

- `proto/vyajena/v1/content.proto`: new `Image` body in `Content.body`, new
  `Image` message (imports `common.proto` for `Mention`). Additive:
  `buf breaking` passes.
- `proto/vyajena/v1/blobs.proto` (new): the `Blobs` service and its request and
  response messages. New file: `buf breaking` passes.

## Capabilities

### New Capabilities

- `blob-store`: the Pi's encrypted blob service — upload, download, per-blob
  ack and deletion, 7-day expiry, who may upload and download, size limits,
  storage on the external mount and graceful degradation when it is absent.

### Modified Capabilities

- `message-content`: `Content` may carry an `Image` body; image rules (WebP
  only, EXIF stripped, one static image, dimensions, ThumbHash, blob reference,
  caption with mentions). Old clients show the unknown-body placeholder.
- `client-runtime`: the core transcodes to WebP, strips EXIF, downscales,
  computes the ThumbHash, does the blob crypto, uploads and downloads blobs,
  keeps a device media cache (OS file encryption, excluded from backup,
  LRU-capped) and surfaces "media expired — resend".
- `app-ui`: pick and send an image with an optional caption; the image bubble
  (ThumbHash blur → full image on tap), the download/expired states, and the
  caption with mentions.
- `pi-runtime`: a new `blob_dir` setting on the external mount; the server
  degrades gracefully when the mount is absent.
- `pi-deployment`: the thumb-drive mount (ext4 by UUID, `nofail`), the service
  depending on the mount, a writable path for `blob_dir`, `check.sh` verifying
  it, and blobs excluded from backup.

## Impact

- `proto/`, regenerated `pb.rs` on both sides (`cargo run -p pbgen`).
- `protocol/src/blob_store.rs` (new): `MAX_BLOB_BYTES`, `MAX_IMAGE_BYTES`,
  `MAX_IMAGE_EDGE`, `WEBP_QUALITY`, `BLOB_ID_LEN`, `BLOB_KEY_LEN`,
  `MAX_THUMBHASH_BYTES`, `BLOB_TTL`.
- `server/`: a new `blobs` feature (the `Blobs` service), blob storage on the
  mount, expiry and ack-delete, a schema migration for blob metadata, config
  `blob_dir`, graceful degradation.
- `client/`: a new `media` feature (WebP transcode, EXIF strip, downscale,
  ThumbHash, blob crypto, upload/download, device media cache), `send_image`,
  `download_image`, image fields on `Message`, a store migration.
- `bridge/`: image send/download calls and types; regenerated frb bindings
  (`flutter_rust_bridge_codegen generate`).
- `app/`: image picker, the image bubble and viewer, caption composer with
  mentions, the fake core.
- `deploy/pi/`: the thumb-drive mount setup, service dependency on the mount,
  `ReadWritePaths` for `blob_dir`, `check.sh`, `backup.sh` (blobs excluded).
