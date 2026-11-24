## ADDED Requirements

### Requirement: Voice replies
A voice message SHALL be able to answer another message. The `Audio` body SHALL
carry an optional `reply_to` of the same `ReplyTo` shape a text reply uses
(message-content: Replies), set when the voice message answers another message and
unset otherwise. The reply lives inside the encrypted `Content`; the server never
sees it and stores no history. A client that predates the field SHALL decode the
body as a plain `Audio` (the unknown field is ignored) and SHALL play the voice
message with no quote, never the "needs a newer app version" placeholder. The core
builds the `reply_to` from the answered message exactly as it does for a text
reply. Text and image replies are unchanged.

#### Scenario: Bob answers with a voice note
- **WHEN** Bob records a voice message while a reply to Alice's message is pending
- **THEN** the core sends an `Audio` body whose `reply_to` names Alice's message
  (its `sender`, `msg_id`, quote and original kind), and the recipients show the
  voice message as answering Alice's

#### Scenario: Old app receives a voice reply
- **WHEN** a device whose app predates `Audio.reply_to` receives a voice reply
- **THEN** it plays the voice message and shows no quote, and shows no "needs a
  newer version" placeholder

### Requirement: Reply quote describes the original's kind
A `ReplyTo` SHALL carry the kind of the message it answers (`original_kind`: text,
image, or audio) and enough to preview any **media** original without holding it:
for a voice original its length (`original_duration_ms`), and for a picture
original its ThumbHash (`original_thumbhash`). This generalizes the fix to **all
media original types**, not voice alone. The core SHALL set `original_kind` from
the answered message's body, `original_duration_ms` from a voice original's
duration, and `original_thumbhash` from a picture original's ThumbHash (each 0 or
empty otherwise); the text `quote` SHALL stay the original's text excerpt, a
picture's caption, or empty for a voice original. A receiver SHALL render the quote
from `original_kind` — a voice original as a voice preview (a mic glyph and the
duration), a picture original as a thumbnail (its ThumbHash blur) with its caption
if any, and a text original as its quoted text — and SHALL NOT treat an empty quote
as an unknown body. A known media kind SHALL never fall back to the "needs a newer
app version" placeholder; that placeholder is only for a body whose kind the client
does not know, which `original_kind` now distinguishes from a media original. All
of this is client-side content inside the encrypted `Content`.

#### Scenario: Replying to a voice message
- **WHEN** Alice replies to Bob's 7-second voice message
- **THEN** the `ReplyTo` carries `original_kind = ORIGINAL_KIND_AUDIO` and
  `original_duration_ms = 7000` with an empty text quote, and every recipient shows
  the reply quoting a voice message with a mic glyph and "0:07", whether or not it
  holds Bob's original

#### Scenario: Replying to a picture
- **WHEN** Alice replies to Bob's photo (with or without a caption)
- **THEN** the `ReplyTo` carries `original_kind = ORIGINAL_KIND_IMAGE` and the
  photo's `original_thumbhash` (the caption, if any, as the quote), and every
  recipient shows the reply quoting a thumbnail of the photo, never the "needs a
  newer version" placeholder, whether or not it holds Bob's original

#### Scenario: Replying to a text message is unchanged
- **WHEN** Alice replies to Bob's text "see you at 6"
- **THEN** the `ReplyTo` carries the text quote as before, with
  `original_kind = ORIGINAL_KIND_TEXT`, and the quote renders as the text

#### Scenario: Quote from an older sender
- **WHEN** a reply arrives from an app that predates `original_kind` (so it is
  `ORIGINAL_KIND_UNSPECIFIED`) with a non-empty text quote
- **THEN** the receiver renders it as a text quote, exactly as before this change
