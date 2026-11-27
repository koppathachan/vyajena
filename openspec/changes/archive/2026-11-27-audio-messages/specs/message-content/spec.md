## MODIFIED Requirements

### Requirement: Inner content format
The plaintext of every MLS application message SHALL be a serialized
`vyajena.v1.Content` message: a client send time and exactly one of `Text`,
`GroupSettings`, `Profile`, `Image` or `Audio`. Only the Rust core SHALL serialize
or parse it; the server SHALL never see it.

#### Scenario: Text message
- **WHEN** a user sends "hello"
- **THEN** the Rust core encrypts a `Content` with a `Text` body of "hello" and
  its send time

#### Scenario: Image message
- **WHEN** a user sends a photo
- **THEN** the Rust core encrypts a `Content` with an `Image` body carrying the
  blob id, the blob key, the dimensions and the ThumbHash

#### Scenario: Audio message
- **WHEN** a user sends a voice message
- **THEN** the Rust core encrypts a `Content` with an `Audio` body carrying the
  blob id, the blob key, the size, the duration and the waveform

#### Scenario: Unknown body type
- **WHEN** a client decrypts a `Content` with a body type it does not know
- **THEN** it shows a placeholder saying the message needs a newer app version,
  and does not fail
