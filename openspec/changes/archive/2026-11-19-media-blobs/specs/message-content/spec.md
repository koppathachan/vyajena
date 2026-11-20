## MODIFIED Requirements

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

## ADDED Requirements

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
