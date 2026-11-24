## Why

The architecture doc lists video under the blob-store "Future". `audio-messages`
added voice notes; this is its sibling for short **video messages**. The owner
locked the format and sourcing decisions on 2026-11-23; this change authors the
spec. Planning only — no code, no deploy.

## What Changes

- **A new `Video` body in the `Content` oneof** carrying a short video message: the
  blob id and key (as `Image`/`Audio` do), the blob size, the duration, the pixel
  `width`/`height`, and a first-frame **ThumbHash** (owner, 2026-11-23) so the
  bubble lays out and shows a blur + play before download — the video analogue of
  the image ThumbHash. Old clients degrade an unknown body to the "needs a newer
  app" placeholder.
- **Format (locked, owner 2026-11-23): MP4 container, H.264 (AVC) video + AAC
  audio.** Chosen for universal hardware encode *and* decode on phones and parity
  with mainstream messengers; explicitly **not** WebM/VP9 (no hardware VP9 encode
  on phones) and deliberately not Opus-consistent with `audio-messages`.
- **Normalize target (locked, owner 2026-11-23): 480p, ~1 Mbps.** Downscale the
  long edge to 854, H.264 ~1 Mbps + AAC, so a 30 s clip is ≈ 4 MB.
- **Duration cap 30 s, CONFIG-DRIVEN (locked, owner 2026-11-23):** the server reads
  a maximum blob size from its config file at startup and enforces it on blob
  **size** (the server only ever sees ciphertext bytes, never duration).
  `protocol/` keeps a hard ceiling and a default, so config can only tune within a
  safe bound. Clients fetch the current size limit from the server and enforce the
  30 s **duration** client-side. Changing the limit is a config edit + server
  restart, no rebuild. This is a **deliberate, documented departure** from the
  invariant "limits live compile-time in `protocol/`".
- **Two sources (locked, owner 2026-11-23):**
  - **In-app record** — capture configured to H.264/MP4 directly (overriding the
    iOS HEVC default), bounded to 30 s at capture, no transcode.
  - **Gallery pick** — reuse the `image_picker` family in video mode. An
    over-limit clip gets a **trim-to-30 s slider** (like WhatsApp), not a rejection.
- **Normalize in a NATIVE PLUGIN; the core does crypto only (locked, owner
  2026-11-23):** gallery clips are normalized — transcoded to canonical
  H.264/MP4, downscaled, bitrate/size-capped and trimmed to ≤ 30 s — by a platform
  plugin (AVFoundation on iOS, MediaCodec on Android), **not** the pure-Rust core.
  The core does only the blob crypto. The normalized plaintext sits briefly in the
  app's private, encrypt-at-rest temp (the same place today's image/audio cache
  lives) before the core encrypts it. This is a **deliberate, documented bend** of
  "the core owns every media transform" — unavoidable for video; stated honestly
  in `design.md`.
- **Where (owner, 2026-11-23): groups and DMs, like images** — the lobby, named
  groups and DMs.
- **Advertised limit (owner, 2026-11-23):** the server advertises the current max
  video blob size as a new field on `ServerInfo.Limits`, returned by the existing
  `GetServerInfo` handshake; no new RPC.
- **Blob reuse (locked, owner 2026-11-23):** reuse the existing image/audio blob
  machinery unchanged — upload-then-`APPLICATION` send order, media cache,
  ack-delete, 7-day expiry; **no new `Blobs` RPC** for storage. The one blob-store
  change is that the server's max blob size becomes config-driven (video-sized)
  within a protocol ceiling.
- **Out of scope:** video calls (see `audio-calls`); GIFs/animation beyond a single
  clip; multiple clips per message; in-body full poster images; editing/filters;
  background upload beyond today's queue.
- Details and decisions: `design.md`.

## Open decisions resolved (owner, 2026-11-23)

- **Normalize target:** 480p, ~1 Mbps (30 s ≈ 4 MB).
- **Where allowed:** groups and DMs, like images.
- **Poster:** a first-frame ThumbHash in the `Video` body (no separate poster blob).
- **Limit advertisement:** a new field on `ServerInfo.Limits` via `GetServerInfo`.

## Documented departures from invariants (owner-approved, 2026-11-23)

- **Config-driven limit vs "limits live compile-time in `protocol/`":** the max
  blob size is read from server config within a `protocol/` hard ceiling + default.
  `docs/architecture-decisions.md` and `openspec/config.yaml` context note this.
- **Native-plugin transcode vs "the core owns every media transform":** video
  normalization runs in a platform plugin; the core keeps the crypto. Both the doc
  and `client-runtime` state the bend and why (no viable pure-Rust mobile H.264
  transcode).

## Proto files touched

- `proto/vyajena/v1/content.proto`: a new `Video` body in the `Content` oneof (the
  next free field; coordinate the number with `delete-messages` and `audio-calls`
  — `audio-messages` took field 6 — so they do not collide; `buf breaking` catches
  a clash).
- `proto/vyajena/v1/devices.proto`: a new `Limits.max_video_blob_bytes` field.
- `proto/vyajena/v1/blobs.proto`: unchanged — a video blob is a blob.
- `protocol/`: `MAX_VIDEO_BYTES` (hard ceiling), `DEFAULT_MAX_VIDEO_BYTES`,
  `MAX_VIDEO_DURATION_MS` (30 000), the H.264/MP4/AAC normalize target (480p,
  ~1 Mbps), and the config key for the tunable max; reuse `BLOB_TTL`,
  `BLOB_ID_LEN`, `BLOB_KEY_LEN`, `MAX_THUMBHASH_BYTES`.
- All additive: `buf breaking` passes.

## Capabilities

### New Capabilities

- `video-messages`: the `Video` body and poster, recording and gallery import with
  trim, native-plugin normalization, and the config-driven duration/size limit.

### Modified Capabilities

- `message-content`: the `Content` oneof gains `Video`.
- `blob-store`: the server's max blob size is config-driven within a protocol
  ceiling.
- `wire-transport`: `GetServerInfo` advertises the max video blob size.
- `app-ui`: send a video (record / gallery-with-trim) and the video bubble + player.
- `client-runtime`: video normalization runs in a native plugin; the core does the
  blob crypto and reuses the image/audio send order.

## Impact

- **Server (`server/`):** read `max_video_blob_bytes` from config at startup, clamp
  it to `[image cap, MAX_VIDEO_BYTES]`, enforce it as the blob upload size cap, and
  return it in `ServerInfo.Limits`. No new RPC, no new table (a video blob is a
  blob).
- **Core (`client/`):** a `Video` body; blob crypto, upload-then-send order, cache,
  ack-delete, expiry and resend reused from images/audio; it receives the
  already-normalized MP4 bytes from the app and encrypts them; it computes/holds the
  ThumbHash if the plugin does not. Dart never gets the key.
- **Bridge and app:** record (H.264/MP4, 30 s) and gallery pick + trim-to-30 s; the
  native normalize plugin; a video bubble (blur poster → play, download on tap) and
  a full-screen player; the normalized plaintext in the encrypt-at-rest temp.
- **Config/ops:** a documented `max_video_blob_bytes` key in
  `deploy/server/config.toml` and `server/local.toml`; changing it is edit +
  restart, no rebuild.
- **Depends on:** `media-blobs` (the blob store and send/download machinery).
  Independent of the other pending changes beyond coordinating the `Content` field
  number.
