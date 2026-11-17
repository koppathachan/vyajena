## ADDED Requirements

### Requirement: Mention picker
The composer SHALL show, when the user types `@` at the start of the text or
after a space, the group's other members by display name and username,
and "everyone", filtered by what follows the `@`. Picking one SHALL insert
"@" + the display name (or username) and record a mention of that user id
(or `everyone`); editing inside an inserted name SHALL drop that mention.
The client enforces this.

#### Scenario: Alice mentions Bob
- **WHEN** Alice types "hi @bo" and picks "Bobby (bob)"
- **THEN** the composer shows "hi @Bobby " and sending calls the core with a
  mention of `bob@pi` over "@Bobby"

#### Scenario: Everyone
- **WHEN** Carol types "@" and picks "everyone"
- **THEN** the composer shows "@everyone " and sends a mention of `everyone`

#### Scenario: Name edited away
- **WHEN** Alice inserts "@Bobby" and then deletes its last letter
- **THEN** the message is sent with no mention

### Requirement: Mentions of me
The app SHALL draw each mention as "@" + this device's name for the target
(or "@everyone"), highlight mentions of this device's user and `@everyone`
more strongly than other mentions, and show in the chat's app bar an `@`
badge with the number of unseen mentions. A mention SHALL become seen when
its message has been on screen while the app is in the foreground, or when
the user taps the badge, which jumps to the oldest unseen one. The push text
SHALL stay "New message". The client enforces this.

#### Scenario: Bob is mentioned
- **WHEN** Bob's phone shows Alice's "hi @Bob"
- **THEN** "@Bobby" is highlighted in the bubble

#### Scenario: Badge
- **WHEN** Bob has scrolled up to older messages and two messages mention him
- **THEN** the app bar shows an `@` badge with 2, and tapping it jumps to the
  older one and lowers the count to 1

#### Scenario: Not mentioned
- **WHEN** Carol's phone shows Alice's "hi @Bob"
- **THEN** "@Bobby" is drawn as a mention but not highlighted as hers, and
  Carol's badge does not change

### Requirement: Reply to a message
The app SHALL let the user reply to a message by swiping it to the right or
by long-pressing it and choosing Reply; the composer SHALL then show the
quote with a way to cancel. A reply SHALL show the replied-to user's name
and the quote above its text, and tapping the quote SHALL scroll to the
original when this device has it, else say that the original is not on this
device. The client enforces this.

#### Scenario: Swipe to reply
- **WHEN** Bob swipes Alice's "dinner at 8?" to the right and sends "yes"
- **THEN** the core gets a reply to that message, and the chat shows "yes"
  under "Alice (alice)" and "dinner at 8?"

#### Scenario: Long-press to reply
- **WHEN** Bob long-presses a message and taps Reply
- **THEN** the composer shows its quote and a cancel button

#### Scenario: Jump to the original
- **WHEN** Bob taps the quote of a reply whose original is in the chat
- **THEN** the chat scrolls to the original

#### Scenario: Original not on this device
- **WHEN** Bob's tablet taps the quote of a reply whose original it never had
- **THEN** the app says the original is not on this device
