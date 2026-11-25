## Why

Users want to turn a photo into a **sticker**: pick a picture, cut the subject out
of its background, and send the cutout — a transparent-background image — into a
chat. The owner locked the MVP shape on 2026-11-24: do the cutout **on device with
the platform's own segmentation APIs**, send the result down the **existing image
path** (a sticker is just a transparent image), and leave a clean seam so a better
cutout engine can be added later without a rewrite. Planning only — no code, no
deploy.

## What Changes

- **A sticker is an ordinary `Image`, so the wire does not change.** The cutout is
  encoded as a WebP that keeps its alpha channel (transparent background) and rides
  the existing `Image` body and blob path unchanged — upload-then-`APPLICATION`
  send order, media cache, ack-delete, 7-day expiry, resend. **No new `Content`
  body, no new RPC, no proto change.** Old clients just see an image.
- **On-device native segmentation only (locked, owner 2026-11-24):** the subject is
  cut out using the platform's built-in APIs — **iOS Vision** (foreground-instance /
  subject mask) and **Android ML Kit Subject Segmentation**. **No server component,
  no bundled or downloaded model, no extra third-party dependency with a license or
  attribution burden.**
- **A pluggable cutout seam — `SegmentationSource` (locked, owner 2026-11-24):** a
  single abstraction (an enum/trait) names who produced the cutout and pins a clear
  **RGBA output contract**: a segmentation source takes the picked photo and returns
  a straight-alpha 8-bit **RGBA** image (sRGB, known `width`/`height`) in which
  background pixels are fully transparent. The MVP registers two sources (iOS
  Vision, Android ML Kit). A future **consented server path** or an **on-device
  ONNX/BiRefNet model** slots in behind the same contract without touching the send
  path or the encode step.
- **Styling and encoding live in shared Rust (locked, owner 2026-11-24):** the
  native platforms only produce the mask/RGBA; the **core** (shared Rust) does the
  consistent work — trim/clean the RGBA, preserve its alpha, downscale and encode
  WebP-with-alpha within the existing image size cap, and compute the ThumbHash —
  then runs the unchanged image blob crypto and send order. Natives never encode or
  touch keys.
- **Where (owner, 2026-11-24): wherever images are** — the lobby, named groups and
  DMs.
- **Out of scope for MVP:** any server-side segmentation; any downloaded/bundled
  model (ONNX/BiRefNet); outline, border, drop-shadow or other styling beyond what
  is trivial in the shared Rust encode step; animated or multi-subject stickers; a
  sticker gallery/reuse library; a wire hint that lets the receiver render a sticker
  differently from a normal transparent image. All noted as future work in
  `design.md`.
- Details and decisions: `design.md`.

## Open decisions resolved (owner, 2026-11-24)

- **Cutout engine:** on-device native only (iOS Vision, Android ML Kit); no server,
  no model, no attribution-burdened dependency.
- **Transport:** a sticker is a transparent-background WebP sent as an ordinary
  `Image`; no proto change.
- **Seam:** a `SegmentationSource` abstraction with an RGBA output contract; styling
  and encoding stay in shared Rust so a future engine slots in behind it.
- **Where allowed:** the lobby, named groups and DMs, like images.

## Proto files touched

- **None.** A sticker reuses the existing `Image` body (WebP already carries an
  alpha channel) and the existing `Blobs` RPCs. `buf breaking` has nothing to check.
- `protocol/`: reuse `MAX_IMAGE_EDGE`, `MAX_IMAGE_BYTES`, `WEBP_QUALITY`,
  `MAX_THUMBHASH_BYTES`, `BLOB_*` as-is; add only the segmentation/encode constants
  the design names (e.g. the RGBA contract's pixel format), no wire constants.

## Capabilities

### New Capabilities

- `photo-sticker`: on-device subject cutout from a picked photo, the
  `SegmentationSource` seam and its RGBA contract, and sending the cutout as a
  transparent image.

### Modified Capabilities

- `client-runtime`: the image encode pipeline also accepts an RGBA subject cutout
  and encodes WebP preserving alpha; the `SegmentationSource` seam and the
  shared-Rust style/encode split are added.
- `app-ui`: create a sticker from a photo (pick → on-device cutout → preview → send)
  wherever images are offered.

## Impact

- **Server (`server/`):** none. A sticker is a blob like any image; the blob store,
  size cap and expiry are unchanged.
- **Core (`client/`):** accept an RGBA cutout from a `SegmentationSource`, preserve
  its alpha, encode WebP-with-alpha within the existing cap, compute the ThumbHash,
  and reuse the image blob crypto, send order, cache, ack-delete, expiry and resend
  unchanged. Keys never leave the core.
- **Bridge and app:** a native segmentation plugin per platform (iOS Vision,
  Android ML Kit) that returns the RGBA cutout across the bridge; a "make a sticker"
  flow (pick a photo, run the cutout, preview the transparent result, send) offered
  wherever images are.
- **Config/ops:** none.
- **Depends on:** the image/blob path from `media-blobs` (reused unchanged).
  Independent of the other pending changes — it adds no `Content` field, so there is
  no field-number coordination.
