# Design

## Context

The core already tracks, per chat, a read mark: `chat_reads.read_up_to` holds
the local id of the newest message the user had read, `Store::mark_chat_read`
bumps it to the newest message, and `Store::unread_in` counts received
messages (`outgoing = 0`) with a greater local id. The chat list shows that
count (change `dms`). Message local ids are a monotonically increasing
`INTEGER PRIMARY KEY`, so "the first unread message" is the first message with
`local_id > read_up_to`.

Nothing surfaced the read mark itself, and the chat screen
(`app/lib/chat/chat_screen.dart`) always opened at the bottom of its
`reverse: true` list with no divider.

## Decisions

### D1 — Expose the read mark as a read-only accessor, not on `ChatSummary`

The divider needs the read mark *id* (not just the unread *count* the chat
list already has), captured at the instant the chat opens. Two shapes were
considered: adding `read_up_to` to `ChatSummary`, or a dedicated accessor.

Chosen: a dedicated `chat_read_mark(chat_id) -> i64` (0 when the chat was
never read). `ChatSummary` is the chat-list row; the divider is a chat-screen
concern that wants exactly one number for one chat at open time. A dedicated
accessor keeps `ChatSummary` about the list and lets the screen read the mark
without fetching and scanning the whole chat list. It mirrors the existing
`Client::mark_chat_read` / `Store::mark_chat_read` pair.

### D2 — Snapshot the read mark on open, before `markRead` advances it

The chat screen already advances the read mark to the newest message in a
post-frame callback (`_markRead`) once the history is loaded and the app is in
the foreground. The divider must pin to where the user *was*, so the screen
reads `chat_read_mark` once in `initState` and, crucially, holds off
`_markRead` until that read has returned (`_snapshotLoaded`). This removes any
race between reading the old mark and writing the new one: the write cannot
happen until the read is done.

The snapshot lives on the screen state, not on the long-lived `ChatModel` (the
main group's model outlives the screen). So the divider re-pins every time the
screen opens, which is the wanted behaviour.

```mermaid
sequenceDiagram
    participant S as ChatScreen (initState)
    participant C as core
    participant F as first frame / post-frame
    S->>C: chat_read_mark(chatId)
    C-->>S: read_up_to = M  (snapshot)
    Note over S: _snapshotLoaded = true
    S->>F: history loaded, build list
    F->>F: place divider before first received id > M
    F->>C: _scrollTo(first unread) or stay at bottom
    F->>C: markRead()  (only now; advances mark to newest)
```

### D3 — The divider sits before the first *received* message past the mark

The divider is placed before the first message with `id > snapshot` that is
**not** from this user's own devices (`ownUser == false`). This matches
`Store::unread_in`, which counts received messages, and avoids drawing a "New
messages" label above a message the user themselves sent from another device.

Edge cases (all by id comparison, so an expired gap is harmless):

- **No unread messages** (no received message past the mark): no divider; the
  chat lands at the bottom, as today.
- **Everything unread** (the first unread message is the oldest message in the
  window, with nothing above it): no divider — a divider at the very top
  separates nothing — but the chat still lands at the top, on that message.
- **While the chat stays open:** the snapshot does not move, so the divider
  stays where it was even as `markRead` advances the stored mark and as new
  messages arrive below.

### D4 — Scroll-to-bottom button

The list is `reverse: true`, so the newest message is at scroll offset 0. A
small `FloatingActionButton` is shown only while the controller is scrolled
away from the bottom (offset past a small threshold) and animates back to
offset 0 on tap. It reuses the screen's existing `ScrollController`.

### D5 — Test home: `app-ui`, not `message-delivery`

The read mark is a local, client-side position with no wire or server behaviour;
`message-delivery` is about envelopes, ordering and the server. Every existing
read/unread/chat test is tagged under `app-ui` (`app-ui/chat-list`,
`app-ui/chat-screen`), and the divider, the open behaviour and the button are
all chat-screen UI. So all three new requirements live in `app-ui`, and the
Rust accessor test is tagged `app-ui/read-mark`. (The handover note suggested a
`message-delivery` slug; `app-ui` is the capability that actually owns this
behaviour and matches the established tagging.)

## Risks

- Display order is `server_seq` then local id, so in a rare case a received
  message with a smaller local id than an own pending message could sort after
  it. The divider walks the displayed list and stops at the first received
  message past the snapshot, so it is always placed at a real message boundary;
  it never splits the list at a non-existent position.
