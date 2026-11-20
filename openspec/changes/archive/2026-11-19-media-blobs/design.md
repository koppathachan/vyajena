## Context

Images do not fit the text path. An application ciphertext is capped at 65,536
bytes (wire-transport: Size limits); a 2 MB photo is far larger and cannot ride
inside an MLS message. So an image is split in two: the bytes go to a store on
the server as an encrypted blob, and the MLS message carries only a small reference
(blob id, the blob's key, dimensions, a ThumbHash blur, an optional caption).
The invariants still hold: keys never leave the core, and the server and the relay
see ciphertext only — the blob is encrypted on the device before upload, with a
key that travels only inside the MLS ciphertext.

This is the first of three sequential media changes (media-blobs →
image-transform → sticker-cutout-plugin). It owns `content.proto`'s image body
and the WebP pipeline; the later two build on both. This change is images only:
no stickers, cutout or transforms.

## Goals / Non-Goals

**Goals:** send and receive one static image per message, end to end encrypted,
with a blurred placeholder before download and the full image on tap; a blob
store on the server that never sees plaintext and expires blobs; graceful behaviour
for old clients, a missing mount and an expired blob.

**Non-Goals:** stickers, cutout, transforms (later changes); albums or multiple
images per message; GIF, animated or video; editing or re-encoding a received
image; thumbnails or progressive/preview downloads; cross-device sync of the
media cache; reply-to on an image (only Text carries `reply_to`).

## Decisions

Proto first (config.yaml: proto-first). The two proto edits below define the
contract; every other decision follows from them.

### D1. `content.proto`: a new `Image` body in `Content`

`Content.body` gains `Image image = 5`. The image reference is small and lives
inside the MLS ciphertext like every other body.

```proto
message Content {
  google.protobuf.Timestamp sent_at = 1;
  oneof body {
    Text text = 2;
    GroupSettings group_settings = 3;
    Profile profile = 4;
    Image image = 5;              // new
  }
}

// message-content: Image messages. One static image, sent as an encrypted blob
// on the server. This body is what a receiver needs to show a blurred placeholder
// at once and to fetch and decrypt the full image on tap. Only the Rust core
// reads it; the server never sees it.
message Image {
  // The 16-byte random id the ciphertext is stored under on the server
  // (blob-store). Unique per upload; no dedup (protocol::blob_store::BLOB_ID_LEN).
  bytes blob_id = 1;
  // The 256-bit XChaCha20-Poly1305 key for this blob, fresh per blob. It
  // travels only here, inside the MLS ciphertext
  // (protocol::blob_store::BLOB_KEY_LEN).
  bytes blob_key = 2;
  // Decoded pixel size of the WebP, so the bubble lays out before the image
  // downloads. Both are >= 1 and <= protocol::blob_store::MAX_IMAGE_EDGE.
  uint32 width = 3;
  uint32 height = 4;
  // A ThumbHash of the image (compact blurred placeholder), shown until the
  // full image is downloaded and decrypted. At most
  // protocol::blob_store::MAX_THUMBHASH_BYTES bytes.
  bytes thumbhash = 5;
  // The blob ciphertext size, so the UI can show a size before download and the
  // receiver can check the download. In bytes; <= MAX_BLOB_BYTES.
  uint64 blob_size = 6;
  // Optional caption. UTF-8, limited like Text (D3).
  optional string caption = 7;
  // Mentions in `caption`, same rules as `Text.mentions` (message-content:
  // Mentions). Empty when there is no caption.
  repeated Mention caption_mentions = 8;
}
```

Alternative: make the caption a whole `Text` (text + mentions + `reply_to`).
Rejected for MVP: it would give an image a reply reference, which is a non-goal,
and it muddies "Text only". A separate `caption` plus `caption_mentions` matches
the owner's "reuses the mentions mechanism" literally. Open to the owner (D3).

### D2. `blobs.proto`: a new `Blobs` service

A blob is too large for the `Connect` stream (its envelopes cap at 64 KB /
256 KB), so it needs its own RPCs. It is a small unary service; the shape and
authorization are settled in D4 and the storage behind it in D8.

```proto
// Blobs service: encrypted image blobs, stored on the server, never seen in the
// clear. All RPCs require a session token (wire-transport: Session
// authentication). A blob is opaque ciphertext; the server learns its size, its
// group and its timing, never its content or key.
service Blobs {
  // blob-store: Upload. Idempotent on `blob_id`.
  rpc UploadBlob(UploadBlobRequest) returns (UploadBlobResponse);
  // blob-store: Download. NOT_FOUND once the blob has expired or been deleted.
  rpc DownloadBlob(DownloadBlobRequest) returns (DownloadBlobResponse);
  // blob-store: Ack and delete. A member device reports it no longer needs the
  // blob; the server deletes it once every member device has acked (blob-store:
  // Expiry and ack-delete).
  rpc AckBlob(AckBlobRequest) returns (AckBlobResponse);
}

message UploadBlobRequest {
  // The group the image is for; the server authorizes download against membership
  // and scopes ack-delete to it (D4). The server already sees the matching
  // APPLICATION envelope's group, so this leaks nothing new.
  bytes group_id = 1;
  bytes blob_id = 2;         // 16 bytes, client-generated, unique
  bytes ciphertext = 3;      // XChaCha20-Poly1305, <= MAX_BLOB_BYTES
}
message UploadBlobResponse {
  google.protobuf.Timestamp expires_at = 1;  // upload time + BLOB_TTL
}

message DownloadBlobRequest {
  bytes group_id = 1;
  bytes blob_id = 2;
}
message DownloadBlobResponse {
  bytes ciphertext = 1;
}

message AckBlobRequest {
  bytes group_id = 1;
  bytes blob_id = 2;
}
message AckBlobResponse {}
```

`buf breaking` passes: `content.proto` adds a oneof field and a message,
`blobs.proto` is new. `cargo run -p pbgen` regenerates both `pb.rs` files.

### D3. The caption reuses mentions and the text-message length limit (decided, owner 2026-11-19)

`Image.caption` is optional UTF-8 with `caption_mentions`, the same `Mention`
message and span rules as `Text` (message-content: Mentions). The length limit
is the text-message limit (owner, 2026-11-19): a caption plus a tiny image
reference encrypts well within the 65,536-byte application cap, so reusing
`protocol::message_content` (the text length rule) keeps one limit to reason
about. The sending core refuses an over-limit caption and refuses invalid
mention spans (`InvalidCaption`); a receiver drops invalid spans and still shows
the caption.

### D4. Blob-store API shape and authorization (decided, owner 2026-11-19)

The owner chose the recommended shape, encoded above:

- **A new `Blobs` service** (not an extension of `Messaging` or `Keys`): blobs
  are large, on a different storage device, and optional (the feature degrades
  when the mount is gone). A feature folder `server/src/blobs/` keeps it
  splittable, per config.yaml.
- **Unary upload and download** with a `MAX_BLOB_BYTES` (≈2 MB + crypto
  overhead) cap, which is within gRPC's default 4 MB message limit, so no
  transport tuning. Idempotent on `blob_id`: a re-upload with identical bytes
  returns the same `expires_at`; a re-upload of a different ciphertext under a
  live `blob_id` is rejected (`ALREADY_EXISTS`).
- **Group-bound authorization.** Upload and download name the `group_id`; the server
  allows them only from a current member device of that group (`PERMISSION_DENIED`
  otherwise). This leaks nothing the server did not already see (it routes that
  group's envelopes) and it gives ack-delete a membership set to scope to.

Alternatives for the owner:

| Axis | Recommended | Alternative |
|---|---|---|
| Service | new `Blobs` | extend `Messaging` with blob RPCs |
| Transport | unary, 2 MB cap | client/server streaming (chunked, resumable) |
| Authz | group-bound + member check | anonymous: the 128-bit `blob_id` is the capability, any authenticated device may fetch |
| Delete | ack-delete + 7-day backstop | 7-day expiry only, no acks |

Streaming buys resumability for a flaky home tunnel; the anonymous model is
simpler but loses ack-delete (blobs then always live the full 7 days) and lets
any member of any group fetch a blob whose id leaks. **Decided (owner
2026-11-19): the new `Blobs` service, unary with a 2 MB cap, group-bound with
per-blob ack-delete.** The storage behind the service is a pluggable backend
(D8), so this wire shape stays the same whether the bytes land on a drive or in
a bucket.

### D5. The WebP pipeline runs in the core

The app hands the core the picked image's raw bytes (and nothing about format);
the core owns every transform so the rule is one place and Dart never touches
pixels for the wire. On send the core:

1. Decodes the input (JPEG, PNG, HEIC where the platform decoder is available,
   static WebP). An animated input (animated WebP/GIF) is reduced to its first
   frame — static only (owner). A format it cannot decode is refused
   (`UnsupportedImage`).
2. Drops all metadata (EXIF, including orientation and GPS) by re-encoding from
   decoded pixels; it first applies the EXIF orientation so the image is shown
   upright.
3. Downscales so the long edge is at most `MAX_IMAGE_EDGE` (2048); never
   upscales.
4. Encodes WebP at `WEBP_QUALITY` (~80). If the result exceeds `MAX_IMAGE_BYTES`
   (2 MB) it lowers quality in steps and, if still over at a floor quality,
   downscales further, then refuses (`ImageTooLarge`) only if a sane floor still
   overflows. WebP is the only output; no other format reaches the wire.
5. Computes the ThumbHash from the decoded (scaled) pixels and records
   `width`/`height` of the encoded WebP.

HEIC decoding depends on the platform image library; where unavailable the core
refuses that input with a clear error and the app tells the user. This is noted
as a per-platform risk, not an owner decision.

**Spike result (task 2.1, 2026-11-19): PASS.** Crates: `image` 0.25 with
`default-features = false, features = ["jpeg", "png", "gif", "webp"]` (decode,
EXIF orientation via `ImageDecoder::orientation()` + `apply_orientation`,
Lanczos resize), `webp` 0.3 (libwebp, the only lossy WebP encoder; `image`'s own
encoder is lossless), `thumbhash` 0.1 (pure Rust). Run on the host:
- a JPEG with EXIF orientation 6 decodes as 300x400 after `apply_orientation`
  from a 400x300 source, and the WebP is built from pixels, so no EXIF or GPS
  can reach it (no `EXIF` chunk in the output);
- a 4000x3000 image scaled to 2048x1536 and encoded at q80 is 181 KB in about
  180 ms. The test image is a synthetic gradient, so the size is not
  representative of a photo; the lower-quality-then-smaller loop stays;
- ThumbHash of a 100x100 downscale is 21 bytes and decodes (so
  `MAX_THUMBHASH_BYTES` = 64 has room); the input must be at most 100x100;
- an animated GIF decodes to its first frame through `load_from_memory`. For an
  animated WebP, `image-webp` 0.2.4's `read_image` returns the first frame
  (read in its source, not run: no animated WebP was at hand), so the
  "first frame only" rule holds for both.
- HEIC is not decoded by `image`. The core does not decode it; it refuses it
  with `UnsupportedImage`, and the app's picker is expected to hand over JPEG
  or PNG. Task 6.1 checks what the picker gives on a real iOS and Android
  device and changes this if it does not.

Cross-compile: the release spike builds for `aarch64-apple-ios-sim` and for
`aarch64-linux-android` (NDK 28.2 clang from `toolchains/llvm`, API 24, linked
by the NDK clang; no `cargo-ndk` needed). libwebp is C, so the build needs a C
compiler for the target, as the native key stores already do.

### D6. Blob crypto: fresh key, XChaCha20-Poly1305, id as AAD

Per image the core generates a random `BLOB_ID_LEN` (16) byte id and a random
`BLOB_KEY_LEN` (32) byte key, then encrypts the WebP with XChaCha20-Poly1305: a
24-byte nonce prepended to the ciphertext (derived from the key and the blob id,
D7, so a retry is byte-identical), the tag appended, and the
`blob_id` as additional authenticated data so a blob cannot be served under the
wrong id. `MAX_BLOB_BYTES` = `MAX_IMAGE_BYTES` + nonce + tag (24 + 16), rounded.
The key is put in `Image.blob_key` and never leaves the core except inside the
MLS ciphertext (invariant: keys never leave the core). No dedup: every send is a
new id and key, even of the same picture.

**Spike result (task 2.1): PASS.** `chacha20poly1305` 0.10, which the core
already depends on, encrypts with `XChaCha20Poly1305` and `Payload { msg, aad }`:
the ciphertext is the plaintext plus the 16-byte tag, a flipped bit of the AAD
(`blob_id`) fails decryption, and it builds for iOS and Android with the other
spike crates (see D5).

### D7. Send and download order; idempotency

Send (owner, 2026-11-19: images queue offline like text): `Client::send_image`
encodes the image, puts its WebP in the device media cache (the sender cannot
decrypt its own ciphertext and must keep the plaintext, client-runtime: Local
message store), and stores the own message as `sending` with its `Image` content
(blob id, blob key, size, ThumbHash, caption) **and a `blob_pending` mark**, in
one transaction. That is all it does: it needs no connection and returns at
once, as `send_text` does.

The outbox then does the rest, in order, when the message's chat can send:

1. The message is `blob_pending`: start uploading its blob and hold the chat's
   later messages behind it. The upload runs in a **task of its own** (a 2 MB
   upload inside the engine loop would stall heartbeats and the stream on a slow
   link); the task reads the WebP from the cache, encrypts it under the key from
   the stored content, calls `UploadBlob`, and reports back to the engine.
2. On `Done` the engine clears `blob_pending` and runs the outbox again, which
   now sends the `APPLICATION` envelope carrying `Content{Image{…}}` exactly as
   for text.

So "the envelope implies the blob exists" holds per message at the moment it is
really sent, however long it waited, and the blob's 7 days start at that upload,
not when the user tapped Send. A crash anywhere resumes: the message is
`sending` and `blob_pending`, so the upload is tried again (byte-identical, so
the server answers with the original expiry), or it is `sending` without the mark,
so the envelope resends byte for byte under the same `msg_id` (message-delivery:
Idempotent sends).

Upload failures (the sub-decisions below are the owner's):

| The upload ends with | The core |
|---|---|
| accepted | clears the mark, sends the envelope |
| `UNAVAILABLE` (the server's drive is away, or it stores no pictures), `UNAUTHENTICATED` (the session ran out), a timeout or a network error | keeps the message `sending`, holds the chat, tries again after 3 s and at once on every reconnect |
| `PERMISSION_DENIED`, `INVALID_ARGUMENT`, `ALREADY_EXISTS`, `NOT_FOUND` | fails the message with the server's reason; later messages go on |
| the picture is no longer in the media cache | fails it: "the picture is no longer on this device" |
| the message waited 7 days (`BLOB_TTL`) | fails it: "the picture could not be sent in time" |

A failed image is an ordinary failed own message: the sender can still use
"Resend picture" if the cache has the picture.

**History.** The first draft had the outbox upload the blob. The first
implementation (task 4.2) instead uploaded in `send_image` and only then stored
the message, which kept the outbox unchanged but meant an image could be sent
only while connected. The owner reversed that the same day, so this is the draft
again, built with the upload in a separate task.

The blob upload is idempotent on `blob_id` and byte-identical on a retry because
the XChaCha20 nonce is derived from the key and the blob id rather than drawn at
random: `nonce = HMAC-SHA256(blob_key, "vyajena2-blob-nonce-v1" || blob_id)[..24]`.
That is safe because a key encrypts exactly one plaintext, ever (fresh key per
blob, D6), so no (key, nonce) pair is reused for different data. A retried call
inside the core therefore sends the same bytes and the server answers with the
original `expires_at` instead of `ALREADY_EXISTS`.

Download (on tap): the core calls `DownloadBlob(group_id, blob_id)`, decrypts
with `blob_key` (checking the `blob_id` AAD), decodes the WebP, caches the
plaintext under `blob_id`, shows it, then calls `AckBlob(group_id, blob_id)`.
`NOT_FOUND` means the blob expired or was deleted → the "media expired — resend"
state (D9). The sender shows its own image straight from the cache and never
downloads.

```mermaid
sequenceDiagram
  participant UI as Flutter (Alice)
  participant C as Rust core (Alice)
  participant P as server (Blobs + Messaging)
  participant B as Rust core (Bob)
  UI->>C: send_image(bytes, caption)
  C->>C: decode, strip EXIF, scale ≤2048, WebP q80, ThumbHash
  C->>C: blob_id, key; keep the WebP in the cache; store the message, blob_pending
  Note over C,P: later, when the outbox reaches it with a connection (maybe after an offline wait or a restart)
  C->>C: XChaCha20 encrypt from the cache
  C->>P: Blobs.UploadBlob(group, blob_id, ciphertext)
  P-->>C: UploadBlobResponse(expires_at)
  C->>C: clear blob_pending
  C->>P: Messaging Envelope APPLICATION = Content{Image{blob_id,key,w,h,thumbhash,caption}}
  P-->>B: Envelope (ciphertext only); wake ping "New message" if offline
  B->>B: decrypt → Image; show ThumbHash blur at w×h
  UI->>B: tap image
  B->>P: Blobs.DownloadBlob(group, blob_id)
  P-->>B: ciphertext
  B->>B: decrypt (AAD=blob_id), decode WebP, cache, show
  B->>P: Blobs.AckBlob(group, blob_id)
```

### D8. Pluggable storage backend; the default is the local filesystem (decided, owner 2026-11-19)

The blob store is not tied to the server (owner, 2026-11-19). The server writes blob
bytes through a `BlobStore` backend trait — `put`, `get`, `delete` and `exists`
by `blob_id`, returning opaque ciphertext — so a future backend (an
object-storage bucket, other cheap media) drops in without touching the wire
protocol or the specs. The **MVP ships one backend, the local filesystem**; the
trait is the seam for the rest.

What stays in the server, independent of the backend: the blob *metadata* (id, group,
size, `uploaded_at`, `expires_at`, ack set) lives in the server's SQLite, and so does
all authorization, expiry and ack-delete logic (D4, D9). The backend only stores
and returns opaque bytes by key; it never decides who may read a blob or when it
dies. So group-bound authorization and ack-delete work the same whether the bytes
sit on a drive or in a bucket.

The filesystem backend writes under one **stable, configured path**, the same on
every start (owner: the drive is auto-mounted at a fixed path). Config is a
`[blob_store]` table — `backend = "filesystem"` and, for it, `path = "…"`; when
the table is absent the feature is off. The database and keys stay in
`/var/lib/vyajena` on the server's main storage (invariant; server-runtime: Durable
storage unchanged). Blobs are not backed up (`backup.sh` keeps covering DB + keys
only); losing them loses at most 7 days of re-sendable images (owner).

**No extra at-rest encryption layer (owner, 2026-11-19):** blobs are already
client-side ciphertext and the decryption key never reaches the server, so a LUKS
or encrypted-volume layer would cost real complexity (a headless server needing a
passphrase at boot) for almost no gain. Each backend relies on whatever at-rest
protection it natively has — none extra for the filesystem; a bucket brings its
own.

**Degrade gracefully when the backend is unavailable** (the drive not mounted, a
bucket unreachable): the server still starts and serves all text messaging, and
`Blobs` RPCs return `UNAVAILABLE` until the backend is back. The filesystem
backend never creates its directory and writes and removes a probe file in it on
every RPC. A missing directory fails the probe; so does an unmounted mountpoint,
which belongs to root and is not writable by the service user (the deploy makes
the path `vyajena`-owned only on the drive itself). So a blob is never written to
the SD card by mistake. This does not tell a drive from an ordinary directory
the service user owns: that is the deploy script's job (task 7.1), and a
test host may point it anywhere.

Deployment of the default backend goes through `deploy/server/` only (CLAUDE.md): the
drive auto-mounts at the stable path (the owner's setup, not scripted by us);
`deploy.sh` ensures the directory exists, owned by `vyajena` mode 0700; the
service unit gets a writable path for it (today only `StateDirectory` is writable
under `ProtectSystem=strict`); `check.sh` verifies the path, ownership and mode
and warns, without failing, when the store is absent.

### D9. Expiry, ack-delete and "media expired — resend"

The server deletes a blob when either 7 days (`BLOB_TTL`) pass since upload, or every
device that is a current member of the blob's group has `AckBlob`'d it (the
sender's own device acks implicitly at upload, since it keeps the plaintext).
Membership changes between upload and the last ack are covered by the 7-day
backstop; the MVP does not re-derive the ack set when membership changes. A
`DownloadBlob` for a gone blob is `NOT_FOUND`; the receiving core raises a
`MediaExpired` state for that message. The app then shows the ThumbHash blur
with a "media expired — resend" label; the blur is always available because it
lives in the message, not the blob. The sender, which kept its plaintext, offers
"resend" (a fresh blob id, key and message).

### D10. Device media cache

Decrypted WebP bytes are cached on the device, keyed by `blob_id`, in a
dedicated directory that is not the encrypted SQLite message store (images are
too large to inline) and is excluded from OS backup (iCloud / Android
auto-backup) and protected by OS file-based encryption (iOS Data Protection
`NSFileProtectionComplete`; Android FBE). The cache is LRU-capped by total size.
A received image is re-downloadable from the server within 7 days if evicted. The
sender's own sent images are kept in the cache until `BLOB_TTL` so "resend"
works, then are eligible for eviction like any entry. The cache stores plaintext
WebP only; the per-blob key stays with the message in the store, so an evicted
image is re-fetched and re-decrypted, never kept decrypted beyond the cache.

**As built (tasks 4.3, 6.1).** The cache is a folder of `<hex blob id>.webp`
files, mode 0600 in a 0700 folder, beside the database, with its index
(`media_cache`: size, `own`, created, last shown) in the core's SQLite
(migration 0007). The cap defaults to 256 MiB; eviction drops the least recently
shown entry first, never the entry just cached, and skips an `own` entry until
7 days after it was cached. A cached file that went missing is forgotten, not an
error. A blob the server answered `NOT_FOUND` for is recorded (`media_expired`), so a
later tap says "expired" without a network call.

Where the folder is: `ClientConfig::media_dir`, which defaults to `media` inside
`data_dir`. The app's `data_dir` is already the platform's protected folder
(app-ui: Data on the phone: iOS Application Support with
`NSFileProtectionComplete` and excluded from iCloud backup; Android
`noBackupFilesDir`, which `allowBackup="false"` and file-based encryption cover),
so the cache inherits both properties and **no native code or channel is added**.
The app passes no media folder; `Core.open(dataDir)` is unchanged.

**The blur is decoded by the core.** `thumbhash` already ships in the core for
encoding, so it also decodes: `thumbhash_preview` returns the blur as a small
PNG that the app draws with `Image.memory`. The app has no ThumbHash code and no
`dart:ui` pixel API, and `app-ui: UI goes through the core` keeps its import
whitelist.

**Picking.** The picker (`image_picker`, the one new Dart dependency, which
reads a file the user chose and opens no connection) is asked for a bounded size
and a quality, which makes both platforms return a JPEG even for an iPhone HEIC
photo. The core does not decode HEIC (D5). This was not run on a real device in
this change; task 6.1's check on one is left to the owner's test run.

### D11. Old clients, versioning and push

The `Image` body is field 5 of the `Content.body` oneof; a client built before
this change decodes it as an unset body and shows the existing "needs a newer
app version" placeholder (message-content: Inner content format) — no crash, no
wire-version bump. A sender does not suppress an image when some members are old;
those members see the placeholder. The `Blobs` service is additive; a
pre-media device never calls it. Nothing new leaves the device for the server, the
relay or push: the image rides as one `APPLICATION` envelope like any message,
so its size class and the wake ping are unchanged — one fixed "New message"
(push invariant). The blob upload is a separate authenticated RPC the server sees as
an opaque ciphertext of known size; it carries no content and no push.

## Risks / Trade-offs

- [A receiver opens an image but the blob upload never completed] → the core
  uploads the blob and waits for its acceptance before sending the envelope
  (D7), so the envelope implies the blob exists; a transient `NOT_FOUND` within
  7 days is shown as "not downloaded, retry", distinct from "expired".
- [The server learns which groups send images, how large and when] → accepted: it
  already routes and sizes that group's envelopes; the blob content and key stay
  encrypted. The anonymous model (D4) would hide the group at the cost of
  ack-delete.
- [A crash between the backend write and the metadata insert leaves a file with
  no row] → an upload writes the bytes first and records the row second, so a
  receiver never finds a row without bytes. The file is invisible to every RPC
  and the expiry sweep, which works from rows, never deletes it. At most one
  blob per crash and a retry of the same `blob_id` replaces it; an orphan scan
  is left for a later change. Accepted for the MVP.
- [Two uploads race for one blob id with different bytes] → the second write can
  replace the first's file while the row keeps the first's hash. Blob ids are
  random 128-bit values, so only a member trying to attack its own group could
  do it; a download checks the hash and answers `DATA_LOSS` rather than serving
  the wrong bytes.
- [The storage backend fills or is gone] → the feature degrades to `UNAVAILABLE`
  (D8) and text keeps working; `check.sh` flags a missing or full store.
- [HEIC/odd inputs fail to decode on some platforms] → the core refuses with a
  clear error (D5); the app tells the user rather than sending a broken image.
- [EXIF orientation lost on strip] → the core bakes orientation into the pixels
  before dropping EXIF (D5), so the image shows upright.
- [Sender plaintext grows the cache] → bounded by `BLOB_TTL` plus the LRU cap
  (D10); after expiry even the sender may lose the plaintext and can no longer
  resend. Accepted for MVP.

## Migration Plan

- Core: a store migration adds image columns / a blob-reference table to the
  message store (local id → blob_id, key, dims, thumbhash, caption, state). New
  column defaults keep existing text messages valid. No change to how text is
  stored.
- server: a schema migration adds blob metadata (blob_id, group_id, size,
  `uploaded_at`, `expires_at`, ack set). The blob bytes live in the storage
  backend (files under the filesystem path), not in SQLite.
- Deploy: `deploy/server/` wires the filesystem backend at the stable path; the
  first deploy that turns the feature on needs the drive auto-mounted there.
  Until then the feature is off and text is unaffected (D8). No data migration;
  no re-join.

## Resolved (owner, 2026-11-19)

1. **Caption length (D3):** reuse the text-message limit.
2. **Blob-store API shape and authorization (D4):** a new `Blobs` service, unary
   with a 2 MB cap, group-bound with per-blob ack-delete.
3. **Storage backend (D8):** a pluggable `BlobStore` trait; the MVP ships the
   local filesystem backend only, at one stable configured path; no extra
   at-rest encryption layer (blobs are already ciphertext); the feature degrades
   gracefully when the backend is unavailable. The drive is auto-mounted at a
   fixed path (owner's setup).

4. **Images queue offline like text (D7):** an image is stored and queued at
   once, connected or not; its blob uploads at send time, before its envelope.

## Open Questions

Sub-decisions that fell out of queuing images offline (D7). Each has a default
in the code; the owner may change any of them.

1. **Retry policy for a queued image that keeps failing to upload.** Now: a
   failure that may pass (the server's drive away, a lost session, no network) is
   retried every 3 seconds and at once on each reconnect, forever, until the
   7-day limit below. Alternative: exponential backoff, or a visible "retry"
   button after some number of failures.
2. **Age limit.** Now: a message still waiting 7 days after it was written
   (`BLOB_TTL`) fails with "could not be sent in time", since a picture that old
   is stale and a blob made for it would be near the end of its own life.
   Alternative: no limit, or a shorter one.
3. **`UNAVAILABLE` from a server that stores no pictures at all** (no `[blob_store]`,
   the default on the real server until the owner sets `BLOB_DIR`) is treated as a
   failure that may pass, so such an image waits and retries for 7 days instead
   of failing at once. The core cannot tell "the drive is away" from "this server has
   no picture store" by the status alone. Alternative: a distinct status or a
   `GetServerInfo` capability, so the core can fail at once and the app can say
   "this server does not take pictures".
4. **A failed image has no "retry" of its own**; the sender uses "Resend picture"
   on it (the same new-message path). Alternative: a retry that reuses the
   stored message.
