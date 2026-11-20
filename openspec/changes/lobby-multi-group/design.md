## Context

- **`dms` already made this a multi-group world.** The server keys `mls_group`,
  members, the commit log and queues by `group_id`, with a `kind` column (`main`
  | `dm`) and a partial unique index keeping exactly one `main`. The core holds a
  map of `group_id` to `MlsGroup` and a `chats` table. Routing, inactivity, join
  and re-add already run per group.
- **Three group-context extensions exist.** `0xF0A1` on the main group names the
  admin and drives the main-group commit authority rule; `0xF0A2` on a DM names
  its two users. A core tells a group's kind from which extension it carries.
- **The main group already has a name mechanism.** `GroupSettings` from an admin
  device carries the name; the server never sees it; new devices get it re-sent.
- **Owner decisions (2026-11-20):** admin picks a subset; admin-only commit
  authority in a named group; rename yes and close for "delete"; a new member
  auto-joins lobby only; lobby's name is a fixed "lobby"; protocol version 3 with
  a KeyPackage capability; `ChatKindInfo` becomes `Lobby | Group | Dm`.

## Goals / Non-Goals

**Goals**
- The one group becomes a system lobby: all-members, unrenameable, undeletable.
- The admin creates named groups holding any subset of the members, renames and
  closes them.
- Version 1 and 2 apps keep working and never see a named group.

**Non-goals**
- User-created (non-admin) groups; roles or co-admins; a member leaving a named
  group on their own.
- Nested groups or Discord-style servers.
- Per-chat push, muting, history for new devices.

## Decisions

### D1. The lobby is the existing group, relabelled `system`

- **No new MLS group, no re-key.** The single group the admin created at
  bootstrap keeps its `group_id`, epoch, tree and members. Only its server-side
  `kind` changes from `main` to `system`, and clients start treating it as the
  lobby.
- **It keeps its `0xF0A1` admin extension.** The main-group commit authority rule
  is unchanged for it; it is still the one all-members, admin-run group, now with
  a fixed name.
- **Why not a fresh lobby next to the old group:** the owner was explicit that the
  group they created *is* lobby. A fresh group would orphan the existing history
  and need everyone re-added.

### D2. Lobby's name is a fixed string; `GroupSettings` is ignored for it

