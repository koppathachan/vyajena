## ADDED Requirements

### Requirement: Compose tray
The composer SHALL stage picked images and videos together in a compose tray — an
ordered list of thumbnails with a **+** to add more, one caption field, and one
Send — before anything is sent. Picking from the Photo or Video attachment entry
SHALL add the picked item(s) to the tray rather than send them, and the **+** SHALL
reopen the attachment sheet to add more. On Send the app SHALL send the staged
items as separate messages in tray order, each through the core's existing image or
video send path, and SHALL NOT group them into one wire message. The one caption
(with the same `@` mention picker as a text message) SHALL attach to the **last**
item in order; the earlier items SHALL send captionless. The tray SHALL replace the
single-image preview bar, so a single staged item is captioned and sent as before.
The sticker cutout flow SHALL stay its own single-send flow and SHALL NOT appear in
the tray. The app enforces this.

#### Scenario: Stage a mixed batch and send
- **WHEN** Alice adds two photos, taps **+** and adds a video, types the caption
  "@Bob trip" and taps Send
- **THEN** the app sends three separate messages in order — the two photos
  captionless, then the video carrying the caption with the `@Bob` mention

#### Scenario: Single item keeps today's flow
- **WHEN** Alice stages one photo, types a caption and taps Send
- **THEN** the app sends one image message with that caption, as it did before the
  tray

#### Scenario: Remove clears the tray
- **WHEN** Bob removes the only staged item
- **THEN** the tray is dismissed and nothing is sent

### Requirement: Video send progress
The composer SHALL show a processing state for a just-picked video while the native
plugin normalizes it, and SHALL disable Send until every staged video is ready, so
the user sees the roughly five-second normalize running rather than an
apparently-dead UI. Once the staged batch is sent, each message SHALL show the
normal media sending state until it is accepted. The app enforces this.

#### Scenario: Processing shown while normalizing
- **WHEN** Alice adds a gallery clip that takes several seconds to normalize
- **THEN** the tray shows "processing video…" for it and Send is disabled until it
  is ready

#### Scenario: Send enabled when ready
- **WHEN** the clip finishes normalizing
- **THEN** its thumbnail appears in the tray and Send is enabled

### Requirement: Video caption shown in the bubble
A video message with a caption SHALL show the caption, with its mentions
highlighted, beneath the video bubble, exactly as an image caption shows beneath an
image bubble (Image bubble and viewer). A captionless video SHALL show no caption
line. The app enforces this.

#### Scenario: Caption under the video
- **WHEN** Bob receives a video captioned "@Bob look" mentioning Bob
- **THEN** the caption shows under the video bubble with "@Bobby" highlighted on
  Bob's device

#### Scenario: No caption line when captionless
- **WHEN** Bob receives a video with no caption
- **THEN** the bubble shows the video with no caption line
