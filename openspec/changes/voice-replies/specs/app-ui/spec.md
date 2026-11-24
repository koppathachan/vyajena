## ADDED Requirements

### Requirement: Reply with a voice message
The composer SHALL keep the microphone available while a reply is pending, so a
user can answer a message with a voice note. The mic SHALL show whenever the text
field is empty, whether or not a reply is pending; typing text SHALL still swap the
mic for Send (a text reply), unchanged. When the user records while a reply is
pending, the app SHALL send the voice message answering that message (message-
content: Voice replies) and SHALL clear the reply bar, exactly as a text reply
does. The UI goes through the core (app-ui: UI goes through the core).

#### Scenario: Mic stays while replying
- **WHEN** Bob swipes to reply to Alice's message and the text field is empty
- **THEN** the composer still shows the mic (not only Send), and the reply bar
  stays visible

#### Scenario: Send a voice reply
- **WHEN** Bob records and sends a voice message with that reply pending
- **THEN** the app calls the core to send a voice message answering Alice's, and
  the reply bar clears

#### Scenario: Typing still means a text reply
- **WHEN** Bob types text while a reply is pending
- **THEN** the composer shows Send (not the mic), and sending makes a text reply as
  before

### Requirement: Media reply preview
A reply that answers a media message SHALL render a media-appropriate preview —
rather than a text quote or the "needs a newer app version" placeholder — in both
the composer's reply bar and the sent/received message bubble. A voice original
SHALL show a mic glyph and the clip's duration; a picture original SHALL show a
thumbnail (its ThumbHash blur) with its caption if any; a text original SHALL show
the quoted text, as before. The preview SHALL render from the reply's
`original_kind` and the media fields it carries (message-content: Reply quote
describes the original's kind), so it shows correctly even when this device does
not hold the original.

#### Scenario: Reply bar previews the voice original
- **WHEN** Bob swipes to reply to Alice's voice message
- **THEN** the reply bar shows a mic glyph with the duration and "Voice message",
  not empty text and not the placeholder

#### Scenario: Bubble previews the voice original without holding it
- **WHEN** Carol opens the chat and sees Bob's reply to Alice's voice message, but
  Carol's device does not have Alice's original
- **THEN** the reply quote in the bubble shows a mic glyph and the duration, not the
  "needs a newer version" placeholder

#### Scenario: Picture reply shows a thumbnail
- **WHEN** Carol sees Bob's reply to Alice's photo and does not hold Alice's
  original
- **THEN** the reply quote shows a thumbnail of the photo (its ThumbHash blur),
  not the "needs a newer version" placeholder

#### Scenario: A known media kind never looks unsupported
- **WHEN** a reply quote's text is empty but its `original_kind` is audio or image
- **THEN** the bubble shows the voice or picture preview, not the "needs a newer
  version" placeholder
