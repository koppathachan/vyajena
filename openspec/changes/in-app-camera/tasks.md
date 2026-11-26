## 0. Before starting

- [ ] 0.1 `video-messages` merged (the native `VideoNormalize` + gallery-with-trim
      path and `NormalizedVideo` that captured video reuses)
- [ ] 0.2 The image path (`media-blobs` / image-attach #45 preview-then-send) merged
      (the path captured photos reuse)
- [ ] 0.3 Owner confirms the embedded-camera shape (live preview + shutter + photo/video
      toggle + flip): confirmed 2026-11-26
- [ ] 0.4 No proto, server or core change is expected; if a byte path forces a bridge
      touch, stop and revisit `design.md` D5 before adding a bridge call

## 1. Proto and shared rules

- [ ] 1.1 None. A captured photo is an existing `Image` body and a captured video an
      existing `Video` body; reuse `protocol::video::MAX_VIDEO_DURATION_MS` (the record
      timer cap) and the image size cap as-is. `buf lint`, `buf breaking`: nothing to
      change

## 2. Spike (at implementation time, real iOS + Android — not during planning)

- [ ] 2.1 S1: the `camera` plugin gives an embedded live preview with a working
      shutter, photo ↔ video mode, flip front/back and a 30 s-bounded recording on both
      iOS and Android, and a captured still's bytes feed the existing image entry point
      unchanged (design D5); record the result and the pinned `camera` version in
      `design.md` D6

## 3. Dependency and permissions

- [ ] 3.1 Add `camera: ^0.11.1` to `app/pubspec.yaml` (iOS + Android), commit the
      resolved `pubspec.lock`; note the exact version in `design.md` D6
- [ ] 3.2 iOS: add `NSCameraUsageDescription` to `app/ios/Runner/Info.plist`
      (`NSMicrophoneUsageDescription` already exists from `audio-messages`)
- [ ] 3.3 Android: add `<uses-permission android:name="android.permission.CAMERA"/>` to
      the release manifest (`RECORD_AUDIO` already exists)
- [ ] 3.4 **Update `app/store/release-check.sh` allow-lists so the release check passes
      (design D7; as PR #20 did for the mic string):** add `NSCameraUsageDescription` to
      the iOS usage-description list and `android.permission.CAMERA` to the Android
      permission list (both sorted), and update the `pass` messages; tests
      `// spec: in-app-camera/camera-permissions-and-gallery-fallback`

## 4. App — the camera source seam

- [ ] 4.1 `camera_source.dart`: `CameraSource` (`capturePhoto()` → bytes,
      `captureVideo()` → `NormalizedVideo`), `DeviceCameraSource` (the only file that
      imports `camera`; a captured clip reuses the existing native `VideoNormalize`),
      and `CameraScope` like `VideoScope`; tests
      `// spec: in-app-camera/camera-source-seam`
- [ ] 4.2 `FakeCameraSource` in `app/test/support/` returning an in-memory photo and a
      sample `NormalizedVideo` (reuse `FakeVideoSource`'s sample); records calls; tests
      `// spec: in-app-camera/camera-source-seam`

## 5. App — the embedded camera screen

- [ ] 5.1 `CameraScreen`: live `CameraPreview`, shutter, photo ↔ video mode toggle, flip
      front/back, and in video mode a record timer that stops at `videoMaxDuration`
      (30 s); full-screen route fitting the app shell; tap-to-focus and flash optional;
      tests `// spec: in-app-camera/embedded-camera-screen`
- [ ] 5.2 Permission requests (camera for both modes, microphone for video) and the
      gallery fallback with a clear message on denial; tests
      `// spec: in-app-camera/camera-permissions-and-gallery-fallback`

## 6. App — composer wiring

- [ ] 6.1 Add the Camera entry to the attachment sheet (alongside the gallery Photo and
      Video entries), routing a captured photo into the preview-then-send bar and a
      captured video into the send-a-video flow, through `CameraScope.of(context)`;
      widget tests on `FakeCameraSource`:
      `// spec: app-ui/send-from-the-in-app-camera`
- [ ] 6.2 A captured photo reuses the picked-image preview-then-send path unchanged;
      widget tests `// spec: app-ui/send-an-image`,
      `// spec: in-app-camera/captured-media-reuses-the-existing-paths`

## 7. Docs and check

- [ ] 7.1 `docs/architecture-decisions.md`: the in-app camera (embedded capture
      replacing the OS-camera hand-off; the `CameraSource` seam; app-only, no wire
      change; the release-check allow-list update)
- [ ] 7.2 `cd app && flutter analyze` (warnings = errors), `flutter test`,
      `app/store/release-check.sh` on a built bundle/archive, and
      `openspec validate in-app-camera --strict`
