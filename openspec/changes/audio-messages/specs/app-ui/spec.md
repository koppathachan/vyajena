## ADDED Requirements

### Requirement: Record a voice message
The composer SHALL record a voice message by hold-to-record: the user presses and
holds the mic, releases to stop, and slides to cancel while holding. On release the
app SHALL show a preview with play, delete and send, and SHALL pass the recorded
audio to the core, which encodes and sends it (audio-messages: Recording and
encoding); the app SHALL NOT encode, transcode or upload audio itself. While the
blob uploads the app SHALL show the clip as sending. The app enforces going through
the core.

#### Scenario: Hold, release, send
- **WHEN** Alice holds the mic, speaks, releases, and taps send on the preview
- **THEN** the app hands the recorded audio to the core and shows the clip as
  sending until it is accepted

#### Scenario: Slide to cancel
- **WHEN** Alice holds the mic and slides to cancel before releasing
- **THEN** nothing is recorded into the chat and nothing is sent

#### Scenario: App does no audio work
- **WHEN** Alice sends a voice message
- **THEN** the app passes the recorded audio to the core and never encodes Opus or
  uploads a blob itself

### Requirement: Play a voice message
A voice message SHALL show as a bubble with a play/pause control, the waveform from
the body and the duration. The clip SHALL download on first tap (no auto-download),
then play; starting another clip SHALL stop the one playing. When the core reports
the audio expired the bubble SHALL show "Audio expired" and download nothing. A
message this device sent SHALL offer resend in its long-press menu, which sends the
clip again as a new message. The app enforces this.

#### Scenario: Download then play
- **WHEN** Bob taps a voice message for the first time
- **THEN** the core downloads and decrypts the clip and it plays, with the waveform
  filling as it plays

#### Scenario: One at a time
- **WHEN** a clip is playing and Bob taps another
- **THEN** the first stops and the second plays

#### Scenario: Expired audio
- **WHEN** Bob opens a voice message the server has deleted
- **THEN** the bubble shows "Audio expired" and downloads nothing
