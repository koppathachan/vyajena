## 1. Proto and protocol constants (first)

- [x] 1.1 In `proto/vyajena/v1/messaging.proto` add `enum PresenceStatus`
  (`UNSPECIFIED=0`, `ONLINE=1`, `OFFLINE=2`), `message SetPresence { PresenceStatus
  status = 1; }`, and `message PresenceUpdate { UserId user = 1; PresenceStatus
  status = 2; google.protobuf.Timestamp updated_at = 3; }`.
- [x] 1.2 Add `SetPresence set_presence = 5;` to the `ClientFrame` oneof and
  `PresenceUpdate presence_update = 13;` to the `ServerFrame` oneof (next free tags;
  tag 12 is `mute_prefs`, landed since this change was drafted).
- [x] 1.3 Regenerate the committed `pb.rs` on both sides with the one codegen
  command; confirm no other wire change.
- [x] 1.4 In `protocol/src/wire_transport.rs` set `PROTOCOL_VERSION = 4` and
  `SUPPORTED_PROTOCOL_VERSIONS = [1, 2, 3, 4]`; update the `protocol/tests` asserts.
- [x] 1.5 Add a shared `protocol/` place naming the Online/Offline values and the
  first-launch default (D2) so server and core agree on one constant.

## 2. Server

- [x] 2.1 Migration: add a durable `manual_presence` value per user (users row),
  defaulting to the D2 constant at `Devices.Register`.
- [x] 2.2 Handle `ClientFrame.SetPresence`: write `manual_presence[user]`, stamp
  `updated_at`; reject nothing on value, ignore `UNSPECIFIED`. Never set it from any
  other signal.
- [x] 2.3 Broadcast: on a change, send `PresenceUpdate` to every connected device of
  every **other** user who shares a conversation with the setter (D1), gated to
  streams that negotiated protocol version ≥ 4 (D7).
- [x] 2.4 Snapshot: on each `Connect`, before live traffic, send one `PresenceUpdate`
  (latest value) per user the connecting user may see (D1).
- [x] 2.5 Confirm the server never derives presence from heartbeats/connection, and
  logs ids/status only, never treating it as content.

## 3. Core (client)

- [x] 3.1 Send `SetPresence` when the UI toggles; expose the user's own current flag.
- [x] 3.2 Apply incoming `PresenceUpdate` with latest-`updated_at`-wins (D5); persist
  the latest known flag per user; expose others' flags to the UI (unknown = none).

## 4. App (UI)

- [x] 4.1 Add a sliding toggle (`Switch`) on the Profile screen: on → Online, off →
  Offline (D8, revised from the avatar swipe); drive the bottom-nav profile-tab
  avatar's presence dot from this manual flag (green Online / red Offline), removing
  the earlier connection-driven dot (#40). Must coexist with the profile-tab
  back-nav fix.
- [x] 4.2 Draw a green/red presence dot/ring on avatars in chat-list rows, the
  Contacts tab and conversation headers for users whose flag is visible; draw none
  when unknown.

## 5. Tests

- [x] 5.1 Server test (`server/tests`): `SetPresence` persists; a co-member receives
  `PresenceUpdate`; a non-sharing user does not (D1); a reconnect gets the snapshot;
  v3 stream gets no `PresenceUpdate` (D7); value survives disconnect (D6).
- [x] 5.2 `protocol/tests`: version is 4 and supported is `[1,2,3,4]`.
- [x] 5.3 Core test: latest-wins reconciliation; own-flag round-trip.
- [x] 5.4 App widget tests: the Profile toggle sets each state and recolours the
  profile-tab avatar dot; indicator dots reflect a received `PresenceUpdate`;
  unknown shows no dot.
- [x] 5.5 Each `presence` requirement has at least one test named
  `// spec: presence/<title-slug>`.

## 6. Docs

- [x] 6.1 Record the server-visible manual-presence metadata and the resolved D1/D2
  in `docs/architecture-decisions.md` open-decisions list; note it is manual (no
  activity inference) and not content.
