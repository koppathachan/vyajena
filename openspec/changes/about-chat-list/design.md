## Context

- The unified Chats list comes from `app-shell` (pending): the lobby pinned first,
  then named groups, then DMs, each row with "an avatar, the name, a last-message
  preview with a media glyph, a timestamp and an unread count".
- Each user owns a `Profile { display_name, about }` (`message-content`: User-owned
  profiles). The app already receives a DM peer's `Profile`, so `about` is in hand
  with no new fetch.
- Groups: `GroupSettings` carries only `name` — there is no group description or
  group "about" anywhere in the contract today.

## Goals / Non-Goals

**Goals**
- A DM row reads as the contact's own words: peer `about` under their name.
- Groups are unaffected (preview stays), with a clear fallback for DMs that have no
  `about` yet.
- Zero wire/server/core change.

**Non-Goals**
- A group description field; showing `about` elsewhere; a settings toggle to choose
  preview-vs-about.

## Decisions

### D1. Group about-row rule *(OPEN — owner)*

- **Proposed:** groups (the lobby and named groups) **keep the last-message
  preview**. There is no per-group "about" to show.
- **Alternative (flagged):** add `GroupSettings.description` (a separate
  `message-content` change) and show it on group rows for parity. Not in this change.

### D2. Per-row subtitle rule

| Row type            | Subtitle                                                        |
| ------------------- | --------------------------------------------------------------- |
| Lobby               | last-message preview (+ media glyph) — unchanged                |
| Named group         | last-message preview (+ media glyph) — unchanged                |
| DM, open, peer has `about` | the peer's `Profile.about`, one line, ellipsised        |
| DM, open, no `about`       | fall back to last-message preview                       |
| DM, open, no `about`, no messages | neutral placeholder (e.g. "No messages yet")     |
| DM, closed          | existing closed treatment (greyed, swipe-to-remove) — unchanged |

- The timestamp, unread count and avatar (and the `manual-presence` dot) are
  unchanged; only the **subtitle string** differs for open DM rows.
- `about` is shown verbatim (already plaintext the user chose); the row ellipsises
  to one line, same as a preview.

### D3. Presentation only — no contract change

- The app selects the subtitle locally from the peer `Profile` and the chat summary
  it already has. If a convenience field were ever wanted (e.g. the summary carrying
  a precomputed subtitle), that is a bridge-layer detail noted at implementation,
  not a protocol change — the same stance `app-shell` takes for its per-row glyph.

## Notes on sequencing

- **`app-shell` is a pending change, not yet archived into `specs/`.** This change
  therefore **adds** a requirement to the `app-shell` capability (it cannot `MODIFY`
  a requirement that is not in the baseline spec yet). The added requirement refines
  `app-shell`'s "Chats list" row, which still says every row shows a last-message
  preview.
- **Reconciliation at archive time:** whichever of `app-shell` / `about-chat-list`
  archives second should fold this DM-subtitle rule into the single "Chats list"
  requirement so the spec does not read two conflicting subtitle rules. Until then,
  this requirement is authoritative for the DM-row subtitle.
- **PR #28** is concurrently fixing the profile-tab back-navigation. It touches the
  profile tab, not the chats list, but both are profile/chat-list UI; do not assume
  #28 is merged, and keep this change independent of it.
- Ships in the **same PR as `manual-presence`**, which adds the presence dot to the
  very same chat-list avatars.
