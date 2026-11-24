## MODIFIED Requirements

### Requirement: Inner content format
The plaintext of every MLS application message SHALL be a serialized
`vyajena.v1.Content` message: a client send time and exactly one of `Text`,
`GroupSettings`, `Profile`, `Image`, `Audio` (change `audio-messages`), `Delete`
(change `delete-messages`) or `CallSignal`. Only the Rust core SHALL serialize or
parse it; the server SHALL never see it. The `Content` oneof field numbers SHALL be
coordinated across these pending changes so no two bodies share a number.

#### Scenario: Text message
- **WHEN** a user sends "hello"
- **THEN** the Rust core encrypts a `Content` with a `Text` body of "hello" and
  its send time

#### Scenario: Call signal
- **WHEN** a user starts, answers, declines or ends a call
- **THEN** the Rust core encrypts a `Content` with a `CallSignal` body carrying the
  matching `CallOffer`, `CallAnswer`, `IceCandidate`, `CallHangup` or `CallReject`

#### Scenario: Unknown body type
- **WHEN** a client decrypts a `Content` with a body type it does not know
- **THEN** it shows a placeholder saying the message needs a newer app version,
  and does not fail
