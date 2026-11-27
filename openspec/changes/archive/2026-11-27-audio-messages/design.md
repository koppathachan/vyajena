## Context

- **`media-blobs` built the whole media path.** `Blobs.UploadBlob` /
  `DownloadBlob` / `AckBlob` store opaque ciphertext; the `Image` body carries a
  `blob_id`, `blob_key`, size and a ThumbHash; the core owns the transform, the
  blob crypto (XChaCha20-Poly1305, `blob_id` as AAD), the send order (upload the
  blob, then the `APPLICATION` envelope), the cache, expiry and resend. An audio
  message is the same shape with a different payload.
- **The `Content` oneof degrades unknown bodies** (message-content: Inner content
  format), so adding `Audio` is safe for old apps.
- **The core owns every media transform; keys never leave it** (client-runtime).
- **Owner decisions (2026-11-20):** Opus/mono (locked 2026-11-19) in OGG; ~2 min
  reusing `MAX_BLOB_BYTES`; amplitude peaks in the body; hold-to-record.

## Goals / Non-Goals

**Goals**
- Record, send and play a short voice message, end-to-end encrypted like an image.
- Reuse the blob store and the image send/download/expiry machinery unchanged.
- A waveform preview that needs no download.

**Non-goals**
- Video, calls, background playback, speed controls, transcription.
- A new `Blobs` RPC or any server change.
- Captions on audio in MVP (deferred).

## Decisions

### D1. The `Audio` body mirrors `Image`

- **`Audio { bytes blob_id; bytes blob_key; uint64 blob_size; uint32 duration_ms;
  bytes waveform; }`**
  - `blob_id` (16) / `blob_key` (32): as `Image`; the key travels only inside the
    encrypted `Content`.
  - `duration_ms`: at most `protocol::audio::MAX_AUDIO_DURATION_MS` (~120 000).
  - `waveform`: a bounded array of amplitude peaks (one byte per peak, a fixed
    small count), at most `protocol::audio::MAX_AUDIO_WAVEFORM_BYTES`; shown as the
    bar heights before the clip downloads.
- **No MIME field:** the container and codec are fixed (OGG/Opus mono), as WebP is
  fixed for images.
- **No caption field in MVP** (owner, 2026-11-21: voice note only). If the owner
  later wants captions, add `caption` + `caption_mentions` exactly as `Image` has them.

### D2. OGG container, Opus mono, one voice bitrate

- **OGG/Opus (RFC 7845)** is the standard for a standalone Opus stream, with mature
  native support on iOS and Android. Mono at ~24–32 kbps keeps a 2-minute clip
  ≈ 0.5 MB, well under `MAX_BLOB_BYTES`.
- **Why not WebM:** WebM/Opus is the web-streaming convention; for a stored mobile
  voice note OGG is more conventional and as widely decodable. The owner chose OGG
  (2026-11-20).

### D3. Reuse the image blob machinery wholesale

- **Blob crypto and send order** (client-runtime: Blob crypto and send order): the
  core encrypts the OGG with a fresh key and `blob_id`, uploads the blob, then
  sends the `APPLICATION` envelope — identical to an image, including offline
  queueing, crash recovery, the `UNAVAILABLE` backoff, and resend reusing the
  `msg_id`.
- **Download and cache / expired media** (client-runtime): download on tap,
  decrypt, cache in the media cache, ack; a deleted-on-server blob shows "audio
  expired"; the sender can resend from its cache.
- **No server change:** the server treats the blob as any blob; `MAX_BLOB_BYTES`,
  `BLOB_TTL` and ack-delete apply unchanged.

### D4. Recording: hold-to-record, core-owned encode

- **Hold-to-record** in the composer: press and hold the mic, release to stop;
  slide to cancel while holding. On release the composer shows a **preview**
  (play / delete / send).
- **Dart captures nothing for the wire.** The recorder plugin writes the clip to a
  temp file; Dart hands the **raw recorded bytes (or the file path) to the core**,
  which re-encodes to the canonical OGG/Opus mono at the target bitrate, enforces
  `MAX_AUDIO_DURATION_MS` and `MAX_BLOB_BYTES`, and computes the waveform. Dart
  never encodes Opus or computes the waveform for the wire, mirroring the image
  rule (client-runtime: the core owns every media transform).
- **Spike S1:** confirm a recorder and a player plugin that work on both iOS and
  Android and let the core take the raw audio. Proposed: `record` (captures
  Opus/OGG on both platforms) and `just_audio` (plays OGG/Opus on both). If a
  plugin cannot hand the core raw audio the core decodes and re-encodes to the
  canonical form. Record the result here.

### D5. Playback bubble

- **The bubble** shows a play/pause control, the waveform (from the body, filled as
  it plays), and the duration. First tap downloads (as an image does), then plays.
- **One clip plays at a time;** starting another stops the first. Playback stops
  when the chat closes.
- **No auto-advance** (owner, 2026-11-21): playing one voice note does not start the
  next; continuous playback is a later change.

### D6. Audio stays encrypted, push unchanged

- The audio reference, key and waveform travel only inside the encrypted
  `Content`; the server stores ciphertext with no key (blob-store). The wake ping
  stays the fixed "New message".

### D4 — Spike S1 result (2026-11-21)

Run on an iPhone 16 simulator (iOS 26.5) and an Android 15 (API 35) arm64 emulator
with `record` 7.2.0 and `just_audio` 0.10.6; the spike is deleted.

