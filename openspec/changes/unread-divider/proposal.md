## Why

When a member opens a chat after being away, the chat lands at the newest
message with no sign of where they stopped reading. The chat list already
shows an unread count (change `dms`), but inside the chat there is nothing to
separate the messages read last time from the new ones, and no quick way back
to the bottom after scrolling up through them.

The data to fix this already exists locally: the core keeps a per-chat read
mark (`chat_reads.read_up_to`, the local id of the newest message read last
time) and counts unread messages from it. Nothing on the wire changes.

## What Changes

- **A "New messages" divider** in the chat, drawn once above the first
  received message newer than the read mark as it stood when the chat opened.
  The divider is pinned to that snapshot: it stays put while the chat is open
  (even as the read mark advances) and re-pins the next time the chat opens.
- **Open on the first unread message:** instead of always landing at the
  bottom, the chat scrolls to the first unread message on open. With no
  unread messages it lands at the bottom as before.
- **A scroll-to-bottom button** (a small FAB), shown only while the chat is
  scrolled up, that jumps back to the newest message.
- **Expose the read mark:** the core gains one read-only accessor,
  `chat_read_mark(chat_id)`, returning the stored `read_up_to` (0 if the chat
  was never read). This is the only new bridge surface. No change to the
  protocol, the Pi, or any `.proto` file.
- The behaviour is identical for the main group and DMs (both use
  `chat_reads`).
- **Out of scope:** a per-message "seen by" state, unread counts on the app
  icon, syncing the read mark across a user's devices.

## Open decisions resolved

- None. No open architecture decision is touched.

## Proto files touched

- None. The read mark is local; this is a UI change plus one local accessor.

## Capabilities

### Modified Capabilities

- `app-ui`: a "New messages" divider and open-on-first-unread in the chat
  screen, a scroll-to-bottom button, and the read-mark accessor the divider
  reads.

## Impact

- **Core (`client/`):** one accessor `Store::chat_read_mark` and
  `Client::read_mark`; no schema change (the `chat_reads` table already
  exists).
- **Bridge (`bridge/`):** one method `Core::chat_read_mark`; `frb_generated`
  and `app/lib/src/rust/` regenerated.
- **App (`app/`):** the chat screen draws the divider, scrolls to the first
  unread message on open, and shows the scroll-to-bottom button;
  `CoreService` gains `chatReadMark`.
- **Depends on:** `dms` (PR #5), merged, for the per-chat read mark.
