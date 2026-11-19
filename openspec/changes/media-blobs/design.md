## Context

Images do not fit the text path. An application ciphertext is capped at 65,536
bytes (wire-transport: Size limits); a 2 MB photo is far larger and cannot ride
inside an MLS message. So an image is split in two: the bytes go to a store on
the Pi as an encrypted blob, and the MLS message carries only a small reference
(blob id, the blob's key, dimensions, a ThumbHash blur, an optional caption).
The invariants still hold: keys never leave the core, and the Pi and the relay
see ciphertext only — the blob is encrypted on the device before upload, with a
key that travels only inside the MLS ciphertext.

This is the first of three sequential media changes (media-blobs →
image-transform → sticker-cutout-plugin). It owns `content.proto`'s image body
and the WebP pipeline; the later two build on both. This change is images only:
no stickers, cutout or transforms.

## Goals / Non-Goals

**Goals:** send and receive one static image per message, end to end encrypted,
with a blurred placeholder before download and the full image on tap; a blob
store on the Pi that never sees plaintext and expires blobs; graceful behaviour
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
// on the Pi. This body is what a receiver needs to show a blurred placeholder
// at once and to fetch and decrypt the full image on tap. Only the Rust core
// reads it; the Pi never sees it.
message Image {
  // The 16-byte random id the ciphertext is stored under on the Pi
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
// Blobs service: encrypted image blobs, stored on the Pi, never seen in the
// clear. All RPCs require a session token (wire-transport: Session
// authentication). A blob is opaque ciphertext; the Pi learns its size, its
// group and its timing, never its content or key.
service Blobs {
  // blob-store: Upload. Idempotent on `blob_id`.
  rpc UploadBlob(UploadBlobRequest) returns (UploadBlobResponse);
  // blob-store: Download. NOT_FOUND once the blob has expired or been deleted.
  rpc DownloadBlob(DownloadBlobRequest) returns (DownloadBlobResponse);
  // blob-store: Ack and delete. A member device reports it no longer needs the
  // blob; the Pi deletes it once every member device has acked (blob-store:
  // Expiry and ack-delete).
  rpc AckBlob(AckBlobRequest) returns (AckBlobResponse);
}

message UploadBlobRequest {
  // The group the image is for; the Pi authorizes download against membership
  // and scopes ack-delete to it (D4). The Pi already sees the matching
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
- **Group-bound authorization.** Upload and download name the `group_id`; the Pi
  allows them only from a current member device of that group (`PERMISSION_DENIED`
  otherwise). This leaks nothing the Pi did not already see (it routes that
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

### D6. Blob crypto: fresh key, XChaCha20-Poly1305, id as AAD

Per image the core generates a random `BLOB_ID_LEN` (16) byte id and a random
`BLOB_KEY_LEN` (32) byte key, then encrypts the WebP with XChaCha20-Poly1305: a
random 24-byte nonce prepended to the ciphertext, the tag appended, and the
`blob_id` as additional authenticated data so a blob cannot be served under the
wrong id. `MAX_BLOB_BYTES` = `MAX_IMAGE_BYTES` + nonce + tag (24 + 16), rounded.
The key is put in `Image.blob_key` and never leaves the core except inside the
MLS ciphertext (invariant: keys never leave the core). No dedup: every send is a
new id and key, even of the same picture.

### D7. Send and download order; idempotency

Send: the core stores the outgoing image message's plaintext and its encoded
WebP in the device media cache first (the sender cannot decrypt its own
ciphertext and must keep the plaintext, client-runtime: Local message store),
uploads the blob (idempotent on `blob_id`), and only after the upload is
accepted sends the `APPLICATION` envelope carrying `Content{Image{…}}`. A crash
between steps resumes: the stored message is marked sending, the blob re-uploads
idempotently, the envelope resends with the same `msg_id` (message-delivery:
Idempotent sends). Sending the blob before the envelope means a receiver never
sees an `Image` whose blob is missing because the upload had not happened.

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
  participant P as Pi (Blobs + Messaging)
  participant B as Rust core (Bob)
  UI->>C: send_image(bytes, caption)
  C->>C: decode, strip EXIF, scale ≤2048, WebP q80, ThumbHash
  C->>C: blob_id, key; XChaCha20 encrypt; store plaintext locally
  C->>P: Blobs.UploadBlob(group, blob_id, ciphertext)
  P-->>C: UploadBlobResponse(expires_at)
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

The blob store is not tied to the Pi (owner, 2026-11-19). The server writes blob
bytes through a `BlobStore` backend trait — `put`, `get`, `delete` and `exists`
by `blob_id`, returning opaque ciphertext — so a future backend (an
object-storage bucket, other cheap media) drops in without touching the wire
protocol or the specs. The **MVP ships one backend, the local filesystem**; the
trait is the seam for the rest.

What stays in the Pi, independent of the backend: the blob *metadata* (id, group,
size, `uploaded_at`, `expires_at`, ack set) lives in the Pi's SQLite, and so does
all authorization, expiry and ack-delete logic (D4, D9). The backend only stores
and returns opaque bytes by key; it never decides who may read a blob or when it
dies. So group-bound authorization and ack-delete work the same whether the bytes
sit on a drive or in a bucket.

The filesystem backend writes under one **stable, configured path**, the same on
every start (owner: the drive is auto-mounted at a fixed path). Config is a
`[blob_store]` table — `backend = "filesystem"` and, for it, `path = "…"`; when
the table is absent the feature is off. The database and keys stay in
`/var/lib/vyajena` on the Pi's main storage (invariant; pi-runtime: Durable
storage unchanged). Blobs are not backed up (`backup.sh` keeps covering DB + keys
only); losing them loses at most 7 days of re-sendable images (owner).

**No extra at-rest encryption layer (owner, 2026-11-19):** blobs are already
client-side ciphertext and the decryption key never reaches the server, so a LUKS
or encrypted-volume layer would cost real complexity (a headless Pi needing a
passphrase at boot) for almost no gain. Each backend relies on whatever at-rest
protection it natively has — none extra for the filesystem; a bucket brings its
own.

**Degrade gracefully when the backend is unavailable** (the drive not mounted, a
bucket unreachable): the server still starts and serves all text messaging, and
`Blobs` RPCs return `UNAVAILABLE` until the backend is back. The filesystem
backend checks its path is the real mounted store, not a bare directory on the
root filesystem, so a blob is never written to the SD card by mistake.

Deployment of the default backend goes through `deploy/pi/` only (CLAUDE.md): the
drive auto-mounts at the stable path (the owner's setup, not scripted by us);
`deploy.sh` ensures the directory exists, owned by `vyajena` mode 0700; the
service unit gets a writable path for it (today only `StateDirectory` is writable
under `ProtectSystem=strict`); `check.sh` verifies the path, ownership and mode
and warns, without failing, when the store is absent.

### D9. Expiry, ack-delete and "media expired — resend"

The Pi deletes a blob when either 7 days (`BLOB_TTL`) pass since upload, or every
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
A received image is re-downloadable from the Pi within 7 days if evicted. The
sender's own sent images are kept in the cache until `BLOB_TTL` so "resend"
works, then are eligible for eviction like any entry. The cache stores plaintext
WebP only; the per-blob key stays with the message in the store, so an evicted
image is re-fetched and re-decrypted, never kept decrypted beyond the cache.

### D11. Old clients, versioning and push

The `Image` body is field 5 of the `Content.body` oneof; a client built before
this change decodes it as an unset body and shows the existing "needs a newer
app version" placeholder (message-content: Inner content format) — no crash, no
wire-version bump. A sender does not suppress an image when some members are old;
those members see the placeholder. The `Blobs` service is additive; a
pre-media device never calls it. Nothing new leaves the device for the Pi, the
relay or push: the image rides as one `APPLICATION` envelope like any message,
so its size class and the wake ping are unchanged — one fixed "New message"
(push invariant). The blob upload is a separate authenticated RPC the Pi sees as
an opaque ciphertext of known size; it carries no content and no push.

## Risks / Trade-offs

- [A receiver opens an image but the blob upload never completed] → the core
  uploads the blob and waits for its acceptance before sending the envelope
  (D7), so the envelope implies the blob exists; a transient `NOT_FOUND` within
  7 days is shown as "not downloaded, retry", distinct from "expired".
- [The Pi learns which groups send images, how large and when] → accepted: it
  already routes and sizes that group's envelopes; the blob content and key stay
  encrypted. The anonymous model (D4) would hide the group at the cost of
  ack-delete.
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
- Pi: a schema migration adds blob metadata (blob_id, group_id, size,
  `uploaded_at`, `expires_at`, ack set). The blob bytes live in the storage
  backend (files under the filesystem path), not in SQLite.
- Deploy: `deploy/pi/` wires the filesystem backend at the stable path; the
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

No open questions remain for this change.
