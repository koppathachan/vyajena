## ADDED Requirements

### Requirement: Embedded camera screen
The app SHALL provide an embedded in-app camera screen with a live preview (the
`camera` plugin's `CameraController`/`CameraPreview`), a shutter, a photo ↔ video
mode toggle, and a flip front/back control, opened from the composer's attachment
sheet (app-ui: Send from the in-app camera). In video mode the screen SHALL show a
record timer that counts up and SHALL stop the recording at `videoMaxDuration`
(`protocol::video::MAX_VIDEO_DURATION_MS`, 30 seconds), so a captured clip never
exceeds the cap. Tapping the shutter in photo mode SHALL capture a still and in
video mode SHALL start, then stop, a recording. Tap-to-focus and a flash toggle are
optional and their absence is not a failure. The screen SHALL present as a
full-screen route that fits the app shell. The client enforces the duration cap at
capture.

#### Scenario: Capture a photo
- **WHEN** Alice opens the in-app camera, stays in photo mode and taps the shutter
- **THEN** the screen captures a still and returns its JPEG bytes to the composer

#### Scenario: Record a video to the cap
- **WHEN** Bob switches the camera to video mode and records past 30 seconds
- **THEN** the record timer stops the recording at `videoMaxDuration` and returns a
  clip no longer than 30 seconds

#### Scenario: Flip the lens
- **WHEN** Alice taps flip front/back
- **THEN** the live preview switches between the back and front camera and a
  following capture uses the selected lens

### Requirement: Camera source seam
Every in-app capture SHALL go through a single `CameraSource` abstraction offered by
a `CameraScope` inherited widget, mirroring the existing `VideoScope`/`VideoSource`
and `PicturePickerScope`/`PicturePicker` seams: `CameraSource.capturePhoto()` SHALL
return the captured JPEG bytes (or null if the user backs out) and
`CameraSource.captureVideo()` SHALL return a normalized clip ready for the core
(reusing `video-messages`' `NormalizedVideo`, or null). `DeviceCameraSource` SHALL be
the only file that imports the `camera` plugin, and the composer SHALL read its source
from `CameraScope.of(context)`, never constructing the camera directly. A widget test
SHALL be able to inject a `FakeCameraSource` through `CameraScope` so no test opens a
real camera; the capture flow SHALL NOT bypass this seam. The client enforces this.

#### Scenario: A widget test runs on the fake
- **WHEN** a widget test wraps the chat in a `CameraScope` holding a `FakeCameraSource`
- **THEN** opening the camera and capturing exercises the send flow without touching
  the real camera, gallery or native plugin

#### Scenario: The composer uses the scoped source
- **WHEN** the composer handles the attachment sheet's Camera entry
- **THEN** it captures through `CameraScope.of(context)` and never imports or
  constructs the `camera` plugin itself

### Requirement: Captured media reuses the existing paths
A captured photo and a captured video SHALL reuse the existing downstreams unchanged;
the camera SHALL only produce the source media. A captured photo's bytes SHALL enter
the same picked-image preview-then-send path (app-ui: Send an image) and go to the
core, which owns the WebP encode (client-runtime: Image encode pipeline); the app
SHALL NOT transcode, encrypt or upload the still itself. A captured video's clip SHALL
go through the existing native video normalizer (video-messages: Video normalization
in a native plugin) to the canonical H.264/AAC MP4 with a first-frame ThumbHash, then
the existing `Video` send path. A captured photo SHALL be sent as an ordinary `Image`
body and a captured video as an ordinary `Video` body, with no new `Content` body and
no proto change. The client enforces going through the core.

#### Scenario: Captured photo is an ordinary image
- **WHEN** Alice captures a photo and sends it
- **THEN** the app hands the still's bytes to the core through the picked-image path,
  the core encodes the WebP, and one `Content` with an `Image` body is sent

#### Scenario: Captured video is normalized like a picked clip
- **WHEN** Bob captures a video and sends it
- **THEN** the clip passes through the native `VideoNormalize` to the 480p/~1 Mbps
  H.264/MP4 with a first-frame ThumbHash and is sent as a `Video` body, the same as a
  gallery clip

#### Scenario: The camera adds no wire body
- **WHEN** a captured photo or video is sent
- **THEN** no new `Content` body or RPC is used and a client without this feature
  renders the result as a normal image or video

### Requirement: Camera permissions and gallery fallback
The in-app camera SHALL request the camera permission for both photo and video and the
microphone permission for video recording, and SHALL declare them for the store: iOS
`Info.plist` SHALL carry `NSCameraUsageDescription` (the microphone string already
exists) and the Android release manifest SHALL declare `android.permission.CAMERA`
(`RECORD_AUDIO` already exists). When the user denies the camera permission, or the
microphone permission in video mode, the app SHALL NOT open to a dead preview: it SHALL
show a clear message and fall back to the gallery picker so existing media can still be
attached. The store-release check's allow-lists SHALL be updated so the camera usage
string and `CAMERA` permission pass (app-release: Declared permissions, Release check),
as PR #20 did for the microphone string. The client enforces the fallback; the release
check enforces the declarations.

#### Scenario: Camera denied falls back to the gallery
- **WHEN** Alice opens the in-app camera and denies the camera permission
- **THEN** the app tells her to allow the camera in Settings and opens the gallery
  picker so she can still attach a photo

#### Scenario: Release check passes the camera string
- **WHEN** the release check reads the built `Info.plist` and merged
  `AndroidManifest.xml` after the camera declarations are added
- **THEN** `NSCameraUsageDescription` and `android.permission.CAMERA` are on the
  allow-lists and the release check passes
