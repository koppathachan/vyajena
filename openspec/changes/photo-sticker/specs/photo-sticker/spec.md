## ADDED Requirements

### Requirement: Sticker is an image
A sticker SHALL be sent as an ordinary `Image` body (message-content: Image
messages): a WebP that keeps its alpha channel so the background is transparent,
carrying the same `blob_id`, `blob_key`, `width`/`height`, `thumbhash`, `blob_size`
and optional caption, and riding the existing image blob path (blob-store) unchanged
— upload-then-`APPLICATION` send order, media cache, ack-delete, 7-day expiry and
resend. There SHALL be no new `Content` body and no new RPC for a sticker. A sticker
SHALL be sendable wherever an image is: the lobby, named groups and DMs. A client
that does not know about stickers SHALL render a received sticker as an ordinary
(transparent) image, never as an error. The client enforces this.

#### Scenario: Alice sends a sticker
- **WHEN** Alice turns a photo into a sticker and sends it in the lobby
- **THEN** her core sends one `Content` whose `Image` body references a
  transparent-background WebP blob, with no new body type and no key outside the
  ciphertext

#### Scenario: Any client renders the sticker
- **WHEN** Bob, on a build with no sticker feature, receives Alice's sticker
- **THEN** he sees it as a transparent image through the normal image bubble and
  nothing fails

### Requirement: On-device native cutout
The subject SHALL be cut out of the picked photo entirely on the device using the
platform's built-in segmentation — iOS Vision (foreground/subject mask) and Android
ML Kit Subject Segmentation — with no server, no bundled or downloaded model, and no
added third-party dependency carrying a license or attribution obligation. The photo
SHALL NOT leave the device for segmentation in the MVP. When the platform API is
unavailable or finds no subject, the app SHALL fail cleanly and tell the user rather
than send a wrong result. The client enforces this.

#### Scenario: Cutout runs on device
- **WHEN** Alice picks a photo to make a sticker
- **THEN** the platform's on-device segmentation produces the subject cutout, the
  photo is not uploaded for segmentation, and no model was downloaded

#### Scenario: No subject found
- **WHEN** the segmentation engine finds no subject in Alice's photo
- **THEN** the app tells her no subject was found and sends nothing

### Requirement: Segmentation seam and RGBA contract
Every cutout SHALL be produced through a single `SegmentationSource` abstraction that
names its producer, with the MVP registering `IosVision` and `AndroidMlKit` and
reserving `Server` and `OnnxLocal` for later. A `SegmentationSource` SHALL return the
cutout as a straight-alpha, 8-bit-per-channel RGBA image in the sRGB color space with
explicit `width`/`height`, R,G,B,A byte order, row stride `width`×4 and orientation
already applied, in which background pixels are fully transparent and subject pixels
keep their original color. Nothing other than this RGBA image SHALL cross the seam:
no mask-only buffer and no encoded image. A new segmentation engine SHALL be addable
by registering a `SegmentationSource` that meets this contract, without changing the
encode step or the send path. The client enforces this.

#### Scenario: Both MVP sources meet one contract
- **WHEN** the cutout is produced by `IosVision` on iOS or by `AndroidMlKit` on
  Android
- **THEN** each hands the core the same straight-alpha sRGB RGBA image per the
  contract, and the core encodes and sends it identically

#### Scenario: A future engine slots in
- **WHEN** a later consented `Server` source or an on-device `OnnxLocal` model is
  added behind `SegmentationSource`
- **THEN** it returns the same RGBA image and reuses the unchanged encode and send
  path, with no new `Content` body
