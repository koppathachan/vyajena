## ADDED Requirements

### Requirement: Video caption and mentions
A `Video.caption` SHALL be optional UTF-8 limited like a text message, and its
`caption_mentions` SHALL follow the same `Mention` rules as `Text.mentions`
(Mentions): each a user id or `everyone` over a valid, non-overlapping UTF-8 byte
range of the caption. The sending client SHALL refuse an over-limit caption or an
invalid span; a receiving client SHALL drop an invalid span and still show the
caption. A message SHALL count as mentioning a user from its `caption_mentions`,
never from the caption text, and never for the sender's own messages. Both fields
are optional, so a client that predates them sends and reads a captionless video as
before (an unknown field is ignored). The client enforces this.

#### Scenario: Caption mentions Bob
- **WHEN** Alice captions a video "@Bob look" with a `Mention` of `bob`
- **THEN** Bob's core reports the video message as mentioning him

#### Scenario: Bad caption span dropped
- **WHEN** a video arrives with a caption "hi" and a `Mention` over bytes 1..9
- **THEN** the core drops that mention and shows the caption "hi"

#### Scenario: Older client ignores the caption fields
- **WHEN** a video with a `caption` arrives at a client that predates the field
- **THEN** that client ignores the unknown field and still plays the clip
