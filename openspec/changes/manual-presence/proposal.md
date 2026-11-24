## Why

Every user wants a quick way to tell others "I'm around" or "don't expect me",
the way Discord and Reddit let you flip your own dot green or red. The owner asked
for a **manually set** online/offline badge — a self-chosen flag, **not** anything
derived from whether the app is open, the stream is connected, or the user is
typing. On = green, Off = red; nothing finer for the MVP.

The owner also pinned the interaction: the user's own round profile avatar in the
bottom navigation (the third tab, from `app-shell`) already sits on an oval shadow.
That avatar becomes a **swipe toggle** — swipe one way to go Online (green), the
other to go Offline (red) — and the oval shadow/ring around it shows the current
state. So the profile tab doubles as your presence switch, with no extra screen.

This is the app's first piece of **server-visible, non-content presence metadata**.
It fits the existing pattern (the server already stores small per-user markers such
as push tokens and the removed-conversation set and routes per-conversation), and
it is carried on the one `Messaging.Connect` stream the server already uses to fan
notices out to the right devices.

## What Changes

- **A manual presence flag per user:** Online or Offline, nothing else (no
  away/busy for the MVP). The user sets it; the server never infers it from
  activity or connection.
- **Swipe the profile-tab avatar to toggle it** (owner interaction): a horizontal
  swipe on the bottom-nav profile avatar sets Online (swipe right) or Offline
  (swipe left); the oval shadow/ring renders green when Online and red when Offline.
- **A presence dot/ring on avatars across the app** — chat-list rows, the Contacts
  tab and conversation headers — showing green/red for each user whose status the
  viewer may see.
- **Proto / wire:** a new `PresenceStatus` enum and two stream frames —
  `ClientFrame.SetPresence` (the user sets their flag) and
  `ServerFrame.PresenceUpdate` (the server tells a device about a user's current
  flag). Protocol version bumps `3 → 4`; the server sends `PresenceUpdate` only to
  devices that negotiated version 4 or above, and older clients ignore the unknown
  frame.
- **Server:** stores each user's manual flag durably (it survives disconnect and
  restart, because it is manual, not connection-derived), broadcasts a change to
  every other user who shares a conversation with the setter, and replays each
  visible user's current flag as a snapshot when a device connects.
- **Privacy:** called out in `design.md` — the server and the people who can see you
  learn the flag you set yourself. It is low-sensitivity metadata, not message
  content, and because it is manual there is **no activity inference**.
- **Out of scope:** away/busy or any third state; last-seen or "online N min ago"
  timestamps; presence derived from the connection or from reading/typing;
  per-conversation or per-device presence; blocking/hiding presence from specific
  users (beyond the one visibility-scope decision below).

Details, the wire shapes, the snapshot flow and the privacy analysis: `design.md`.

## Open decisions (owner)

- **D1 — Who may see whose status (visibility scope).** Proposed: every user you
  **share a conversation with** (the lobby, any named group, or a DM). Because
  everyone is in the system lobby, that is in practice **every member of the
  server**. The alternative is a narrower **contacts-only** set, which this server
  model does not yet distinguish from "all members". Flagged for the owner; the spec
  is written to the "shared-conversation" rule, which the server can already compute.
- **D2 — Default flag on first launch / before the user ever sets it.** Proposed:
  **Online (green)**, Discord-style, so a new user is reachable by default. The
  alternative is **Offline (red)** (private by default). Flagged for the owner.

## Proto files touched

- `proto/vyajena/v1/messaging.proto` — add `PresenceStatus` enum, `SetPresence` and
  `PresenceUpdate` messages, a `set_presence` arm on `ClientFrame` and a
  `presence_update` arm on `ServerFrame` (all additive).
- `protocol/src/wire_transport.rs` — `PROTOCOL_VERSION` `3 → 4`,
  `SUPPORTED_PROTOCOL_VERSIONS` `[1, 2, 3] → [1, 2, 3, 4]` (a Rust constant, not a
  `.proto`, but the single source of truth both sides read).

## Capabilities

### New Capabilities

- `presence`: the manual online/offline flag — its semantics, the `SetPresence` /
  `PresenceUpdate` frames, durable per-user server storage, the broadcast and
  on-connect snapshot with their visibility scope, the default, and the UI (the
  profile-tab swipe toggle and the avatar dots/rings).

### Modified Capabilities

- `wire-transport`: the one `Messaging.Connect` stream also carries the manual
  presence frames.

## Impact

- **Proto (`proto/`):** the additive messages and oneof arms above; regenerate the
  committed `pb.rs` on both sides.
- **`protocol/`:** the protocol-version bump; one shared place names the Online and
  Offline values so the server and core agree.
- **Server (`server/`):** a durable per-user `manual_presence` column (one SQLite
  migration), a handler for the `SetPresence` frame, the broadcast to connected
  devices of every user sharing a conversation with the setter, and the on-connect
  snapshot. The server **never** sets or derives this flag.
- **Core (`client/`):** send `SetPresence`, apply incoming `PresenceUpdate` (latest
  `updated_at` wins), persist each known user's latest flag, and expose both the
  user's own flag and others' flags to the UI.
- **App (`app/`):** the swipe gesture and green/red ring on the bottom-nav profile
  avatar, and the presence dot/ring on avatars in the chats list, Contacts and
  conversation headers.
- **Depends on / references:** `app-shell` (the bottom-nav profile tab and the
  unified chats list the indicators sit in — a **pending** change, not yet
  archived) and `message-content` (the `Profile` that supplies the avatar). **PR #28
  is concurrently fixing the profile-tab back-navigation; do not assume it is
  merged** — the swipe toggle must coexist with whatever that lands.
