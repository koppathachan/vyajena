# message-content Specification

## Purpose
TBD - created by archiving change core-wire-protocol. Update Purpose after archive.
## Requirements
### Requirement: Inner content format
The plaintext of every MLS application message SHALL be a serialized
`vyajena.v1.Content` message: a client send time and exactly one of `Text`,
`GroupSettings`, `Profile` or `Image`. Only the Rust core SHALL serialize or
parse it; the server SHALL never see it.

#### Scenario: Text message
- **WHEN** a user sends "hello"
- **THEN** the Rust core encrypts a `Content` with a `Text` body of "hello" and
  its send time

#### Scenario: Image message
- **WHEN** a user sends a photo
- **THEN** the Rust core encrypts a `Content` with an `Image` body carrying the
  blob id, the blob key, the dimensions and the ThumbHash

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
`GroupSettings` from non-admin devices. After merging a commit that adds devices,
each admin device SHALL re-send the current `GroupSettings` if a name is set, so
devices added later learn it.

#### Scenario: Non-admin rename
- **WHEN** a client receives `GroupSettings` from a non-admin device
- **THEN** it ignores the change

#### Scenario: Tablet joins after the rename
- **WHEN** the admin renamed the group to "Home chat" and later Bob's tablet is
  added by a `COMMIT`
- **THEN** the admin's phone re-sends `GroupSettings` "Home chat" and the tablet
  shows "Home chat" as the title

### Requirement: User-owned profiles
A user SHALL be able to set their display name and a short "about" text by
sending a `Profile` body. Clients SHALL apply a `Profile` only to the user whose
device sent it, SHALL show the username alongside the display name, and SHALL
fall back to the username when no display name is set. After merging a commit that
adds devices, every device SHALL re-send its own user's `Profile` if one is set.
When a user's identity key changes, clients SHALL drop that user's stored profile.

#### Scenario: Alice sets a display name
- **WHEN** Alice's phone sends a `Profile` with display name "Ali"
- **THEN** other members show "Ali (alice)" for her messages

#### Scenario: Profile cannot impersonate
- **WHEN** Bob sends a `Profile` with display name "akhil"
- **THEN** clients still show Bob's username next to it, so it reads "akhil (bob)"

#### Scenario: New device learns the names
- **WHEN** Bob set display name "Bobby" and later Bob's tablet is added
- **THEN** Bob's phone re-sends its `Profile`, and the tablet and every other
  member show "Bobby (bob)"

#### Scenario: Reset user starts without a name
- **WHEN** Bob had display name "Bobby", is removed, and registers again with a
  new identity key
- **THEN** members show the key-changed warning and his messages as "bob" until
  the new Bob sets a display name

### Requirement: Image messages
An `Image` body SHALL carry a 16-byte `blob_id`, a 256-bit `blob_key`, the WebP
pixel `width` and `height`, a `thumbhash` placeholder of at most
`protocol::blob_store::MAX_THUMBHASH_BYTES` bytes, the `blob_size`, and an
optional `caption` with `caption_mentions`. It SHALL reference exactly one
static image; there SHALL be no albums, animation or video. The `blob_key` SHALL
travel only inside the encrypted `Content` and SHALL never appear in an
`Envelope` or any RPC, so the server and the relay never learn it. A client built
before `Image` SHALL show the unknown-body placeholder (Inner content format).
The client enforces this.

#### Scenario: Alice sends an image with a caption
- **WHEN** Alice sends a photo captioned "@Bob look"
- **THEN** her core encrypts one `Content` whose `Image` body holds the blob id,
  the blob key, the dimensions, the ThumbHash and a `caption` "@Bob look" with a
  `Mention` of `bob`, and no key appears outside the ciphertext

#### Scenario: Old client receives an image
- **WHEN** a client that predates `Image` decrypts an image message
- **THEN** it shows the "needs a newer app version" placeholder and does not fail

### Requirement: Image caption and mentions
An `Image.caption` SHALL be optional UTF-8 limited like a text message, and its
`caption_mentions` SHALL follow the same `Mention` rules as `Text.mentions`
(Mentions): each a user id or `everyone` over a valid, non-overlapping UTF-8 byte
range of the caption. The sending client SHALL refuse an over-limit caption or an
invalid span; a receiving client SHALL drop an invalid span and still show the
caption. A message SHALL count as mentioning a user from its `caption_mentions`,
never from the caption text, and never for the sender's own messages. The client
enforces this.

#### Scenario: Caption mentions Bob
- **WHEN** Alice captions an image "@Bob look" with a `Mention` of `bob`
- **THEN** Bob's core reports the image message as mentioning him

#### Scenario: Bad caption span dropped
- **WHEN** an image arrives with a caption "hi" and a `Mention` over bytes 1..9
- **THEN** the core drops that mention and shows the caption "hi"

### Requirement: Image messages stay encrypted
An image reference and its caption SHALL travel only inside the encrypted
`Content`; the blob the server stores SHALL be ciphertext with no key. The server SHALL
send the same wake ping for an image message as for any other, with the one
fixed text "New message". The client and the server enforce this.

#### Scenario: Image while Bob is offline
- **WHEN** Alice sends an image while Bob's phone has no open stream
- **THEN** the server queues one `APPLICATION` envelope and sends Bob's phone one
  wake ping with the text "New message", the same as for a plain message
