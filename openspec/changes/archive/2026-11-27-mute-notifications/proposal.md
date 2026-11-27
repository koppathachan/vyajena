## Why

Every chat and group on a device gets a wake push and an in-app notification.
The owner asked (2026-11-23) to let a user silence a conversation they do not
want pinged for — a busy group, a specific DM — and to silence the whole app at
once, without losing the messages themselves. Because the server sends the
content-free wake push (message-delivery: Content-free wake pings), the only
place that can decide "do not ping for this one" is the server: it is the only
party that knows a device is offline and is about to be pinged.

The mechanism is the one `remove-closed-dm` just shipped (PR #21): a private
per-user server-side marker, written by an authenticated RPC and delivered to
the user's own devices on connect. Mute is a *personal preference*, not group
state, so it does NOT travel inside the group's encrypted `GroupSettings`; it
rides the same per-user-marker rail as the removed-conversation set.

## What Changes

- **A conversation can be muted, per user.** A user mutes or unmutes a single
  conversation — a DM, the lobby, or a named group — by its `group_id`. Muting
  suppresses the wake push and the in-app notification for that conversation on
  all of the user's devices; the message still delivers and still shows as unread
  in the chat list.
- **The whole app can be muted.** A single global flag silences notifications for
  every conversation. Whole-app mute overrides the per-conversation flags.
- **MVP is on/off only.** No timed mute (1h/8h/until-tomorrow) and no @mention
  exception (see design D3/D4); both are deferred.
- **Server-side private per-user marker (owner mechanism, 2026-11-23).** A new
  authenticated `Prefs.SetMutePref{scope, group_id, muted}` records the preference
  in a private per-user *mute set* (muted conversation ids + a `muted_all` flag).
  On every connect the server delivers the caller's mute set to each of their
  devices as one `ServerFrame.mute_prefs{muted_group_ids, muted_all}` snapshot.
  Before enqueuing a wake push, the server checks the recipient user's mute set
  and skips the push for a muted conversation or when `muted_all` is set.
- **Privacy / invariants:** the server stores conversation ids and the one
  `muted_all` flag only — never message content or history (invariant: the server
  is the authority for routing, ordering and membership, never history). A user's
  mute set is readable and writable only by that user's own devices. The server
  already routes these conversations, so which ones are muted is minor added
  routing metadata, learned by no other user.

## Open decisions resolved

- Mechanism: private per-user server-side mute set, the `remove-closed-dm` marker
  pattern (owner, 2026-11-23).
- Duration: on/off toggle for the MVP; timed mute deferred (owner, 2026-11-23).
- @mention exception: skipped for the MVP (owner, 2026-11-23); see design D4.

## Proto files touched

- **New** `proto/vyajena/v1/prefs.proto`: a `Prefs` service with
  `rpc SetMutePref(SetMutePrefRequest) returns (SetMutePrefResponse)`;
  `SetMutePrefRequest{ MuteScope scope = 1; bytes group_id = 2; bool muted = 3; }`;
  `enum MuteScope { MUTE_SCOPE_UNSPECIFIED = 0; MUTE_SCOPE_CONVERSATION = 1;
  MUTE_SCOPE_GLOBAL = 2; }`. `Prefs` is the shared per-user-preference rail the
  mute-set note anticipated; later per-user prefs (and, eventually, the
  removed-conversation set) can move onto it.
- `proto/vyajena/v1/messaging.proto`: `ServerFrame.mute_prefs = 12`
  (`MutePrefs{ repeated bytes muted_group_ids = 1; bool muted_all = 2; }`). Field
  11 is already `conversation_removed` (`remove-closed-dm`); 12 is the next free
  tag. Unlike `conversation_removed` (one frame per entry), the mute set is one
  snapshot frame, because it carries the global flag alongside the ids.
- Additive: `buf lint`, `buf breaking` pass; `cargo run -p pbgen`. Field numbers
  reconciled with the in-flight changes (`remove-closed-dm` pins
  `conversation_removed = 11`; `notification-reliability` adds no proto;
  `self-hostable` adds none); `buf breaking` catches any clash.

## Capabilities

### New Capabilities

- `notification-prefs`: muting a conversation, muting the whole app, the per-user
  mute-set sync, and the rule that a muted conversation gets no wake push.

### Modified Capabilities

- `app-ui`: mute/unmute affordances on a conversation and a global mute control,
  plus a muted marker on muted rows.

## Impact

- **Proto:** new `prefs.proto`; `ServerFrame.mute_prefs = 12`.
- **Server:** migration `0009_mute_prefs.sql` (`muted_conversations(username,
  group_id)` and a per-user `muted_all` flag, ids and the flag only);
  `Prefs.SetMutePref` (authenticated, idempotent); the mute set sent on connect as
  `mute_prefs`; the wake-push sender consults the recipient user's mute set before
  enqueuing FCM/APNs. Tests
  `notification-prefs/mute-a-conversation`,
  `notification-prefs/mute-preference-sync`,
  `notification-prefs/a-muted-conversation-gets-no-wake-push`.
- **Core:** `Client::set_mute_pref(target, muted)`; applying `mute_prefs`; a local
  view of the mute set so the app suppresses the in-app notification and renders
  the muted marker. The core does NOT itself gate the server push.
- **Bridge/app:** `set_mute_pref`; mute toggles in the app shell (conversation
  overflow + chat-row long-press), a global "mute all" in the Profile tab, a mute
  icon on muted rows.
- **Ops:** server upgrade before the app (an old server answers `UNIMPLEMENTED`;
  the app surfaces it and shows the conversation as unmuted). No existing data
  changes.
