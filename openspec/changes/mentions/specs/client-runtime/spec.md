## ADDED Requirements

### Requirement: Reply lookup
The core SHALL send a reply only to a stored chat message, building the
`ReplyTo` from that message's sender device, `msg_id` and text. For each
message it reports, the core SHALL report the reply's quote and replied-to
user, and the local id of the original when it has the original stored.

#### Scenario: Original on the device
- **WHEN** Alice's phone has Bob's "dinner at 8?" and receives Carol's reply to
  it
- **THEN** the core reports the reply with the quote, `bob@pi` as the
  replied-to user and the local id of "dinner at 8?"

#### Scenario: Original missing
- **WHEN** a reply arrives whose original this device never stored
- **THEN** the core reports the quote and replied-to user with no original id

#### Scenario: Reply to an unknown id
- **WHEN** the app asks to reply to a local id that is not a stored chat
  message
- **THEN** the core refuses and stores nothing

### Requirement: Unseen mentions
The core SHALL store, with each received message that mentions this device's
user or `everyone`, that the mention is unseen; SHALL list the unseen ones in
chat order; and SHALL mark one seen when the app says so. Messages from this
user's own devices SHALL never be unseen mentions.

#### Scenario: Mention survives a restart
- **WHEN** Bob's phone receives "@Bob look" and the app is killed before Bob
  saw it
- **THEN** after a restart the core still lists it as an unseen mention

#### Scenario: Seen once
- **WHEN** the app marks that mention seen
- **THEN** the core no longer lists it, also after a restart
