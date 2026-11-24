## Why

In the unified Chats list (`app-shell`), every row shows a last-message preview as
its subtitle. The owner wants a **DM** row to show the other person's **"about"**
text there instead — the short self-description each user already sets on their
profile (`message-content`: User-owned profiles; the `Profile.about` field). It
makes the contact's own words the thing you read under their name, more like an
address book than a message log.

Groups have no single "about": the lobby and admin-created named groups are shared
spaces, and `GroupSettings` carries only a `name`, not a description. So group rows
keep the last-message preview. The per-row rule is spelled out in `design.md`.

This is a **presentation-only** change to the chats-list row subtitle. The peer's
`about` already reaches the app inside the peer's `Profile`; nothing on the wire,
the server or the core changes.

## What Changes

- **DM rows show the peer's `about` as the subtitle**, in place of the last-message
  preview. When the peer has set no `about`, the row falls back to the last-message
  preview (and, with no messages yet, a neutral placeholder).
- **Group rows (lobby + named groups) keep the last-message preview** with its media
  glyph, unchanged — there is no group `about` to show.
- **Closed-DM rows keep their existing closed treatment** (greyed, swipe-to-remove)
  from `app-shell` / `delete-messages`; the about subtitle applies to open DMs.
- **No proto / server / wire / core change** — the subtitle is chosen in the app
  from data it already has (the peer's `Profile.about` and the existing chat
  summary).
- **Out of scope:** a group description field (would be a separate
  `GroupSettings`/`message-content` change — see the open decision); showing `about`
  anywhere other than the chats-list subtitle; truncation/length rules beyond
  one-line ellipsis.

## Open decisions (owner)

- **D1 — Group about-row rule.** Proposed: groups keep the **last-message preview**,
  because no per-group "about" exists today. If the owner wants parity, a later
  change can add `GroupSettings.description` and groups would show that; flagged, not
  included here.

## Proto files touched

- None. No `.proto`, `protocol/`, server or core change; the app re-sources one
  subtitle string from data the bridge already provides.

## Capabilities

### Modified Capabilities

- `app-shell`: the Chats list's DM rows show the peer's `about` as the subtitle
  instead of the last-message preview; group rows keep the preview. (Added as a new
  requirement in the `app-shell` capability; see Notes on sequencing in `design.md`,
  since `app-shell` is still a pending change.)

## Impact

- **App (`app/`):** the chats-list row builder picks the subtitle by row type — DM →
  peer `about` (fallback preview); lobby/named group → last-message preview. Reuses
  the peer `Profile` and chat summary already in hand.
- **Depends on / references:** `app-shell` (the unified Chats list and its
  last-message-preview rows — a **pending** change, not yet archived) and
  `message-content` (`Profile.about`). **PR #28 is concurrently fixing the
  profile-tab back-navigation; do not assume it is merged.** Also related to
  `manual-presence` (same PR), which adds the presence dot to these same avatars.
