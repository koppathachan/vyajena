## Why

The app grew feature by feature — a single chat, then DMs and a chat list
(`dms`), the lobby and named groups (`lobby-multi-group`), images, audio, links,
and soon video. The owner approved (2026-11-23) a standard, Telegram-like
navigation shell to hold it all: a bottom tab bar, one unified conversation list,
one conversation screen, a contacts tab, and a profile/settings tab reached by the
user's own round avatar.

This is a **presentation and navigation restructure only** — no protocol, crypto,
server or wire change. It reorganises existing capabilities (`app-ui`, `dms`,
`lobby-multi-group`, media/image, `audio-messages`, `links`, and forward-refs
`video-messages`) under one shell. It revives the previously deferred "feature 2"
(a members tab + bottom nav + a Telegram-like list).

## What Changes

- **Bottom navigation with three destinations** (owner, 2026-11-23): **Chats**,
  **Contacts**, and a third tab that is the user's **own round avatar** — not a
  gear ("settings should be a profile icon, a round photo, like Telegram"). The
  avatar tab opens the user's profile, settings and server info.
- **Chats tab — one unified conversation list:** the `lobby` system group pinned
  first, then admin-created named groups, then DMs. Each row shows an avatar, the
  name, a last-message preview with a media glyph (image / video / voice),
  a timestamp and an unread count. Closed DMs render greyed at the bottom with the
  swipe-to-remove affordance — this is the closed-DM removal already specified in
  `delete-messages` (referenced, not redefined here).
- **Conversation screen — one screen for the lobby, named groups and DMs:** message
  bubbles incoming-left / outgoing-right; the sender's name shown in **groups only**
  (never in a DM); tappable links (`links`); inline media players for image, audio
  and video; an **attachment sheet** (photo / video / audio / file) and a **mic
  button**; an app bar with back, the group/peer avatar, a title + subtitle, and an
  overflow menu.
- **Contacts tab:** the server's members; the admin carries an `admin` badge; the
  current user is marked "you"; an **Invite** action; admin entry points for
  **create group** and **replace user**; tapping a member opens or starts a DM.
- **Profile tab:** the user's identity and profile (display name, about), server
  info, and settings.
- **Out of scope:** any protocol/crypto/server/wire change; new message types;
  changing who-may-do-what (membership, delete rights, admin powers) — those stay
  as their own changes define them; theming beyond the existing Material 3 look.
- Details and the navigation graph: `design.md`.

## Open decisions resolved (owner, 2026-11-23)

- **Three tabs:** Chats, Contacts, Profile.
- **Third tab is the user's round avatar**, opening profile + settings + server
  info (not a gear icon).
- **One unified conversation list** (lobby pinned, named groups, DMs) and **one
  conversation screen** for all three kinds.
- **Sender name in groups only**, never in DMs.

## Proto files touched

- None. No `.proto`, `protocol/`, server or core change. This change only
  reorganises Dart UI and navigation over the existing bridge API.

## Capabilities

### New Capabilities

- `app-shell`: the bottom-tab navigation, the unified Chats list, the one
  conversation screen, the Contacts tab and the Profile tab.

## Impact

- **App (`app/`):** a navigation shell (bottom `NavigationBar` with three
  destinations), the Chats list screen (reusing the chat summaries the core already
  provides), the unified conversation screen (reusing the existing message tile,
  composer, mention, link and media widgets), the Contacts screen (reusing the
  members data and the admin actions), and the Profile screen (reusing the profile
  and server-info data). Existing screens are re-parented under the shell; their
  behaviour is unchanged.
- **No bridge change** expected — the shell consumes the existing chat-summary,
  member, profile and server-info APIs. If a convenience field is missing (e.g. a
  per-row media glyph), it is a bridge addition noted at implementation, not a
  protocol change.
- **Depends on:** `dms`, `lobby-multi-group` (the chats the list shows) and the
  media changes (the inline players) for full content; the shell itself can land on
  what is merged and show the rest as those changes ship. References the
  `delete-messages` closed-DM removal and the `replace-user` admin action.
