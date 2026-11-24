## ADDED Requirements

### Requirement: Send a video
The composer SHALL let the user send a video by recording one (H.264/MP4, bounded to
30 seconds at capture) or picking one from the gallery, and SHALL show a 30-second
trim slider for an over-limit gallery clip rather than rejecting it. The app SHALL
pass the clip through the native normalize plugin and then to the core, and SHALL NOT
encrypt or upload it itself. While the blob uploads the app SHALL show the video as
sending. Video SHALL be offered in the lobby, named groups and DMs. The app enforces
going through the core.

#### Scenario: Record and send
- **WHEN** Bob records a clip and taps send
- **THEN** the app captures H.264/MP4 ≤ 30 s, normalizes it, hands it to the core, and
  shows it as sending until accepted

#### Scenario: Pick, trim, send
- **WHEN** Alice picks a 90-second gallery clip
- **THEN** the app shows a 30-second trim slider and sends the trimmed, normalized clip

### Requirement: Video bubble and player
A video message SHALL show as a bubble at the clip's aspect ratio
(`width`/`height`), first the first-frame ThumbHash blur with a play overlay and the
duration, then the clip once downloaded. The clip SHALL download on tap (no
auto-download), and playback SHALL open a full-screen player; one video SHALL play at
a time. When the core reports the video expired the bubble SHALL show "Video expired"
and download nothing. A message this device sent SHALL offer resend in its long-press
menu. The app enforces this.

#### Scenario: Blur then play
- **WHEN** Bob receives a video
- **THEN** the bubble shows the ThumbHash blur at the right aspect ratio with a play
  overlay and the duration; tapping it downloads and plays the clip full-screen

#### Scenario: Expired video
- **WHEN** Bob opens a video whose blob the server has deleted
- **THEN** the bubble shows "Video expired" and downloads nothing
