## Why

Today a photo is only ever **picked** from the gallery (`picture_picker.dart`,
`ImageSource.gallery`) and a video is **recorded through the OS camera** by handing
off to `image_picker`'s `ImageSource.camera` (`video_plugins.dart`). There is no
in-app capture: the user leaves Vyajena for the system camera UI and comes back with
a file. The owner asked (2026-11-26) for capture **from within the app, not just a
selector**, and chose a **full embedded in-app camera** — a live preview with a
shutter — over a quick OS-camera "Take photo" hand-off. This change authors the spec
for that camera. Planning only — no code, no deploy.

## What Changes

- **A new embedded camera screen** — a live preview (the `camera` Flutter plugin's
  `CameraController`), a **shutter**, a **photo ↔ video mode toggle**, **flip
  front/back**, and, in video mode, a **record timer** that stops at the existing
  `videoMaxDuration` cap (`protocol::video::MAX_VIDEO_DURATION_MS`, 30 s). Tap-to-
  focus and a flash toggle are **nice-to-haves**, not required for the MVP. The
  screen fits the existing app shell (a full-screen route opened from the composer's
  attachment sheet, `app-shell`: Conversation screen).
- **Capture replaces the OS-camera hand-off for both media types; the gallery
  selector stays.** The attachment sheet keeps **Photo** (gallery) and **Video**
  (gallery) for choosing existing media and gains a **Camera** entry that opens the
  embedded camera. A captured photo or video flows into the **same downstream** the
  pickers already feed; nothing about picking is removed.
- **A clean capture seam — `CameraSource` behind a `CameraScope`** (owner, the
  "clean seam" ask), mirroring the existing `VideoScope`/`VideoSource` and
  `PicturePickerScope`/`PicturePicker` abstractions. The one file that imports the
  `camera` plugin exposes a `CameraSource`; a widget test injects a **`FakeCameraSource`**
  (`app/test/support/`) so a test never opens a real camera. The camera flow does
  **not** bypass the test seam.
- **The camera only produces the source bytes/file; the existing paths do the rest
  (no duplication):**
  - A captured **photo** hands its JPEG bytes to the **existing image path** — the
    preview-then-send bar and caption from the image-attach / #45 flow, then the
    core's WebP encode (`client-runtime`: Image encode pipeline). The camera does
    not transcode, encrypt or upload.
  - A captured **video** hands its clip to the **existing native `VideoNormalize`**
    (`video-messages`: H.264/AAC MP4, 480p/~1 Mbps, first-frame ThumbHash) and the
    same gallery-with-trim preview. The camera caps the recording at 30 s at capture;
    an over-cap clip still meets the trim path.
- **Permissions (owner, 2026-11-26):** the embedded camera needs the **camera**
  permission for both photo and video, and the **microphone** permission for video
  only (the mic string already exists from `audio-messages`). iOS `Info.plist` gains
  **`NSCameraUsageDescription`**; the Android release manifest gains
  **`android.permission.CAMERA`**. When the user denies a permission the app
  **falls back to the gallery picker with a clear message** rather than a dead
  screen.
- **The store-release guard is updated so the camera strings PASS**
  (`app/store/release-check.sh`): the iOS usage-description allow-list adds
  `NSCameraUsageDescription` and the Android permission allow-list adds
  `android.permission.CAMERA`, exactly as PR #20 did for the microphone string — so
  this change does not re-break the release check (`app-release`: Declared
  permissions, Release check).
- **One new Flutter dependency:** the **`camera`** plugin, pinned to a version that
  supports iOS and Android (`design.md` D6). App-only — **no proto, server or Rust
  change** is intended; `design.md` flags the one place a photo byte path *might*
  touch the bridge and why it should not.
- **Out of scope:** any new `Content` body or wire change (a captured photo is an
  `Image`, a captured video is a `Video` — both already exist); in-camera filters,
  zoom, grid, timer-countdown or burst; QR/barcode scanning; simultaneous
  photo+video; a camera outside the chat composer. All noted in `design.md`.
- Details and decisions: `design.md`.

## Open decisions resolved (owner, 2026-11-26)

- **Capture surface:** a full embedded in-app camera (live preview + shutter), not a
  quick OS-camera "Take photo" hand-off.
- **Scope:** capture replaces the OS-camera hand-off for **both** photo and video;
  the gallery selector stays for picking existing media.
- **Seam:** a `CameraSource`/`CameraScope` abstraction with a `FakeCameraSource` for
  widget tests, like the existing picker/video seams.
- **Downstream:** a captured photo reuses the image encode + preview-then-send path;
  a captured video reuses the native `VideoNormalize` + gallery-with-trim path. The
  camera only produces the source media.

## Proto files touched

- **None.** A captured photo is an existing `Image` body; a captured video is an
  existing `Video` body (`video-messages`). No new `Content` field, no new RPC, so
  `buf breaking` has nothing to check.
- `protocol/`: reuse `MAX_VIDEO_DURATION_MS` (the record-timer cap) and the image
  size cap as-is; add no wire constants.

## Capabilities

### New Capabilities

- `in-app-camera`: the embedded camera screen (preview, shutter, mode toggle, flip,
  record timer), the `CameraSource` capture seam and its fake, the reuse of the
  image and video downstream for captured media, and the camera/microphone
  permissions with a gallery fallback on denial.

### Modified Capabilities

- `app-ui`: the composer offers in-app capture (a Camera entry in the attachment
  sheet opening the embedded camera) alongside the gallery pickers; a captured photo
  reuses the picked-image preview-then-send path.

## Impact

- **Server (`server/`):** none. A captured photo or video is a blob like any other.
- **Core (`client/`) and proto:** none intended. The core receives the captured
  photo's bytes (image path) or the normalized MP4 (video path) exactly as it does a
  picked one; keys never leave the core. `design.md` D5 flags the only byte-path
  risk and why it needs no bridge change.
- **Bridge and app:** a new `camera`-plugin-backed `CameraSource` behind
  `CameraScope`; a full-screen embedded camera screen (preview, shutter, photo/video
  toggle, flip, 30 s record timer, focus/flash nice-to-haves); the attachment-sheet
  Camera entry; the permission requests and the gallery fallback on denial; a
  `FakeCameraSource` in `app/test/support/`.
- **Config/ops:** `app/store/release-check.sh` allow-lists gain the camera usage
  string (iOS) and `CAMERA` permission (Android); iOS `Info.plist` and the Android
  manifest gain the camera declarations; `app/pubspec.yaml` gains the pinned
  `camera` dependency.
- **Depends on:** `video-messages` (the native `VideoNormalize` + gallery-with-trim
  path that captured video reuses) and the image path from `media-blobs` /
  image-attach (the preview-then-send flow captured photos reuse). It stacks after
  `video-messages`; it adds no `Content` field, so there is no field-number
  coordination.
