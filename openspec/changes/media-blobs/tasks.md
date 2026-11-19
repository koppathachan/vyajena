> Owner decisions resolved 2026-11-19 (design.md): caption reuses the text limit
> (D3); a new `Blobs` service, unary, 2 MB cap, group-bound + ack-delete (D4);
> a pluggable `BlobStore` backend with the filesystem backend only for the MVP,
> one stable path, no extra at-rest encryption, graceful degradation (D8).

## 1. Proto and shared limits

- [x] 1.1 `content.proto`: `Image image = 5` in `Content.body`; new `Image`
  (`blob_id`, `blob_key`, `width`, `height`, `thumbhash`, `blob_size`,
  `caption`, `caption_mentions`). `blobs.proto` (new): `Blobs` service
  (`UploadBlob`, `DownloadBlob`, `AckBlob`) and its messages. `buf lint`,
  `buf breaking`; `cargo run -p pbgen`
- [x] 1.2 `protocol::blob_store` (new module): `MAX_BLOB_BYTES`,
  `MAX_IMAGE_BYTES` (2 MB), `MAX_IMAGE_EDGE` (2048), `WEBP_QUALITY` (~80),
  `BLOB_ID_LEN` (16), `BLOB_KEY_LEN` (32), `MAX_THUMBHASH_BYTES`, `BLOB_TTL`
  (7 days); unit test for the size relation (`MAX_BLOB_BYTES` = image + nonce +
  tag)

## 2. Spike: the image pipeline crate stack (before core code)

- [x] 2.1 `spikes/webp-thumbhash/`: prove lossy WebP encode + decode, EXIF
  read/orient/strip, ThumbHash encode, and XChaCha20-Poly1305 with chosen
  crates, and that they cross-compile for the iOS simulator and an Android
  target (libwebp is a C dependency, like the native key stores). Record the
  crate choices and the result in `design.md` under D5/D6, then delete the
  spike (config.yaml: spikes are throwaway)

## 3. Pi blob store

- [x] 3.1 `server/src/blobs/` (new feature): a `BlobStore` backend trait
  (`put`/`get`/`delete`/`exists` by id, opaque bytes) with a filesystem backend
  at the stable path; the `Blobs` service — idempotent `UploadBlob`
  (member-of-group check, size cap, `ALREADY_EXISTS` on a mismatched re-upload),
  `DownloadBlob` (member check, `NOT_FOUND` when gone); schema migration for blob
  metadata (id, group, size, `uploaded_at`, `expires_at`, ack set) in SQLite,
  independent of the backend. Tests `blob-store/upload`, `blob-store/download`,
  `blob-store/blobs-are-opaque-ciphertext`, `blob-store/pluggable-storage-backend`
- [x] 3.2 `AckBlob`, 7-day expiry sweep and ack-delete (uploader acks
  implicitly); test `blob-store/expiry-and-ack-delete`
- [x] 3.3 Config `[blob_store]` (optional; `backend` + filesystem `path`) and
  graceful degradation (backend absent/unwritable → start, serve text, `Blobs`
  RPCs `UNAVAILABLE`, nothing on main storage). Tests
  `blob-store/blob-storage-and-graceful-degradation`,
  `pi-runtime/blob-storage-degrades-gracefully`, `pi-runtime/configuration`

## 4. Core media

- [x] 4.1 `client/src/media/` (new feature): encode pipeline — decode, apply +
  strip EXIF orientation, downscale ≤2048, WebP q~80 within 2 MB (step quality
  then size), first-frame for animated, refuse undecodable, ThumbHash,
  `width`/`height`. Tests `client-runtime/image-encode-pipeline`
- [x] 4.2 Blob crypto (fresh key + id, XChaCha20-Poly1305, `blob_id` AAD, no
  dedup, nonce derived from key and id); store migrations `0007_media.sql` (the
  cache index and the expired blobs) and `0008_blob_pending.sql`;
  `Client::send_image` stores the message queued, with its plaintext in the cache
  and no connection needed (owner: images queue offline like text, design D7).
  Tests `client-runtime/blob-crypto-and-send-order`,
  `message-content/image-messages`, `message-content/image-caption-and-mentions`,
  old-client decode (D11)
- [x] 4.2a The outbox uploads a queued image's blob first, in a task of its own,
  and sends the envelope only after the Pi accepted it; later messages of the
  chat wait; retry, refusal, missing-picture and 7-day rules (D7). Tests
  `client-runtime/blob-crypto-and-send-order`: offline queue and order, a
  restart, the Pi's drive away, the picture cleared, waited too long
- [x] 4.3 `Client::download_image` (download, decrypt + AAD check, decode,
  cache, ack), device media cache (separate store, OS file encryption, excluded
  from backup, LRU cap), expired handling + sender resend. Tests
  `client-runtime/image-download-and-cache`, `client-runtime/expired-media`
- [x] 4.4 End-to-end across two cores against the real server: send + receive an
  image with a caption mention; `message-content/image-messages-stay-encrypted`
  (Pi push sink records "New message")

## 5. Bridge

- [x] 5.1 `send_image`, `download_image`, image fields on `ChatMessage`
  (blob id, dims, ThumbHash, caption, mentions, state), caption `MentionSpan`
  (UTF-16); UTF-8↔UTF-16 tests; `flutter_rust_bridge_codegen generate`

## 6. App

- [x] 6.1 Composer: image picker (`image_picker` behind a `PicturePicker`, the
  one new Dart dependency) and caption field with the `@` mention picker; bytes +
  caption go through the core only. Widget tests `app-ui/send-an-image`
- [x] 6.2 Image bubble and full-screen viewer: ThumbHash blur at aspect ratio →
  full image on tap, caption with highlighted mentions, "media expired —
  resend", download retry, and "Resend picture" on the sender's long-press menu.
  Widget tests `app-ui/image-bubble-and-viewer`

## 7. Deploy (Pi)

- [x] 7.1 `deploy/pi/`: `pi.conf` `BLOB_DIR` (empty: images off) renders
  `[blob_store]`; a systemd drop-in lets the service write the path; deploy gives
  the path to `vyajena` (0700) only if it is on a mount of its own, never makes
  it and never mounts it (the owner's auto-mount does); a reset deletes the
  blobs. `shellcheck`
- [x] 7.2 `check.sh` verifies the mount, ownership, mode and that the running
  service can write, and warns (no fail) when the drive is absent; `backup.sh`
  excludes blobs. `shellcheck`. Tests `pi-deployment/blob-store-deployment`,
  `pi-deployment/sandboxed-service`, `pi-deployment/backup-to-the-mac`,
  `pi-deployment/hardening-check`, `pi-deployment/scripted-reset`

## 8. Check and docs

- [x] 8.1 `cargo test --workspace`, clippy `-D warnings`, `cargo fmt --check`,
  `buf lint`, `buf breaking`, `flutter analyze`, `flutter test`, `shellcheck`,
  `openspec validate media-blobs --strict`; confirm `cargo run -p pbgen` and
  `flutter_rust_bridge_codegen generate` leave no diff; no spike code under
  `spikes/`
- [x] 8.2 The invariant "text only" is now "text and one picture per message":
  `docs/architecture-decisions.md`, `openspec/config.yaml` context and
  `README.md` updated. (`CLAUDE.md` line 3 still says "text-only chat": left for
  the owner, it is the project's instruction file.)