- **Clients show the system group as "lobby"**, a built-in constant
  (`protocol::...` for the server-side nothing; the string lives in the app and
  the core's view model). `GroupSettings.name` on the system group is ignored.
- **The admin's device sends no `GroupSettings` for it** and its rename UI is
  hidden.
- **On upgrade the old name is dropped.** The group is currently "Home chat"; the
  app simply stops reading that name for the system group. No migration message
  is sent, and the "group renamed" notice is never shown for lobby.
- **Why client-side, not a one-time rename-then-lock:** a fixed constant needs no
  migration send, no lock-check on `GroupSettings`, and cannot drift; "unchangeable
  by anyone" is then true by construction (the owner chose this, 2026-11-20).

### D3. A named group is an admin-created MLS group with extension `0xF0A3`

- **Creation mirrors `Admin.CreateGroup` / `Dms.CreateDm`.** The admin's device
  builds an epoch-0 MLS group whose only member is itself, whose group context
  lists the server as external sender and carries:
  - the `0xF0A1` admin extension naming the admin (so the admin commit authority
    rule applies, exactly as for the main group), and
  - a new **`0xF0A3` named-group extension**, the marker that gates the group
    behind protocol version 3 and tells a core "this is a named group, not the
    lobby".
- **`Admin.CreateNamedGroup`** takes the `group_id`, `group_info` and
  `ratchet_tree`, like `CreateGroup`. The server checks the server is the external
  sender, the context names the admin in `0xF0A1`, carries `0xF0A3`, and the
  caller's device is the only member at epoch 0, and records the group with kind
  `named`. A non-admin caller is rejected with `PERMISSION_DENIED`, as every
  `Admin` RPC is.
- **The name** travels as a `GroupSettings` body from the admin, exactly as the
  main group's name does today. Until the admin sends one, members show a
  placeholder name ("Unnamed group").
- **Why `0xF0A1` *and* `0xF0A3`:** `0xF0A1` reuses the tested admin commit
  authority rule unchanged; `0xF0A3` is the capability a KeyPackage must list, so
  the version gate is the KeyPackage itself (as in `dms` D5), not a flag the
  server could get wrong.
- **Code touch points (found 2026-11-20 by reading `client/src/mls/group.rs` and
  `client/src/engine/mod.rs`):**
  - `mls_group::capabilities()` lists only `0xF0A1`, `0xF0A2`, `LastResort`. A v3
    core SHALL add `0xF0A3`. Because every member's leaf must advertise every
    group-context extension, OpenMLS then refuses to *add* a pre-v3 KeyPackage to a
    named group at commit time — the admin's commit fails before any Welcome
    exists. Spike S1 confirmed this with the bare `0xF0A3` context extension
    (`InsufficientCapabilities`); a `RequiredCapabilities{0xF0A3}` extension is
    optional belt-and-suspenders, not required.
  - Classification is 2-way today: `mls_group::is_dm()` (is `0xF0A2` present?) and
    everything else is treated as the main group. The v3 core SHALL classify
    3-way — main/system (`0xF0A1`, no `0xF0A3`), DM (`0xF0A2`), named (`0xF0A3`) —
    and the `on_welcome` non-DM branch (`client/src/engine/mod.rs:1497`), which
    joins with `replace_old_group()` into the single main group, SHALL take the
    named path for a `0xF0A3` group instead of falling through to main.

### D4. Membership: the admin picks any subset and is the sole moderator

- **The admin adds members by committing Adds** (admin commit authority), after
  `CreateNamedGroup`, fetching their KeyPackages with
  `FetchKeyPackages(group_id = the named group)`.
- **The admin removes a member** by committing a Remove of that member's devices
  in the group; the member stays in lobby and in their own DMs.
- **No one else changes membership.** A non-admin device commits only its own
  leaf-update, Adds/Removes of its own user's other devices (multi-device), and
  server inactivity Removes — the main-group rule verbatim.
- **A new server member is not auto-added** to any named group; only lobby's
  "Automatic add of new users" runs for them.
- **Why no member-initiated leave:** out of scope this change; a member who wants
  out asks the admin. Keeping the main-group rule unchanged means no new
  commit-authority branch to test.

### D5. Named-group commit authority (invariant change)

- **In a named group the server SHALL accept a commit from the admin's device
  with any proposals**, and from any other member device only: an update of its
  own leaf, an Add of a device of its own user, a Remove of a device of its own
  user, or a server-issued external Remove.
- This is the main-group rule, parameterised by the group's admin (named in
  `0xF0A3` / `0xF0A1`, which for every group on this server is the one configured
  admin).
- **Who enforces:** the server and every client, as for the main group and DMs.
- **The main-group and DM rules do not change.**

### D6. Closing a named group reuses the DM close

- **`Admin.CloseGroup(group_id)`** (admin only) sets `closed_at`, deletes the
  group's queued entries and public state, and rejects later envelopes for it with
  `REJECTION_REASON_GROUP_CLOSED` — the same server behaviour as `RemoveUser`
  closing a DM (`dms` D7).
- **`GroupClosed{group_id, closed_at}`** is a new `ServerFrame`, sent to each of
  the group's member devices on every connect until that device sends
  `Ack(group_id, server_seq = 0)` for it — the durable-without-a-queue mechanism
  `DmClosed` uses.
- **The core** marks the chat closed; the app shows it read-only with "This group
  was closed". No one commits in a closed group.
- **Lobby cannot be closed:** `CloseGroup` on the system group is rejected with
  `FAILED_PRECONDITION`.
- **Why a new frame, not `DmClosed`:** `DmClosed` is defined for DMs and the app
  shows a DM-specific banner ("Bob was removed"); a sibling `GroupClosed` keeps
  each path readable while sharing the ack mechanism.

### D7. Protocol version 3 keeps named groups off old apps

- **The danger is the same as `dms` D5, one layer up.** A version 1 core holds one
  group and would overwrite it. A version 2 core holds many groups but knows only
  the main (`0xF0A1`) and DM (`0xF0A2`) kinds; given a named group's Welcome it
  must refuse cleanly, not corrupt lobby or a DM.
