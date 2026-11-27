## MODIFIED Requirements

### Requirement: Text only
`Text` SHALL hold UTF-8 text and, optionally, mentions and one reply
reference, and nothing else: no attachments, link previews or rich
formatting. The client SHALL limit the encoded content to what encrypts within
the 65,536 byte ciphertext cap.

#### Scenario: Very long paste
- **WHEN** a user pastes text that would encrypt to more than 65,536 bytes
- **THEN** the app refuses to send it and tells the user it is too long

#### Scenario: Text with a mention and a reply
- **WHEN** Alice replies "@Bob yes" to Bob's message, mentioning Bob
- **THEN** her core encrypts one `Text` with text "@Bob yes", one `Mention`
  and one `ReplyTo`, and no other content

## ADDED Requirements

### Requirement: Mentions
A `Text` SHALL carry each mention as a `Mention`: a user id or `everyone`, and
the UTF-8 byte range in `Text.text` it covers. Any member MAY mention
`everyone`. The sending client SHALL refuse a range that is empty, outside the
text, not on character boundaries or overlapping an earlier one; a receiving
client SHALL drop such a mention and still show the text. A client SHALL treat
a message as mentioning a user when a `Mention` names that user or
`everyone`, never from the text, and SHALL NOT count a user's own messages as
mentioning them. The client enforces this.

#### Scenario: Alice mentions Bob
- **WHEN** Alice sends "hi @Bob" with a `Mention` of `bob@chat` over bytes 3..7
- **THEN** Bob's core reports the message as mentioning him, and Carol's core
  does not

#### Scenario: Anyone mentions everyone
- **WHEN** Carol, who is not the admin, sends "@everyone dinner" with a
  `Mention` of `everyone`
- **THEN** every other member's core reports the message as mentioning them

#### Scenario: Bad range is dropped
- **WHEN** a `Text` "hi" arrives with a `Mention` over bytes 1..9
- **THEN** the core drops that mention and the chat shows "hi"

#### Scenario: Old client
- **WHEN** a client that predates `Text.mentions` receives "hi @Bob" with a
  mention of `bob@chat`
- **THEN** it ignores the unknown field and shows "hi @Bob" as plain text

### Requirement: Replies
A `Text` that answers another message SHALL carry a `ReplyTo` naming the
original by its sender device (`DeviceRef`) and `msg_id`, and a quote of the
original's text: whitespace folded to single spaces, at most
`MAX_REPLY_QUOTE_CHARS` (100) characters, the last one "…" when cut. The sending
client SHALL build the quote from its stored original; a receiving client
SHALL cut a longer quote to the limit and SHALL show the quote and who was
replied to even when it does not have the original. The client enforces
this.

#### Scenario: Bob replies to Alice
- **WHEN** Bob replies "yes" to Alice's "dinner at 8?"
- **THEN** his core sends a `ReplyTo` with Alice's device, the original's
  `msg_id` and the quote "dinner at 8?"

#### Scenario: Long original
- **WHEN** Bob replies to a 300-character message
- **THEN** the quote is its first 99 characters followed by "…", 100 in all

#### Scenario: Device without the original
- **WHEN** Bob's tablet, added after Alice's "dinner at 8?", receives Bob's
  reply to it
- **THEN** the tablet shows the reply with "Alice" and "dinner at 8?" above it

#### Scenario: Old client and a reply
- **WHEN** a client that predates `Text.reply_to` receives Bob's reply "yes"
- **THEN** it shows "yes" as a plain message

### Requirement: Mentions and replies stay encrypted
Mentions, `everyone` and reply references SHALL travel only inside the
encrypted `Content`; the client SHALL put none of them in the `Envelope` or
any RPC, so the server learns nothing new. The server SHALL send the same wake ping
for a mentioning message as for any other, with the one fixed text "New
message". The client and the server enforce this.

#### Scenario: Mention while Bob is offline
- **WHEN** Alice mentions Bob while Bob's phone has no open stream
- **THEN** the server queues one `APPLICATION` envelope and sends Bob's phone one
  wake ping with the text "New message", the same as for a plain message
