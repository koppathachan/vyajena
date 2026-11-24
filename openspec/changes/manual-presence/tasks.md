## 1. Proto and protocol constants (first)

- [ ] 1.1 In `proto/vyajena/v1/messaging.proto` add `enum PresenceStatus`
  (`UNSPECIFIED=0`, `ONLINE=1`, `OFFLINE=2`), `message SetPresence { PresenceStatus
  status = 1; }`, and `message PresenceUpdate { UserId user = 1; PresenceStatus
  status = 2; google.protobuf.Timestamp updated_at = 3; }`.
- [ ] 1.2 Add `SetPresence set_presence = 5;` to the `ClientFrame` oneof and
  `PresenceUpdate presence_update = 12;` to the `ServerFrame` oneof (next free tags).
- [ ] 1.3 Regenerate the committed `pb.rs` on both sides with the one codegen
  command; confirm no other wire change.
- [ ] 1.4 In `protocol/src/wire_transport.rs` set `PROTOCOL_VERSION = 4` and
  `SUPPORTED_PROTOCOL_VERSIONS = [1, 2, 3, 4]`; update the `protocol/tests` asserts.
- [ ] 1.5 Add a shared `protocol/` place naming the Online/Offline values and the
  first-launch default (D2) so server and core agree on one constant.

## 2. Server

- [ ] 2.1 Migration: add a durable `manual_presence` value per user (users row),
  defaulting to the D2 constant at `Devices.Register`.
- [ ] 2.2 Handle `ClientFrame.SetPresence`: write `manual_presence[user]`, stamp
  `updated_at`; reject nothing on value, ignore `UNSPECIFIED`. Never set it from any
  other signal.
- [ ] 2.3 Broadcast: on a change, send `PresenceUpdate` to every connected device of
  every **other** user who shares a conversation with the setter (D1), gated to
  streams that negotiated protocol version ≥ 4 (D7).
- [ ] 2.4 Snapshot: on each `Connect`, before live traffic, send one `PresenceUpdate`
  (latest value) per user the connecting user may see (D1).
- [ ] 2.5 Confirm the server never derives presence from heartbeats/connection, and
  logs ids/status only, never treating it as content.

## 3. Core (client)

- [ ] 3.1 Send `SetPresence` when the UI toggles; expose the user's own current flag.
- [ ] 3.2 Apply incoming `PresenceUpdate` with latest-`updated_at`-wins (D5); persist
  the latest known flag per user; expose others' flags to the UI (unknown = none).

## 4. App (UI)

- [ ] 4.1 Make the bottom-nav profile-tab avatar swipable: right → Online, left →
  Offline (D8); render the oval shadow/ring green (Online) or red (Offline); add a
  confirm label/haptic. Must coexist with PR #28's profile-tab back-nav fix.
- [ ] 4.2 Draw a green/red presence dot/ring on avatars in chat-list rows, the
  Contacts tab and conversation headers for users whose flag is visible; draw none
  when unknown.

## 5. Tests

- [ ] 5.1 Server test (`server/tests`): `SetPresence` persists; a co-member receives
  `PresenceUpdate`; a non-sharing user does not (D1); a reconnect gets the snapshot;
  v3 stream gets no `PresenceUpdate` (D7); value survives disconnect (D6).
- [ ] 5.2 `protocol/tests`: version is 4 and supported is `[1,2,3,4]`.
- [ ] 5.3 Core test: latest-wins reconciliation; own-flag round-trip.
- [ ] 5.4 App widget tests: swipe sets each state and recolours the ring; indicator
  dots reflect a received `PresenceUpdate`; unknown shows no dot.
- [ ] 5.5 Each `presence` requirement has at least one test named
  `// spec: presence/<title-slug>`.

## 6. Docs

- [ ] 6.1 Record the server-visible manual-presence metadata and the resolved D1/D2
  in `docs/architecture-decisions.md` open-decisions list; note it is manual (no
  activity inference) and not content.
