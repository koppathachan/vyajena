## Why

The owner has been demo-testing the media flow shipped in #74 and asked for four
improvements (owner, 2026-11-27):

1. **Video send has no visible feedback.** Picking a gallery video runs the native
   normalize (Android MediaCodec / iOS AVFoundation) for ~5 s or more with nothing
   on screen, then the message just appears — it feels like the tap did nothing.
   See `chat_screen.dart` `_sendVideo` and `video_plugins.dart`
   `DeviceVideoSource.normalize`.
2. **Video has no caption.** An image gets a preview-and-caption flow
   (`composer.dart`, `Image.caption` on the wire); a video sends with none. The
   owner wants parity.
3. **No mixed multi-attachment.** #74 added multi-select, but each picked item
   sends as its own message, images and videos go through two separate pickers,
   and a video still has no caption. The owner wants to stage several images *and*
   videos together.
4. **No compose tray.** The owner wants a staging area that shows the picked
   thumbnails (images + videos), a **+** to add more before sending, one caption
   field, and one **Send**.

Today the composer stages at most one picture (the `_picture` preview bar +
caption) or sends a multi-pick immediately, each item its own captionless message
(`_attachGallery` / `_sendPictures` / `_sendVideo`). There is no shared staging
area and no video caption.

This is overwhelmingly an app (`app/`) change built on the existing per-item send
paths (`CoreService.sendImage` / `sendVideo` / `sendSticker`), **plus one small,
backward-compatible wire addition**: a caption on `Video` (ask #2), mirroring the
caption already on `Image`. The E2EE boundary is unchanged — the core still owns
all blob crypto and the native plugin still owns video normalization; the app only
stages picked bytes and hands them to the core as it does now.

## Decisions (owner-approved 2026-11-27)

- **D1 — Separate messages, not grouped.** A staged batch sends as **separate
  messages, one per item, in order** (reusing `sendImage` / `sendVideo`). No
  grouped/album wire body — a single run of media bubbles, as the app already
  produces for a multi-pick today.
- **D2 — One caption for the batch.** The tray has one caption field; with
  separate messages it attaches to the **last** item (it reads as the caption
  under the final bubble of the set), the rest go captionless. A single staged item
  is captioned as today. A single staged **video** uses the new `Video.caption`.
- **D3 — Tray meets the existing UI.** The Photo / Sticker / Video attachment sheet
  stays as the entry; the tray's **+** reopens it; **Video** keeps its
  Record / gallery sub-sheet; **sticker stays its own single-send flow** (not in
  the batch); the tray **replaces** the one-off `_picture` preview bar.
- **D4 — Progress.** While a just-added clip normalizes, the tray shows a
  "processing video…" state and **Send is disabled** until it is ready; on Send the
  bubbles show the normal "sending" state while blobs upload.
- **gRPC streaming — not now** (owner agreed with the recommendation). Blobs move
  over unary `Blobs.UploadBlob` / `DownloadBlob`, each bounded (image ≤ 2 MB, video
  ≤ 8 MB), and a multi-send is just independent unary transfers; streaming would
  only earn its place to raise the per-blob ceiling, add chunk-level progress, or
  cut peak memory, as its own later change. Not started here.

## What Changes

- **Video send progress (ask #1).** While a just-added clip normalizes, and while a
  staged batch uploads, the app SHALL show progress instead of an apparently-dead
  UI. No wire or core change.
- **Video caption (ask #2).** `Video` gains an optional `caption` and
  `caption_mentions`, mirroring `Image` (next free `Video` fields 8–9; older
  clients ignore the unknown fields and still play the clip). `CoreService.sendVideo`
  / `ChatModel.sendVideo` gain an optional caption + mentions; the video bubble
  renders the caption with its mentions exactly as an image caption does.
  - **Proto-first:** `proto/vyajena/v1/content.proto` changes first, then
    `cargo run -p pbgen`, then the core/bridge, then
    `flutter_rust_bridge_codegen generate`.
- **Compose tray (asks #3, #4).** The single-image preview bar is generalized into
  a tray that stages an ordered list of images **and** videos, each a thumbnail,
  with a **+** to add more and one caption field, then one **Send**. Picking from
  the Photo or Video attachment entry now lands items in the tray instead of
  sending immediately; Send dispatches them as separate messages in order with the
  caption on the last.

## Non-goals

- **No grouped/album wire message** (per D1): items send as separate messages.
- **No per-item captions** (per D2): one caption for the batch.
- **No sticker in the batch** (per D3): the cutout flow is unchanged.
- **No media forward** — forwarding an attachment stays the follow-up already noted
  by `message-actions-share`.
- **No gRPC streaming** — a separate change if ever pursued.
- **No change to video normalization, image encode, or any blob crypto.** The core
  and native plugins are untouched except for threading the new video caption
  through `sendVideo`.
- **No new media kind** (files, documents) — images and videos only.

## Coordination / dependencies

- **Spec archive ordering.** The base `app-ui` / `message-content` specs do not yet
  contain the video requirements — they live in the still-active `video-messages`
  (and multi-pick in `photo-sticker`) changes, which `openspec-archive` (PR #78) is
  folding into the base. This change's deltas are written to **ADD** new
  requirements (video caption; compose tray; video send progress) and only lightly
  modify base-present ones, so they do not depend on the video requirements being
  in the base; archive ordering is coordinated with the tracker (`local_38cf5750`).
- **`Video` field numbers.** `caption` = 8, `caption_mentions` = 9 (the next free
  `Video` fields). Flagged to the tracker in case it is managing proto allocation.

## Impact

- **Proto:** `proto/vyajena/v1/content.proto` — add `Video.caption` (8) and
  `Video.caption_mentions` (9); regenerate `pb.rs` via `cargo run -p pbgen`.
- **Core / bridge:** `client/` video send carries the caption + mentions;
  `bridge/src/api/` `sendVideo` gains the caption params; regenerate Dart bindings
  via `flutter_rust_bridge_codegen generate`.
- **App:** `composer.dart` (the tray, the + flow, one caption field, progress
  state, the separate-message send with the caption on the last item),
  `chat_screen.dart` (`_sendVideo` staging), `video_plugins.dart` /
  `picture_picker.dart` (expose staged items without auto-sending), the video
  bubble (render the caption). Widget tests in `app/test/chat/` with the existing
  fakes in `app/test/support/`.
- **Specs:** `app-ui` (compose tray, mixed multi-attachment, video send progress),
  `message-content` (`Video.caption` and mentions).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