- **Capture:** `record` writes a **WAV** (PCM 16-bit, mono, 48 kHz) on both
  platforms (`AudioEncoder.wav`; the header on both reported fmt 1, 1 channel,
  48 000 Hz, 16 bit). Its own Opus encoder is **not canonical**: on iOS it
  produces a CAF container (`caff`), on Android an OGG one. So the recorder is
  asked for the WAV and **the core encodes the canonical OGG/Opus mono from it**;
  Dart hands the core the temp file's bytes and never encodes. The same WAV also
  plays in the preview through `just_audio` on both platforms.
- **Playback:** `just_audio` plays the core's OGG/Opus on both (iOS duration
  5.14 s for a 5.0 s clip, Android 5.006 s; both ran to `completed`).
  **Unverified:** iOS older than 26.5 (the app's floor is iOS 15; only the 26.5
  simulator runtime is installed here). If a device cannot play OGG/Opus the
  core can decode the cached blob to WAV for the player; not built, no
  evidence it is needed.
- **Core encoder: `opus-pure` 0.2.3** (pure Rust: Opus encoder and decoder plus the
  RFC 7845 OGG muxer and reader, no C, no `build.rs`). libopus through `opusic-sys`
  or `audiopus_sys` needs `cmake` (absent here, and a build-tool dependency for
  each phone target), so it was rejected. The spike encoded 5 s of mono at 24 and
  32 kbps (`Application::Voip`): output 13.5 and 17.3 kB (21 and 27 kbps
  effective), and libopus (`opusic-sys`) decoded the same packets to the same
  length and the same SNR as `opus-pure` did, so the OGG/Opus is standard.
  Risk: a young third-party crate; the core's own round-trip tests are the net.
- **Tooling note:** on this machine Gradle needs `JAVA_TOOL_OPTIONS=-Djava.net.useSystemProxies=true`
  to download its distribution (Java sockets cannot connect directly).

### D7. Decisions made while building (2026-11-21)

- **WAV in, path out (client-runtime: the core owns every media transform).**
  The recorder's WAV is the only audio Dart holds for the wire: it is a temp file
  the app deletes as soon as the clip is sent, deleted or cancelled (also when
  the composer is left mid-preview). A received clip is decrypted by the core into
  its media cache as an `.opus` file (the extension is what a player reads the
  format from) and the bridge hands Dart the **path**, never the bytes; Dart does
  not copy or keep it. The invariant "decrypted pictures reach Dart only to be
  shown" is read to cover playing the clip; the owner can veto, in which case
  the core must decode to PCM and stream it to a native player (a larger change).
- **Core input is a 16-bit PCM WAV** (mono or stereo, any rate): it mixes to mono
  and resamples (linear) when the rate is not one Opus takes. It refuses a
  clip over `MAX_AUDIO_DURATION_MS` by sample count before copying samples
  (`AudioError::TooLong`), and one with no samples (`Empty`).
- **The app stops recording at 119 s** (`voiceLimit`), a second under the core's
  120 s refusal, because the recorder keeps a little past the timer; a hold
  shorter than **1 s** is a stray tap and is dropped with a hint. Neither number
  is in the spec's scenarios.
- **Waveform:** 64 peaks (`AUDIO_WAVEFORM_PEAKS`), each the loudest sample of an
  even slice of the clip, scaled so the loudest bar is 255 (a quiet recording is
  still readable); silence is all zeros. `MAX_AUDIO_WAVEFORM_BYTES` is 128 so a
  later sender could be finer.
- **Voice message in a reply:** a voice message is not sent as a reply (no
  `reply_to` on `Audio`), so the mic is hidden while the composer is replying; a
  reply *to* a voice message quotes "Voice message".
- **Field number:** `Content.audio = 6`; `delete-messages` takes 7.
- **Errors:** the bridge adds `AudioUnsupported` and `AudioTooLong`; an expired
  clip reuses `MediaExpired`.

### D8. The microphone widens three recorded app invariants (owner to confirm)

Adding `record` and `just_audio` changed what four existing app tests assert, and I
updated them with the reason beside each: (1) the Android manifest declares
`RECORD_AUDIO` (app-release: declared permissions, as push added
`POST_NOTIFICATIONS`); (2) `Info.plist` declares `NSMicrophoneUsageDescription`
(so "asks for nothing" becomes "nothing but the microphone, on the first voice
message"); (3) the app's dependency list gains `record` and `just_audio`; (4)
`dart:io` and a `File` are allowed in the one file `lib/chat/voice_plugins.dart`
(the temp WAV), as `image_picker` is confined to `picture_picker.dart`. Nothing
else in `lib/` imports `dart:io`, a socket or HTTP. **Not done, owner's call:** the
store privacy sheet, the privacy policy and the Play data-safety answers (all
`app/store/` and `docs/`) do not yet mention the microphone or voice data.

## Spikes (before code that relies on them)

- **S1 (done, D4):** `record` + `just_audio`, with the core encoding the WAV the
  recorder writes; the spike is deleted.

## Risks / Trade-offs

- **Opus encode on device:** the chosen recorder should emit Opus directly; if not,
  the core re-encodes, costing CPU on send. Measured in S1.
- **Waveform cost:** computing peaks is cheap; bounded bytes in the body.
- **No caption** means no text with a voice note in MVP; deferred by the owner.

## Open Questions — resolved (owner, 2026-11-21)

1. **Captions on audio:** text-free voice note for MVP; no caption field.
2. **Plugins:** `record` + `just_audio`, validated by spike S1 on iOS + Android.
3. **Auto-play / continuous playback:** no; one clip at a time, auto-advance is a
   later change.
