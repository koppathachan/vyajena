## 0. Before starting

- [x] 0.1 The image/blob path from `media-blobs` is merged (reused unchanged)
- [x] 0.2 Owner confirms the MVP shape (on-device native only; sticker = transparent
      `Image`, no proto change; `SegmentationSource` seam; styling/encode in shared
      Rust): locked 2026-11-24
- [x] 0.3 No `Content` field number to coordinate — this change adds none

## 1. Spikes (at implementation time, real iOS + Android — not during planning)

- [ ] 1.1 S1: iOS Vision foreground/subject mask produces a straight-alpha RGBA
      cutout at the contract's format and orientation; record the minimum iOS
      version and the fallback in design S1
      — plugin + unpremultiply implemented, floor (iOS 17) and fallback recorded in
      design S1; **on-device run pending** (no device/simulator with a photo in CI)
- [ ] 1.2 S2: Android ML Kit Subject Segmentation produces the same RGBA cutout
      on-device with no model download; record device/SDK requirements in design S2
      — plugin + SDK dep recorded in design S2, incl. the Play-services on-demand
      model-download nuance for the owner; **on-device run pending** (no Play-services
      device/emulator in CI)
- [x] 1.3 S3: the core encodes WebP-with-alpha within `MAX_IMAGE_BYTES` with
      acceptable edges, and a transparent ThumbHash blurs sensibly; result in design
      S3 (verified: `client/tests/sticker_pipeline.rs`, `client/tests/stickers.rs`)

## 2. Shared rules (no proto)

- [x] 2.1 Confirm no `.proto` change is needed (a sticker is an `Image`); reuse
      `MAX_IMAGE_EDGE`, `MAX_IMAGE_BYTES`, `WEBP_QUALITY`, `MAX_THUMBHASH_BYTES`,
      `BLOB_*`
- [x] 2.2 `protocol::photo_sticker`: the RGBA contract constants (pixel format =
      straight-alpha 8-bit RGBA, sRGB, R,G,B,A byte order) and the
      `SegmentationSource` variant names; tests `photo-sticker/segmentation-seam`

## 3. Core

- [x] 3.1 A `SegmentationSource` abstraction (enum/trait) with the RGBA output
      contract; the core accepts an RGBA cutout from any source; tests
      `photo-sticker/segmentation-seam`
- [x] 3.2 Extend the image encode pipeline to encode WebP preserving the alpha
      channel for an RGBA cutout (trim transparent margins, downscale ≤
      `MAX_IMAGE_EDGE`, within `MAX_IMAGE_BYTES`), compute the ThumbHash, record
      `width`/`height`; styling/encode stay in the core; tests
      `client-runtime/image-encode-pipeline`,
      `client-runtime/sticker-styling-and-encode`
- [x] 3.3 Send the encoded sticker through the unchanged image blob crypto, upload-
      then-send order, cache, ack-delete, expiry and resend; tests
      `photo-sticker/sticker-is-an-image`

## 4. Bridge and native plugins

- [x] 4.1 iOS Vision segmentation plugin: picked photo → RGBA cutout per the
      contract; returns bytes + `width`/`height` across the bridge
      (`app/ios/Runner/Sticker.swift`; on-device run tracked by S1)
- [x] 4.2 Android ML Kit Subject Segmentation plugin: same contract, on-device
      (`app/android/.../Sticker.kt`; model fetched on demand by Play services — see
      design S2; on-device run tracked by S2)
- [x] 4.3 Bridge `send_sticker_in` hook that hands the core the RGBA and the
      `SegmentationSource`; `flutter_rust_bridge_codegen generate`; bridge
      end-to-end test (`bridge/tests/stickers.rs`)

## 5. App

- [x] 5.1 A make-a-sticker flow: pick a photo, run the native cutout, preview the
      transparent result (checkerboard), send; offered wherever images are (lobby,
      named groups, DMs); handle no-subject / unsupported-OS per the owner's answer;
      widget tests `app-ui/create-a-sticker`
- [x] 5.2 Render a received transparent image without an opaque bubble fill; widget
      tests `app-ui/create-a-sticker`

## 6. Docs and check

- [x] 6.1 `docs/architecture-decisions.md` (photo stickers: native-only cutout, the
      `SegmentationSource` seam and RGBA contract, styling/encode in shared Rust, the
      future server/ONNX extension points)
- [x] 6.2 `cargo test --workspace`, clippy, fmt, `buf lint`, `flutter analyze`,
      `flutter test`, `openspec validate photo-sticker --strict`
