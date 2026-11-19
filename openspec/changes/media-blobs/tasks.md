> Owner decisions resolved 2026-11-19 (design.md): caption reuses the text limit
> (D3); a new `Blobs` service, unary, 2 MB cap, group-bound + ack-delete (D4);
> a pluggable `BlobStore` backend with the filesystem backend only for the MVP,
> one stable path, no extra at-rest encryption, graceful degradation (D8).

## 1. Proto and shared limits

- [ ] 1.1 `content.proto`: `Image image = 5` in `Content.body`; new `Image`
  (`blob_id`, `blob_key`, `width`, `height`, `thumbhash`, `blob_size`,
  `caption`, `caption_mentions`). `blobs.proto` (new): `Blobs` service
  (`UploadBlob`, `DownloadBlob`, `AckBlob`) and its messages. `buf lint`,
  `buf breaking`; `cargo run -p pbgen`
- [ ] 1.2 `protocol::blob_store` (new module): `MAX_BLOB_BYTES`,
  `MAX_IMAGE_BYTES` (2 MB), `MAX_IMAGE_EDGE` (2048), `WEBP_QUALITY` (~80),
  `BLOB_ID_LEN` (16), `BLOB_KEY_LEN` (32), `MAX_THUMBHASH_BYTES`, `BLOB_TTL`
  (7 days); unit test for the size relation (`MAX_BLOB_BYTES` = image + nonce +
  tag)

## 2. Spike: the image pipeline crate stack (before core code)

- [ ] 2.1 `spikes/webp-thumbhash/`: prove lossy WebP encode + decode, EXIF
  read/orient/strip, ThumbHash encode, and XChaCha20-Poly1305 with chosen
  crates, and that they cross-compile for the iOS simulator and an Android
  target (libwebp is a C dependency, like the native key stores). Record the
  crate choices and the result in `design.md` under D5/D6, then delete the
  spike (config.yaml: spikes are throwaway)

## 3. Pi blob store

- [ ] 3.1 `server/src/blobs/` (new feature): a `BlobStore` backend trait
  (`put`/`get`/`delete`/`exists` by id, opaque bytes) with a filesystem backend
  at the stable path; the `Blobs` service — idempotent `UploadBlob`
  (member-of-group check, size cap, `ALREADY_EXISTS` on a mismatched re-upload),
  `DownloadBlob` (member check, `NOT_FOUND` when gone); schema migration for blob
  metadata (id, group, size, `uploaded_at`, `expires_at`, ack set) in SQLite,
  independent of the backend. Tests `blob-store/upload`, `blob-store/download`,
  `blob-store/blobs-are-opaque-ciphertext`, `blob-store/pluggable-storage-backend`
- [ ] 3.2 `AckBlob`, 7-day expiry sweep and ack-delete (uploader acks
  implicitly); test `blob-store/expiry-and-ack-delete`
- [ ] 3.3 Config `[blob_store]` (optional; `backend` + filesystem `path`) and
  graceful degradation (backend absent/unwritable → start, serve text, `Blobs`
  RPCs `UNAVAILABLE`, nothing on main storage). Tests
  `blob-store/blob-storage-and-graceful-degradation`,
  `pi-runtime/blob-storage-degrades-gracefully`, `pi-runtime/configuration`

## 4. Core media

- [ ] 4.1 `client/src/media/` (new feature): encode pipeline — decode, apply +
  strip EXIF orientation, downscale ≤2048, WebP q~80 within 2 MB (step quality
  then size), first-frame for animated, refuse undecodable, ThumbHash,
  `width`/`height`. Tests `client-runtime/image-encode-pipeline`
- [ ] 4.2 Blob crypto (fresh key + id, XChaCha20-Poly1305, `blob_id` AAD, no
  dedup); store migration for image message columns; `Client::send_image`
  (store plaintext + WebP, upload, then send envelope; resend reuses `msg_id` +
  `blob_id`). Tests `client-runtime/blob-crypto-and-send-order`,
  `message-content/image-messages`, `message-content/image-caption-and-mentions`,
  old-client decode (D11)
- [ ] 4.3 `Client::download_image` (download, decrypt + AAD check, decode,
  cache, ack), device media cache (separate store, OS file encryption, excluded
  from backup, LRU cap), expired handling + sender resend. Tests
  `client-runtime/image-download-and-cache`, `client-runtime/expired-media`
- [ ] 4.4 End-to-end across two cores against the real server: send + receive an
  image with a caption mention; `message-content/image-messages-stay-encrypted`
  (Pi push sink records "New message")

## 5. Bridge

- [ ] 5.1 `send_image`, `download_image`, image fields on `ChatMessage`
  (blob id, dims, ThumbHash, caption, mentions, state), caption `MentionSpan`
  (UTF-16); UTF-8↔UTF-16 tests; `flutter_rust_bridge_codegen generate`

## 6. App

- [ ] 6.1 Composer: image picker and caption field with the `@` mention picker;
  bytes + caption go through the core only. Widget tests `app-ui/send-an-image`
- [ ] 6.2 Image bubble and full-screen viewer: ThumbHash blur at aspect ratio →
  full image on tap, caption with highlighted mentions, "media expired —
  resend" and download retry. Widget tests `app-ui/image-bubble-and-viewer`

## 7. Deploy (Pi)

- [ ] 7.1 `deploy/pi/`: ensure the filesystem backend's stable path exists owned
  by `vyajena` mode 0700 (the drive is auto-mounted there by the owner, not
  scripted here), set `[blob_store]` in `config.toml`; `vyajena.service` gains a
  writable path for it. `shellcheck`
- [ ] 7.2 `check.sh` verifies the path, ownership and mode and warns (no fail)
  when the store is absent; `backup.sh` excludes blobs. `shellcheck`. Tests
  `pi-deployment/blob-store-deployment`, `pi-deployment/sandboxed-service`,
  `pi-deployment/backup-to-the-mac`

## 8. Check

- [ ] 8.1 `cargo test --workspace`, clippy `-D warnings`, `cargo fmt --check`,
  `buf lint`, `buf breaking`, `flutter analyze`, `flutter test`, `shellcheck`,
  `openspec validate media-blobs --strict`; confirm `cargo run -p pbgen` and
  `flutter_rust_bridge_codegen generate` leave no diff; no spike code under
  `spikes/`