- **Spike S1 — done 2026-11-21, `spikes/named-welcome/` (OpenMLS 0.9.2, deleted
  after):** the lobby-clobber risk is closed at the protocol level, *before* the
  Welcome stage.
  - A current core's leaf capabilities omit `0xF0A3`. When the admin commits an Add
    of such a KeyPackage to a named group (context carries `0xF0A3`), OpenMLS
    rejects the Add at the committer with
    `CreateCommitError(ProposalValidationError(InsufficientCapabilities))`. **The
    admin's own core cannot build the commit**, so no named-group Welcome for a
    pre-v3 device is ever produced, and the dangerous `on_welcome` path
    (`client/src/engine/mod.rs:1460-1552`, which joins with `replace_old_group()`
    into the single main group) is never reached.
  - **`RequiredCapabilities{0xF0A3}` is not needed for this:** the spike got the
    same refusal with only the `Unknown(0xF0A3)` group-context extension present
    (OpenMLS requires every member's leaf to advertise every group-context
    extension in use). Adding `RequiredCapabilities{0xF0A3}` is harmless
    belt-and-suspenders; the implementation MAY include it but need not.
  - **Control:** a v3 KeyPackage that lists `0xF0A3` is added normally.
  - **Implication:** the safety guarantee is identical to the DM mechanism (`dms`
    D5) and does not depend on the server routing gate or on OpenMLS's
    Welcome-time check. The v3 core's 3-way classifier (main / DM / named) is still
    needed for *correctness* — so a v3 core routes a named group to the named path
    rather than treating it as main — but a current core is safe by capabilities
    alone.
- **The rules (mirroring `dms` D5):**
  - `PROTOCOL_VERSION = 3`; the server supports `[1, 2, 3]`; all behave the same
    on the stream. Version 3 in `GetServerInfo` tells the app the server knows
    named groups.
  - A version 3 core lists `0xF0A3` in every KeyPackage and leaf (alongside
    `0xF0A2` from `dms`). On its first version-3 run it uploads a fresh set,
    last-resort included.
  - The server stores whether each KeyPackage lists `0xF0A3` (a second capability
    flag next to `dm_capable`).
  - `FetchKeyPackages(group_id = named)` returns only `0xF0A3`-listing KeyPackages
    of the members, and a named-group `COMMIT` that adds a leaf without `0xF0A3`
    is rejected with `POLICY_VIOLATION`. So every named-group member is a version
    3 device and nothing of a named group reaches a version 1 or 2 device.
- **On upgrade:** the core's first version-3 upload makes the device named-group
  capable; the admin can then add it.

### D8. Removing a user spans every group

- **`Admin.RemoveUser(bob)`** already revokes Bob's devices and closes his DMs.
  Now the admin also commits Removes of Bob's devices in lobby and in every named
  group Bob is in, before the call, exactly as the existing "Removing a user"
  requirement says for "the group" — now read across all groups.
- A revoked device cannot authenticate, so even an un-removed leaf is harmless; the
  Removes keep the trees correct.

### D9. The server store

- **Migration `000N_named_groups.sql`:**
  - `mls_group.kind` gains `system` and `named`; the existing single `main` row
    becomes `system` (one statement). The partial unique index becomes
    `WHERE kind = 'system'`.
  - `closed_at` already exists for DMs (`dms`); named groups reuse it.
  - `key_packages` gains a `named_group_capable` flag, set on upload from the
    KeyPackage's capabilities, mirroring `dm_capable`.
- **No new per-group member table:** membership is the MLS tree, as for every
  group. A named group's name is never stored (it is encrypted content).
- **Spike S2 — done 2026-11-20, by reading `server/src/store/migrations/0005_groups.sql`:**
  the `dms` store has no `main | dm`-only assumption beyond a handful of named
  spots, all in the migration layer:
  - `mls_groups.kind CHECK (kind IN ('main', 'dm'))` → gains `'named'`, and the
    existing `'main'` row becomes `'system'`.
  - the `one_main_group` partial unique index `WHERE kind = 'main'` → `WHERE kind =
    'system'`.
  - the three CHECKs tying `dm_user_a`/`closed_at` to `kind` assume non-DM ⟺ main;
    they become: `dm_user_a` set ⟺ `kind = 'dm'`, and `closed_at` allowed for
    `'dm'` or `'named'`.
  - `key_packages.dm_capable` → add a sibling `named_group_capable` column.
  The group-scoped delivery tables (`envelopes`, `deliveries`, `cursors`, `gaps`,
  `pi_removes`, `mls_public_state`, `mls_members`) are all keyed by `group_id`, not
  by `kind`, so named groups ride them unchanged.

### D10. The core and bridge

- **Store:** the `chats.kind` gains `named`; a `name` column holds a named group's
  last-known name (from `GroupSettings`); `closed_at` already exists.
- **Engine:** named groups ride the existing per-group map, routing, outbox,
  rotation, acks and gap notices; the only new branches are the commit-authority
  selector (`0xF0A3` → admin rule) and the Welcome check (accept a named group's
  Welcome when its context carries `0xF0A1` + `0xF0A3` and the server is the
  external sender).
