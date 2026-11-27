# audio-messages Specification

## Purpose
TBD - created by archiving change audio-messages. Update Purpose after archive.
## Requirements
### Requirement: Audio messages
An `Audio` body SHALL carry a 16-byte `blob_id`, a 256-bit `blob_key`, the
`blob_size`, the `duration_ms` (at most `protocol::audio::MAX_AUDIO_DURATION_MS`),
and a bounded `waveform` of amplitude peaks (at most
`protocol::audio::MAX_AUDIO_WAVEFORM_BYTES`). It SHALL reference exactly one Opus
mono clip in an OGG container stored as an encrypted blob (blob-store), and SHALL
carry no video, no album and no second track. The `blob_key` SHALL travel only
inside the encrypted `Content` and SHALL never appear in an `Envelope` or any RPC.
A client built before `Audio` SHALL show the unknown-body placeholder
(message-content: Inner content format). The client enforces this.

#### Scenario: Alice sends a voice message
- **WHEN** Alice records and sends a 20-second clip
- **THEN** her core encrypts one `Content` whose `Audio` body holds the blob id,
  the blob key, the size, `duration_ms` 20000 and the waveform, and no key appears
  outside the ciphertext

#### Scenario: Old client receives audio
- **WHEN** a client that predates `Audio` decrypts a voice message
- **THEN** it shows the "needs a newer app version" placeholder and does not fail

### Requirement: Recording and encoding
The core SHALL own the audio transform: given recorded audio it SHALL produce Opus
mono in an OGG container at the voice bitrate, SHALL refuse a clip longer than
`protocol::audio::MAX_AUDIO_DURATION_MS` or one whose encoded blob would exceed
`protocol::blob_store::MAX_BLOB_BYTES`, and SHALL compute the `waveform` peaks.
Dart SHALL never encode Opus or compute the waveform for the wire. The core
enforces this.

#### Scenario: Clip is encoded to Opus/OGG
- **WHEN** Alice records a clip
- **THEN** the core sends an OGG/Opus mono blob at the voice bitrate, with a
  computed waveform and `duration_ms` set

#### Scenario: Too long
- **WHEN** Alice records past `MAX_AUDIO_DURATION_MS`
- **THEN** the core refuses it with a clear error and sends nothing

### Requirement: Audio stays encrypted
An audio reference and its waveform SHALL travel only inside the encrypted
`Content`; the blob the server stores SHALL be ciphertext with no key. The server
SHALL send the same wake ping for a voice message as for any other, with the one
fixed text "New message". The client and the server enforce this.

#### Scenario: Voice message while Bob is offline
- **WHEN** Alice sends a voice message while Bob's phone has no open stream
- **THEN** the server queues one `APPLICATION` envelope and sends Bob's phone one
  wake ping with the text "New message", the same as for a plain message

