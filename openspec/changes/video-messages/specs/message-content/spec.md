## MODIFIED Requirements

### Requirement: Inner content format
The plaintext of every MLS application message SHALL be a serialized
`vyajena.v1.Content` message: a client send time and exactly one of `Text`,
`GroupSettings`, `Profile`, `Image`, `Audio` (change `audio-messages`) or `Video`.
Only the Rust core SHALL serialize or parse it; the server SHALL never see it. The
`Content` oneof field numbers SHALL be coordinated across the pending media and
delete changes so no two bodies share a number.

#### Scenario: Text message
- **WHEN** a user sends "hello"
- **THEN** the Rust core encrypts a `Content` with a `Text` body of "hello" and
  its send time

#### Scenario: Video message
- **WHEN** a user sends a short video
- **THEN** the Rust core encrypts a `Content` with a `Video` body carrying the blob
  id, the blob key, the size, the duration, the dimensions and a first-frame
  ThumbHash

#### Scenario: Unknown body type
- **WHEN** a client decrypts a `Content` with a body type it does not know
- **THEN** it shows a placeholder saying the message needs a newer app version,
  and does not fail