- **API:** `create_named_group() -> ChatId` and `close_group(chat)` on the admin
  path; `chats()` returns the name and kind; the member set per chat for the
  admin's picker.
- **Bridge:** `ChatKindInfo` becomes `Lobby | Group{ name, closed } |
  Dm{ peer, waiting, closed }`; `ChatSummary` carries the name for a `Group`.

### D11. The app

- **Chat list:** lobby pinned first (labelled "lobby"), then the admin's named
  groups and the DMs by newest message, closed ones greyed at the bottom. Each
  with the unread dot and `@ N` badge, as `dms` set up.
- **Admin screens:** "Create group" opens a member picker over the server's
  members; on confirm the app calls `create_named_group` then adds the picked
  members. A named group the admin is in offers "Rename" (a `GroupSettings`) and
  "Close group" (`close_group`, with a confirm).
- **Named-group screen:** the chat screen titled with the group's name, with the
  admin's add/remove-member and rename actions on the admin's devices only, and
  `@everyone` meaning everyone in that group. A closed group shows the read-only
  banner.

### D12. Nothing new leaves the device

- A named group's name and messages travel only inside MLS application messages.
- **What the server learns**, beyond what it already sees per group (sizes,
  timing, members): that a group is a named group rather than a DM, and when it
  was closed. It never learns the name.

```mermaid
sequenceDiagram
    participant Ad as Admin's phone (v3)
    participant S as server
    participant B as Bob's phone (v3)
    participant C as Carol's phone (v2)
    Ad->>Ad: new MLS group, ext 0xF0A1{admin} + 0xF0A3, server as external sender
    Ad->>S: Admin.CreateNamedGroup(group_id, GroupInfo, tree)
    S-->>Ad: ok (kind=named, epoch 0)
    Ad->>S: Keys.FetchKeyPackages(bob, group_id=named)
    S-->>Ad: Bob's phone (v3) only
    Ad->>S: COMMIT Add(Bob's phone) + Welcome ; then GroupSettings "Weekend"
    S-->>Ad: accepted
    S->>B: WELCOME (named) ; then the name
    Note over C: v2 — never offered, never routed a named group
    Ad->>S: Admin.CloseGroup(group_id)
    S->>B: GroupClosed (until Ack server_seq=0)
```

## Spikes (before code that relies on them)

- **S1 — done 2026-11-21 (`spikes/named-welcome/`, deleted), result in D7:**
  OpenMLS 0.9.2 refuses to *add* a pre-v3 leaf (capabilities omit `0xF0A3`) to a
  named group with `InsufficientCapabilities` — the admin cannot build the commit,
  so no named Welcome ever reaches a current core and the lobby cannot be clobbered.
  The bare `0xF0A3` context extension suffices; `RequiredCapabilities` is optional.
- **S2 — done 2026-11-20 (by reading the migrations), result in D9:** the store's
  only `main | dm` spots are the `mls_groups.kind` CHECKs, the `one_main_group`
  index and `key_packages.dm_capable`; delivery tables are keyed by `group_id`.

## Risks / Trade-offs

- **KeyPackage use grows again.** A device now joins up to one group per named
  group it is in, on top of lobby and its DMs. Replenishment and the last-resort
  KeyPackage cover bursts; watch `LowKeyPackages` in a test with several named
  groups.
- **A third version to keep straight.** `[1, 2, 3]` all share the stream; only the
  KeyPackage capabilities and the `GetServerInfo` list differ. Tested in
  `wire-transport/protocol-version-negotiation`.
- **Metadata:** the server sees the named-group membership and timing, as it sees
  the main group's. No new class of leak.
- **Server first, app second:** a version 3 app needs a server that lists version
  3; the app hides "Create group" otherwise.

## Migration Plan

1. Deploy the server (backs up first). Version 1 and 2 apps keep working, in
   lobby and DMs.
2. Release the app (owner step).
3. Devices that update become named-group capable on their first version-3
   upload; the admin can then add them to named groups.

**Rollback:** the server rollback restores the backup. Named groups created since
are lost; lobby and DMs are unaffected.

## Open Questions (owner)

1. **Unnamed-group placeholder:** is "Unnamed group" the right label before the
   admin sets a name, or should the admin be required to name it at creation?
2. **Member-initiated leave** of a named group: a later change, or never?
3. **Co-admins / roles** in a named group: Future, as the doc already sketches?
4. **A closed group's history:** kept read-only forever on the device, as a closed
   DM is — or offer the member a "delete this chat" (local only)?
