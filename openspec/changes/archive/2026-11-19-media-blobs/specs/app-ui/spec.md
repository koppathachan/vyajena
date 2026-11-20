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

#### Scenario: Sent while offline
- **WHEN** Alice sends a picture while her phone has no connection
- **THEN** the picture appears in the chat as sending, like a text, and goes out
  when the phone is connected again

#### Scenario: App does no image work
- **WHEN** Alice sends an image
- **THEN** the app passes the raw bytes to the core and never encodes WebP,
  strips EXIF or uploads a blob itself

### Requirement: Image bubble and viewer
An image message SHALL show as a bubble at the image's aspect ratio
(`width`/`height`), first the ThumbHash blur (decoded by the core), then the full
image once downloaded; the caption, with its mentions highlighted, SHALL show
under it. The full image SHALL download on tap (no thumbnail, no auto-download),
and a tap on the loaded image SHALL open a full-screen viewer. When the core
reports the image expired the bubble SHALL show the blur with "Media expired —
resend" and SHALL download nothing. A message this device sent SHALL offer
"Resend picture" in its long-press menu, which sends the picture again as a new
message. A download that fails without being expired SHALL say so and let the
user tap to try again. The app enforces this.

#### Scenario: Blur then full image
- **WHEN** Bob receives an image
- **THEN** the bubble shows the ThumbHash blur at the right aspect ratio and the
  size to download, tapping it downloads and shows the full image, and tapping
  that opens the viewer

#### Scenario: Expired image
- **WHEN** Bob opens an image the server has deleted
- **THEN** the bubble shows the blur with "Media expired — resend"

#### Scenario: Alice resends
- **WHEN** Bob tells Alice her picture expired and she long-presses it and picks
  "Resend picture"
- **THEN** the core sends the picture again as a new message, and the chat shows
  both; a message from another user has no such entry

#### Scenario: Failed download can be retried
- **WHEN** a download fails because the phone is offline
- **THEN** the bubble says it could not load the picture, and a tap tries again

#### Scenario: Caption with a mention
- **WHEN** an image arrives captioned "@Bob look" mentioning Bob
- **THEN** the caption shows under the image with "@Bobby" highlighted on Bob's
  device
