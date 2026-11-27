# Tasks

Proto first, then core, bridge, app, tests. Tick a task only when its tests pass.

## 1. Proto (wire contract)

- [x] 1.1 Add `optional string caption = 8` and `repeated Mention caption_mentions = 9`
  to `Video` in `proto/vyajena/v1/content.proto`, documented like `Image`'s.
  (spec: message-content/video-caption-and-mentions)
- [x] 1.2 `buf lint` and `buf breaking --against '.git#branch=main'` clean.
- [x] 1.3 `cargo run -p pbgen`; commit the regenerated `server/src/pb.rs`,
  `server/tests/support/pb.rs`, `client/src/pb.rs` with the proto change.

## 2. Core (client) — video caption

- [x] 2.1 Carry an optional caption + mentions on the video send path in `client/`,
  validating the caption length and mention spans like the image caption, and
  setting `Video.caption` / `Video.caption_mentions`.
  (spec: message-content/video-caption-and-mentions)
- [x] 2.2 Report a received video as mentioning a user from `caption_mentions`,
  and drop an invalid span while still showing the caption, mirroring the image
  caption path.
  (spec: message-content/video-caption-and-mentions)
- [x] 2.3 `client/tests/` cover a captioned video, a caption mention, and a bad
  span dropped (names the requirement).

## 3. Bridge (flutter_rust_bridge)

- [x] 3.1 Add `caption` + `mentions` params to `sendVideo` in `bridge/src/api/`,
  threading them to the core (the Dart video message model exposes the caption like
  the image one).
- [x] 3.2 `cd app && flutter_rust_bridge_codegen generate`; commit
  `bridge/src/frb_generated.rs` and `app/lib/src/rust/`.
- [x] 3.3 `cargo build --workspace`, `cargo clippy --workspace --all-targets -D warnings`,
  `cargo fmt --check` clean.

## 4. App — compose tray

- [x] 4.1 `CoreService.sendVideo` / `ChatModel.sendVideo` gain optional caption +
  mentions; pass through to the bridge.
- [x] 4.2 Generalize `composer.dart`'s `_picture` preview bar into a tray staging an
  ordered `List<StagedMedia>` (image bytes or `NormalizedVideo`), each a thumbnail,
  with a **+** that reopens the attachment sheet and one caption field.
  (spec: app-ui/compose-tray)
- [x] 4.3 Picking from Photo / Video adds to the tray instead of sending; the video
  Record/gallery sub-sheet and the photo camera/gallery chooser are unchanged;
  sticker stays its own flow.
  (spec: app-ui/compose-tray)
- [x] 4.4 Send dispatches the staged items as separate messages in order via
  `sendImage` / `sendVideo`, with the caption + mentions on the last item only.
  (spec: app-ui/compose-tray)
- [x] 4.5 While a just-added clip normalizes, show a "processing video…" item and
  disable Send until every staged video is ready; show "sending" on upload.
  (spec: app-ui/video-send-progress)

## 5. App — video caption in the bubble

- [x] 5.1 Render a video message's caption (with mentions highlighted) beneath the
  bubble, reusing the image-caption rendering; no caption line when captionless.
  (spec: app-ui/video-caption-shown-in-the-bubble)
- [x] 5.2 The reply/copy paths already read a video caption (`videoQuote` /
  `_copyText`); confirm a captioned video quotes and copies its caption.

## 6. Tests + validation

- [x] 6.1 Widget tests in `app/test/chat/` (fakes in `app/test/support/`,
  `fake_video.dart`): stage a mixed batch → three ordered sends, caption on the
  last; single item keeps today's flow; remove clears the tray; processing shown
  while normalizing with Send disabled; caption shown under a video bubble.
  Each tagged `// spec: app-ui/<slug>`.
- [x] 6.2 `cd app && flutter analyze && flutter test` clean.
- [x] 6.3 `openspec validate media-compose-tray --strict` passes.
- [ ] 6.4 Open a PR against `main` (concise ~50-word message); do not deploy.
