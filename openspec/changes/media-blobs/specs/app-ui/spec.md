## ADDED Requirements

### Requirement: Send an image
The composer SHALL let the user pick an image and send it with an optional
caption, passing the picked bytes and caption through the core
(UI goes through the core); the app SHALL NOT transcode, inspect or upload image
bytes itself. The caption field SHALL offer the same `@` mention picker as a text
message. While the blob uploads the app SHALL show the image as sending. The app
enforces going through the core.

#### Scenario: Pick and send
- **WHEN** Alice picks a photo, types the caption "@Bob look" and sends
- **THEN** the app hands the bytes and caption to the core and shows the image
  as sending until it is accepted

#### Scenario: App does no image work
- **WHEN** Alice sends an image
- **THEN** the app passes the raw bytes to the core and never encodes WebP,
  strips EXIF or uploads a blob itself

### Requirement: Image bubble and viewer
An image message SHALL show as a bubble at the image's aspect ratio
(`width`/`height`), first the ThumbHash blur, then the full image once
downloaded; the caption, with its mentions highlighted, SHALL show under it. The
full image SHALL download on tap (no thumbnail, no auto-download). A tap on the
loaded image SHALL open a full-screen viewer. When the core reports the image
expired the bubble SHALL show the blur with a "media expired — resend" label,
and the sender's own expired image SHALL offer resend. A download that fails
without being expired SHALL offer retry. The app enforces this.

#### Scenario: Blur then full image
- **WHEN** Bob receives an image
- **THEN** the bubble shows the ThumbHash blur at the right aspect ratio, and
  tapping it downloads and shows the full image

#### Scenario: Expired image
- **WHEN** Bob opens an image the Pi has deleted
- **THEN** the bubble shows the blur with "media expired — resend"

#### Scenario: Caption with a mention
- **WHEN** an image arrives captioned "@Bob look" mentioning Bob
- **THEN** the caption shows under the image with "@Bob" highlighted on Bob's
  device
