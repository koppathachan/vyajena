## 0. Before starting

- [x] 0.1 `media-blobs` merged (the blob store and media send/download machinery)
- [x] 0.2 Owner decides the deferred items: captions on audio (proposed: no); the
      record/playback plugins (proposed: `record` + `just_audio`)

## 1. Spikes

- [x] 1.1 S1: a cross-platform record + playback plugin pair (proposed `record` +
      `just_audio`) that lets the core own the Opus/OGG encode; result in design D4

## 2. Proto and shared rules

- [x] 2.1 `content.proto`: `Audio` body in the `Content` oneof (next free field);
      `buf lint`, `buf breaking`; `cargo run -p pbgen`
- [x] 2.2 `protocol::audio`: `MAX_AUDIO_DURATION_MS`, the Opus voice bitrate, the
      OGG container, `MAX_AUDIO_WAVEFORM_BYTES`; tests
      `audio-messages/recording-and-encoding`

## 3. Core

- [ ] 3.1 Audio encode pipeline: raw audio → Opus/OGG mono at the target bitrate,
      enforce duration and `MAX_BLOB_BYTES`, compute the waveform; tests
      `audio-messages/recording-and-encoding`
- [ ] 3.2 `Audio` body; blob crypto, send order, download, cache, expiry and resend
      reused from images; tests `audio-messages/audio-messages`,
      `audio-messages/audio-stays-encrypted`

## 4. Bridge

- [ ] 4.1 `ChatBody::Audio` (duration, waveform, media status), `send_audio`,
      playback hooks; `flutter_rust_bridge_codegen generate`; bridge end-to-end test

## 5. App

- [ ] 5.1 Composer hold-to-record with slide-to-cancel and a preview; widget tests
      `app-ui/record-a-voice-message`
- [ ] 5.2 Playback bubble: play/pause, waveform, duration, download-on-tap; widget
      tests `app-ui/play-a-voice-message`

## 6. Check

- [ ] 6.1 `cargo test --workspace`, clippy, fmt, `buf lint`, `buf breaking`,
      `flutter analyze`, `flutter test`, `openspec validate audio-messages
      --strict`
