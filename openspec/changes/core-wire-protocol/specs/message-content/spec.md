## ADDED Requirements

### Requirement: Inner content format
The plaintext of every MLS application message SHALL be a serialized
`vyajena.v1.Content` message: a client send time and exactly one of `Text`,
`GroupSettings` or `Profile`. Only the Rust core SHALL serialize or parse it; the Pi SHALL
never see it.

#### Scenario: Text message
- **WHEN** a user sends "hello"
- **THEN** the Rust core encrypts a `Content` with a `Text` body of "hello" and its
  send time

#### Scenario: Unknown body type
- **WHEN** a client decrypts a `Content` with a body type it does not know
- **THEN** it shows a placeholder saying the message needs a newer app version,
  and does not fail

### Requirement: Text only
`Text` SHALL hold UTF-8 text and nothing else: no attachments, link previews or
rich formatting. The client SHALL limit text to what encrypts within the 65,536
byte ciphertext cap.

#### Scenario: Very long paste
- **WHEN** a user pastes text that would encrypt to more than 65,536 bytes
- **THEN** the app refuses to send it and tells the user it is too long

### Requirement: Group settings travel encrypted
The group name and settings SHALL be sent as a `GroupSettings` body, and only
messages from admin devices SHALL change them. Clients SHALL ignore
`GroupSettings` from non-admin devices.

#### Scenario: Non-admin rename
- **WHEN** a client receives `GroupSettings` from a non-admin device
- **THEN** it ignores the change

### Requirement: User-owned profiles
A user SHALL be able to set their display name and a short "about" text by
sending a `Profile` body. Clients SHALL apply a `Profile` only to the user whose
device sent it, SHALL show the username alongside the display name, and SHALL
fall back to the username when no display name is set.

#### Scenario: Alice sets a display name
- **WHEN** Alice's phone sends a `Profile` with display name "Ali"
- **THEN** other members show "Ali (alice)" for her messages

#### Scenario: Profile cannot impersonate
- **WHEN** Bob sends a `Profile` with display name "akhil"
- **THEN** clients still show Bob's username next to it, so it reads "akhil (bob)"
