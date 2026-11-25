## MODIFIED Requirements

### Requirement: Image encode pipeline
The core SHALL own every image transform: given a picked image's raw bytes it
SHALL decode it, apply its EXIF orientation and then drop all metadata (EXIF,
GPS included), downscale so the long edge is at most
`protocol::blob_store::MAX_IMAGE_EDGE` (2048) without upscaling, and encode WebP
at `protocol::blob_store::WEBP_QUALITY` (~80) within
`protocol::blob_store::MAX_IMAGE_BYTES` (2 MB), lowering quality and then size
before refusing. WebP SHALL be the only output. An animated input SHALL be
reduced to its first frame; an input the core cannot decode SHALL be refused with
a clear error. The core SHALL also accept a straight-alpha RGBA subject cutout from
a `SegmentationSource` (photo-sticker: Segmentation seam and RGBA contract) and
encode it as WebP **preserving its alpha channel** (a transparent background), never
compositing it onto an opaque fill, within the same size cap; for an opaque photo
the output stays opaque as before. The core SHALL compute the ThumbHash and record
the WebP `width`/`height`. Dart SHALL never transform image pixels for the wire. The
core enforces this.

#### Scenario: Photo is transcoded
- **WHEN** Alice picks a 4000×3000 JPEG with GPS EXIF
- **THEN** the core sends a WebP at most 2048 px on the long edge, at quality
  ~80, under 2 MB, with no EXIF, shown upright

#### Scenario: Undecodable input
- **WHEN** Alice picks a file the core cannot decode as an image
- **THEN** the core refuses it with an error and sends nothing

#### Scenario: Cutout keeps its transparency
- **WHEN** the core encodes an RGBA subject cutout into a sticker
- **THEN** the output WebP keeps the alpha channel so the background is transparent,
  within the same 2 MB cap, with its ThumbHash computed

## ADDED Requirements

### Requirement: Sticker styling and encode
Sticker styling and encoding SHALL live in the shared Rust core, not in the native
platforms: the native `SegmentationSource` SHALL only produce the RGBA cutout, and
the core SHALL do the consistent work — optionally trim fully-transparent margins,
downscale the long edge to at most `protocol::blob_store::MAX_IMAGE_EDGE`, and encode
WebP-with-alpha within `protocol::blob_store::MAX_IMAGE_BYTES` — so every sticker is
encoded identically regardless of which engine made the mask. The native side SHALL
NOT encode the sticker, and the per-blob key SHALL never leave the core. For the MVP
the core SHALL apply no styling beyond trimming and encoding (richer styling is
future work). The client enforces this.

#### Scenario: Same encode for every engine
- **WHEN** a cutout arrives from `IosVision`, `AndroidMlKit` or any future
  `SegmentationSource`
- **THEN** the core trims, downscales and encodes WebP-with-alpha the same way, and
  the native side encodes nothing

#### Scenario: Key stays in the core
- **WHEN** a sticker is encoded and sent
- **THEN** the core holds the blob key and Dart and the native plugins never see it
