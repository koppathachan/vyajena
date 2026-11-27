## Why

The architecture doc lists voice among the "more media on the blob store built in
`media-blobs`". The owner asked (2026-11-20) for voice messages next: record a
short clip, send it as an encrypted blob like a picture, play it in the chat. The
blob store, its opaque-ciphertext rules, expiry and ack-delete already exist
(`media-blobs`); audio reuses all of it. This change stays at the proposal and
spec stage for now (owner, 2026-11-20); no code yet.

## What Changes

- **A new `Audio` body in the `Content` oneof** carrying a voice message: the blob
  id and key (as `Image` does), the blob size, the duration, and a bounded
  waveform. Old clients degrade an unknown body to the "needs a newer app"
  placeholder (message-content: Inner content format).
- **Codec (locked, owner 2026-11-19):** **Opus, mono**, in an **OGG container**
  (owner, 2026-11-20 — Ogg is the standard container for standalone Opus, RFC 7845,
  with mature native encode/decode on iOS and Android). One target bitrate for
  voice (~24–32 kbps).
- **Limits (owner, 2026-11-20):** duration capped at **~2 minutes**; the blob
  reuses `protocol::blob_store::MAX_BLOB_BYTES` unchanged (Opus mono at 2 minutes
  is ≈ 0.5 MB, well under it). No new size constant.
- **Waveform (owner, 2026-11-20):** the core computes a small bounded array of
  **amplitude peaks** and stores it in the `Audio` body, like a ThumbHash for an
  image, so the bubble draws a waveform before or without downloading.
- **Audio is an encrypted blob, exactly like an image** (blob-store): opaque
  ciphertext on the server, the key only inside the encrypted `Content`, download
  on tap, ack-delete and 7-day expiry. **No new `Blobs` RPC.**
- **Recording (owner, 2026-11-20):** **hold-to-record** in the composer — press and
  hold the mic, release to stop and preview (play / delete / send), slide to
  cancel while holding.
- **Playback:** a bubble with a play/pause control, the waveform and the duration;
  tapping downloads (first time) then plays.
- **The core owns the audio pipeline** (client-runtime: the core owns every media
  transform; keys never leave the core): it records or is handed raw audio,
  encodes Opus/OGG mono, enforces the duration and size, computes the waveform,
  and does the blob crypto and send order the same way it does for images. Dart
  never encodes or uploads audio.
- **Out of scope / deferred:**
  - **Captions on audio** — no for MVP (owner, 2026-11-21: voice note only). No
    caption field in the `Audio` body; add later as `Image` has it if wanted.
  - Video, audio calls, background/looping playback, speed controls,
    transcription, albums of clips.
- Details and decisions: `design.md`.

## Open decisions resolved (owner, 2026-11-20)

- **Container:** OGG (standalone Opus, RFC 7845).
- **Duration / size:** ~2 minutes; reuse `MAX_BLOB_BYTES`.
- **Waveform:** amplitude peaks in the `Audio` body.
- **Record UI:** hold-to-record, release to preview.

## Decisions resolved (owner, 2026-11-21)

- **Captions on audio:** no — voice note only for MVP. No caption field in the
  `Audio` body; can be added later exactly as `Image` has it.
- **Record + playback plugins:** `record` (recording) and `just_audio` (playback),
  both iOS + Android; the build-time spike (design S1) validates them before any
  code relies on them.
- **Continuous playback:** no — one clip plays at a time; auto-advance through
  consecutive voice notes is a later change.

## Proto files touched

- `proto/vyajena/v1/content.proto`: a new `Audio` body in the `Content` oneof (the
  next free field; coordinate the field number with `delete-messages` if both land
  together).
- `proto/vyajena/v1/blobs.proto`: unchanged — an audio blob is a blob.
- `protocol/`: `MAX_AUDIO_DURATION_MS` (~120 000), the Opus voice bitrate, the OGG
  container marker, and `MAX_AUDIO_WAVEFORM_BYTES`; reuse `MAX_BLOB_BYTES`,
  `BLOB_TTL`, `BLOB_ID_LEN`, `BLOB_KEY_LEN`.
- All additive: `buf breaking` passes.

## Capabilities

### New Capabilities

- `audio-messages`: the `Audio` body, recording and encoding, the waveform,
  playback, and audio staying encrypted.

### Modified Capabilities

- `message-content`: the `Content` oneof gains `Audio`.
- `app-ui`: record a voice message and play one back.

## Impact

- **Server (`server/`):** none beyond what `media-blobs` built; an audio blob is an
  image-sized blob.
- **Core (`client/`):** an audio record/encode pipeline (Opus/OGG mono, duration
  and size caps, waveform peaks); the `Audio` body; blob crypto and send order
  reused from images; audio download, cache and expiry reused.
- **Bridge and app:** a `ChatBody::Audio`; the composer's hold-to-record and
  preview; the playback bubble; the `record` and `just_audio` plugins (to confirm).
- **Depends on:** `media-blobs` (the blob store and media send/download machinery).
  Independent of `dms`, `lobby-multi-group` and `delete-messages`.
