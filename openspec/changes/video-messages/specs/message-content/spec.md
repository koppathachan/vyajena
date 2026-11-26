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

### Requirement: Reply quote describes the original's kind
A `ReplyTo` SHALL carry the kind of the message it answers (`original_kind`: text,
image, audio, or **video**) and enough to preview any **media** original without
holding it: for a voice original its length (`original_duration_ms`), and for a
picture original its ThumbHash (`original_thumbhash`). A **video** original is a
known media kind: the core SHALL set `original_kind = ORIGINAL_KIND_VIDEO` with an
empty text `quote`, and a receiver SHALL render it as a video preview (a video glyph
and "Video") rather than treating the empty quote as an unknown body. A known media
kind SHALL never fall back to the "needs a newer app version" placeholder; that
placeholder is only for a body whose kind the client does not know. An older sender
leaves `ORIGINAL_KIND_UNSPECIFIED`, and a receiver that predates
`ORIGINAL_KIND_VIDEO` reads that unknown value as a text quote, exactly as it
already treats any kind it does not know. All of this is client-side content inside
the encrypted `Content`.

#### Scenario: Replying to a video
- **WHEN** Alice replies to Bob's video message (which is followed in the chat by a
  text message)
- **THEN** the `ReplyTo` carries `original_kind = ORIGINAL_KIND_VIDEO` with an empty
  text quote and names Bob's video by its `(sender, msg_id)`, and every recipient
  shows the reply quoting the video with a video glyph and "Video" — never the text
  that follows it and never the "needs a newer version" placeholder — whether or not
  it holds Bob's clip

#### Scenario: Video reply reaches an older app
- **WHEN** a reply with `original_kind = ORIGINAL_KIND_VIDEO` reaches a device whose
  app predates that value
- **THEN** it reads the kind as `ORIGINAL_KIND_UNSPECIFIED` and renders the empty
  quote as a text quote, without failing
