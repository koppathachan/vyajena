## MODIFIED Requirements

### Requirement: Send an image
The composer SHALL let the user attach an image by picking one from the gallery or
capturing one with the in-app camera (in-app-camera: Embedded camera screen), and send
it with an optional caption, passing the picked or captured bytes and caption through
the core (UI goes through the core); the app SHALL NOT transcode, inspect or upload
image bytes itself. A captured photo SHALL use the same preview-then-send bar and
caption as a picked one. The caption field SHALL offer the same `@` mention picker as a
text message. While the blob uploads the app SHALL show the image as sending. The app
enforces going through the core.

#### Scenario: Pick and send
- **WHEN** Alice picks a photo, types the caption "@Bob look" and sends
- **THEN** the app hands the bytes and caption to the core and shows the image
  as sending until it is accepted

#### Scenario: Capture and send
- **WHEN** Alice captures a photo with the in-app camera, types a caption and sends
- **THEN** the app hands the captured bytes and caption to the core through the same
  preview-then-send path as a picked photo and never encodes or uploads the still itself

#### Scenario: Sent while offline
- **WHEN** Alice sends a picture while her phone has no connection
- **THEN** the picture appears in the chat as sending, like a text, and goes out
  when the phone is connected again

#### Scenario: App does no image work
- **WHEN** Alice sends an image
- **THEN** the app passes the raw bytes to the core and never encodes WebP,
  strips EXIF or uploads a blob itself

## ADDED Requirements

### Requirement: Send from the in-app camera
The composer's attachment sheet SHALL offer a Camera entry, alongside the gallery
Photo and Video entries, that opens the embedded in-app camera (in-app-camera: Embedded
camera screen) for capturing a photo or video; the gallery entries SHALL remain for
choosing existing media. A photo captured there SHALL flow into the send-an-image path
(Send an image) and a video captured there SHALL flow into the send-a-video path, so
capture adds a source and removes none. The app SHALL open the camera through
`CameraScope.of(context)` (in-app-camera: Camera source seam), never constructing the
camera directly, so widget tests run on a fake. The app enforces this.

#### Scenario: Camera entry opens the embedded camera
- **WHEN** Alice taps Camera in the attachment sheet
- **THEN** the embedded camera screen opens with a live preview, a shutter, a photo ↔
  video mode toggle and flip front/back, and the gallery Photo and Video entries are
  still offered for picking existing media

#### Scenario: Captured photo lands in the composer
- **WHEN** Alice captures a photo in the embedded camera
- **THEN** it appears in the same preview-then-send bar as a picked photo, ready for a
  caption and sending through the core
