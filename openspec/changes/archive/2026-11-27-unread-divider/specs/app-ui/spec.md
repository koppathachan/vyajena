## ADDED Requirements

### Requirement: Read mark

The core SHALL expose a chat's read mark through `chat_read_mark(chat_id)`: the
local id of the newest message the user had read in that chat, as
`mark_chat_read` last set it, or 0 when the chat was never read. The accessor
SHALL only read; it SHALL NOT advance the mark. It SHALL work for the main
group and for DMs alike.

#### Scenario: Never read

- **WHEN** Bob has never read the main group's chat
- **THEN** `chat_read_mark(0)` returns 0

#### Scenario: Marked up to the newest read message

- **WHEN** Bob receives Alice's "one", calls `mark_chat_read(0)`, and then
  receives Alice's "two" without reading again
- **THEN** `chat_read_mark(0)` returns the local id of "one", and the chat's
  `unread` count is 1

#### Scenario: Advances only on a read

- **WHEN** Bob then calls `mark_chat_read(0)` again
- **THEN** `chat_read_mark(0)` returns the local id of "two"

### Requirement: Unread divider

The chat screen SHALL read the chat's read mark once when it opens, before it
advances the mark, and SHALL draw a single "New messages" divider immediately
above the first received message (not one of this user's own devices') whose
local id is greater than that snapshot. The divider SHALL stay fixed while the
chat is open, even as new messages arrive and the stored mark advances, and
SHALL be re-placed from a fresh snapshot the next time the chat opens. On open
the chat SHALL scroll to that first unread message instead of the bottom. When
no received message is newer than the snapshot the chat SHALL show no divider
and land at the bottom as before. When the first unread message is the oldest
message shown, with no message above it, the chat SHALL show no divider but
SHALL still land on that message. The behaviour SHALL be the same for the main
group and for DMs.

#### Scenario: New messages since last read

- **WHEN** Bob read the group's chat up to "one" last time, and "two" and
  "three" arrived from Alice while he was away, and he opens the chat
- **THEN** a "New messages" divider is shown below "one" and above "two", and
  the chat opens scrolled to "two"

#### Scenario: Nothing new

- **WHEN** Bob has read every message in the chat and opens it
- **THEN** no "New messages" divider is shown and the chat is at the bottom

#### Scenario: Divider stays put while reading

- **WHEN** the divider is shown above "two" and a new message "four" arrives
  from Alice while Bob is in the chat
- **THEN** the divider stays above "two" and does not move to "four"

#### Scenario: All unread

- **WHEN** Bob opens a chat he has never read
- **THEN** no divider is shown and the chat lands on the oldest message in the
  window

### Requirement: Scroll to bottom

The chat screen SHALL show a scroll-to-bottom button only while the chat is
scrolled up away from the newest message, and tapping it SHALL return the chat
to the newest message and hide the button. When the chat is already at the
newest message the button SHALL NOT be shown.

#### Scenario: Scrolled up

- **WHEN** Bob scrolls up from the newest message in a long chat
- **THEN** a scroll-to-bottom button appears

#### Scenario: Back to the bottom

- **WHEN** Bob taps the scroll-to-bottom button
- **THEN** the chat returns to the newest message and the button disappears
