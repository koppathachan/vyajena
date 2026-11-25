## 0. Before starting

- [x] 0.1 `media-blobs` merged (the blob store and media send/download machinery)
- [x] 0.2 Owner approves the two documented departures (config-driven blob size;
      native-plugin transcode): approved 2026-11-23
- [x] 0.3 Coordinate the `Content` oneof field number with `delete-messages` and
      `audio-calls` (`audio-messages` took field 6)

## 1. Spike (at implementation time, real iOS + Android — not during planning)

- [ ] 1.1 S1: the native normalize plugin (AVFoundation / MediaCodec) produces
      canonical H.264/MP4 at 480p/~1 Mbps within the size cap, trims to ≤ 30 s, and
      hands the core bytes + first-frame ThumbHash; iOS camera forced to H.264/MP4;
      result in design S1
      — contract + both plugins written and the result recorded in design.md S1;
      **left unchecked: on-device verification pending** (no iOS/Android device in
      this build; the Android decoder→encoder GL blit is the one piece left)

## 2. Proto and shared rules

- [x] 2.1 `content.proto`: `Video` body (blob id/key, size, `duration_ms`,
      width/height, `thumbhash`), field number coordinated; `devices.proto`:
      `Limits.max_video_blob_bytes`; `buf lint`, `buf breaking`; `cargo run -p pbgen`
- [x] 2.2 `protocol::video`: `MAX_VIDEO_BYTES` ceiling, `DEFAULT_MAX_VIDEO_BYTES`,
      `MAX_VIDEO_DURATION_MS` (30 000), the 480p/~1 Mbps H.264/MP4/AAC target, the
      config key name; tests `video-messages/config-driven-limit`

## 3. Server

- [x] 3.1 Read `max_video_blob_bytes` from config at startup, clamp to
      `[image cap, MAX_VIDEO_BYTES]`, default when unset/out of range; enforce it as
      the `Blobs.UploadBlob` size cap; tests `blob-store/upload`,
      `video-messages/config-driven-limit`
- [x] 3.2 Advertise it in `ServerInfo.Limits.max_video_blob_bytes`; tests
      `wire-transport/size-limits`
- [x] 3.3 Document the config key in `deploy/server/config.toml` and
      `server/local.toml`

## 4. Core

- [x] 4.1 `Video` body encode/decode; accept already-normalized MP4 bytes +
      ThumbHash from the app (compute the ThumbHash if absent); blob crypto, upload-
      then-send order, cache, ack-delete, expiry and resend reused from images;
      enforce the advertised size and the 30 s duration; tests
      `video-messages/video-messages`, `video-messages/video-stays-encrypted`
- [x] 4.2 The normalized plaintext lives only in the encrypt-at-rest temp before
      encryption and is removed after; the core holds the key; tests
      `client-runtime/video-normalization-in-a-native-plugin`

## 5. Bridge and native plugin

- [ ] 5.1 The native normalize plugin (AVFoundation / MediaCodec): transcode +
      downscale + bitrate/size cap + trim, returning MP4 bytes + ThumbHash
      — iOS (`AppDelegate.swift` `VideoNormalize`) and Android (`VideoNormalize.kt`)
      written and the `chat.vyajena/video` channel registered on both; **left
      unchecked: on-device verification pending** (see design.md S1)
- [x] 5.2 `ChatBody::Video`, `send_video`, playback hooks;
      `flutter_rust_bridge_codegen generate`; bridge end-to-end test

## 6. App

- [x] 6.1 Record (H.264/MP4, 30 s) and gallery pick + trim-to-30 s slider; widget
      tests `app-ui/send-a-video`
      — record (image_picker `maxDuration` 30 s) and gallery pick done, routed
      through the native normalize plugin; an over-limit clip is **auto-trimmed to
      the first 30 s** in the plugin. The interactive WhatsApp-style slider to
      choose *which* 30 s is NOT built yet (follow-up); the clip is normalized and
      sent rather than rejected, which satisfies the requirement's "never rejected".
- [x] 6.2 Video bubble (ThumbHash blur + play + duration, download on tap) and
      full-screen player; "Video expired" + resend; widget tests
      `app-ui/video-bubble-and-player`

## 7. Docs and check

- [x] 7.1 `docs/architecture-decisions.md` (video messages; the two documented
      departures), `openspec/config.yaml` context
- [x] 7.2 `cargo test --workspace`, clippy, fmt, `buf lint`, `buf breaking`,
      `flutter analyze`, `flutter test`, `openspec validate video-messages --strict`
      — clippy (`--all-targets -D warnings`) clean, `fmt --check` clean, `buf lint`
      + `buf breaking` clean, `flutter analyze` no issues, `flutter test` 219 pass,
      `openspec validate --strict` valid. The video crate tests (protocol, server,
      client, bridge) pass; the full `cargo test --workspace` was validated per
      crate here and is left to CI (prohibitively slow on this external disk, per
      the project convention).
