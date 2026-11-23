## Why

Today the server holds exactly one group plus DMs. That one group is the group
the admin created at first server start; the architecture doc lists "more
groups, user-created groups" and "then Discord-style servers; admin-only
creation at first" as Future. The owner asked (2026-11-20) for the first step of
that: turn the one group into a **system lobby** that every member is always in
and whose name no one can change, and let the admin create **real named groups**
holding any subset of the members, next to lobby and DMs.

This builds directly on `dms`, which already made the server and the core hold
many MLS groups keyed by `group_id`. Named groups reuse that plumbing; lobby is
the existing group, relabelled.

## What Changes

- **The one group becomes the system lobby.**
  - It is the group created at bootstrap with `Admin.CreateGroup` (now recorded
    with kind `system`).
  - Every member of the server is a member of it, kept there automatically, as
    today's "Automatic add of new users" already does.
  - **Its name is the fixed string "lobby".** Clients ignore any
    `GroupSettings.name` for it; no one, admin included, can rename it. The old
    stored name ("Home chat") is dropped on upgrade (owner, 2026-11-20).
  - It can never be closed.
- **The admin can create named groups** with `Admin.CreateNamedGroup`, alongside
  lobby and DMs.
  - **Membership is any subset** the admin chooses (owner, 2026-11-20). The admin
    creates the epoch-0 group with only its own device and commits Adds for the
    members it picks.
  - **Commit authority is the main-group (admin) rule** (owner, 2026-11-20): the
    admin's devices commit anything; any other device commits only its own
    leaf-update, Adds/Removes of its own user's devices, and server inactivity
    Removes. The admin is the sole moderator; it adds and removes members at any
    time.
  - **A new member auto-joins lobby only** (owner, 2026-11-20), never existing
    named groups. The admin adds them to a named group by choice.
  - **The admin can rename a named group** (a `GroupSettings` name, as the main
    group used to take) and **close it** (owner, 2026-11-20): closing mirrors the
    DM close built in `dms` — the server marks it closed, stops routing, and
    clients keep its history read-only.
- **Protocol version 3** (owner, 2026-11-20):
  - A named group carries its own group-context extension `0xF0A3`, so a version
    1 or 2 core — which does not know named groups — is never put into one, the
    same way `dms` keeps version 1 cores out of DMs (design D5 there).
  - A version 3 core lists `0xF0A3` in its KeyPackages; the server hands out only
    such KeyPackages for a named group and never routes a named group to a
    pre-version-3 device.
  - A device that upgrades uploads new KeyPackages and can then be added to named
    groups.
- **App:**
  - The chat list gains the admin's named groups between lobby (pinned first) and
    the DMs; lobby is labelled "lobby", a named group by its name.
  - The admin's screens gain "Create group" with a member picker, and "Close
    group" on a named group.
  - A closed named group shows a read-only banner, as a closed DM does.
- **Out of scope:** user-created (non-admin) groups; per-member roles or
  co-admins in a named group; a member leaving a named group on their own;
  nested groups / Discord-style servers; muting one chat; history for newly added
  devices (unchanged from `dms`). Push stays content-free, exactly "New message".
- Details and decisions: `design.md`.

## Open decisions resolved (owner, 2026-11-20)

- **Named-group membership:** the admin picks any subset; not all-members.
- **Named-group commit authority:** the main-group admin rule, reused unchanged
  and parameterised by the group's admin (the one configured server admin). No
  member-adds-anyone, no member-initiated leave in this change.
- **Rename / delete:** rename yes (a `GroupSettings` name); "delete" is a close
  that mirrors the DM close.
- **Auto-join:** a new member joins lobby only; named groups are opt-in by the
  admin.
- **Lobby's name on upgrade:** fixed client-side "lobby"; `GroupSettings.name` is
  ignored for the system group and the old name is dropped.
- **Old-app safety:** protocol version 3 plus a KeyPackage capability
  (`0xF0A3`), mirroring the DM mechanism, not a reuse of the main extension.
- **Client model:** `ChatKindInfo` becomes `Lobby | Group{name, closed} |
  Dm{peer, waiting, closed}`; the server stores a group kind `system | named |
  dm`.

## Invariants changed (owner-approved, 2026-11-20)

- **Commit authority** gains a named-group rule: in a named group the same admin
  rule as the main group applies, using the group's admin. The main-group and DM
  rules do not change. `docs/architecture-decisions.md` and
  `openspec/config.yaml` context are updated in tasks.
- **Scope** "one admin-run group plus DMs" becomes "a system lobby and
  admin-created named groups, plus DMs" in the doc, config context and
  `CLAUDE.md`.

## Proto files touched

- `proto/vyajena/v1/admin.proto`: new `Admin.CreateNamedGroup` and
  `Admin.CloseGroup`.
- `proto/vyajena/v1/messaging.proto`: new `ServerFrame.group_closed`
  (`GroupClosed`), reusing the `Ack(server_seq = 0)` acknowledgement the
  `DmClosed` notice already uses.
- `proto/vyajena/v1/common.proto`: `REJECTION_REASON_GROUP_CLOSED` is reused for a
  closed named group (no change).
- `proto/vyajena/v1/keys.proto`: `FetchKeyPackagesRequest.group_id` is reused for a
  named group (no change; its meaning widens, in the spec only).
- `proto/vyajena/v1/content.proto`: `GroupSettings` is reused for a named group's
  name (no change).
- `protocol/`: `PROTOCOL_VERSION = 3`, supported versions `[1, 2, 3]`; the
  named-group group-context extension `0xF0A3`; the named-group commit authority
  rule; the group kind `system | named | dm`.
- All additive: `buf breaking` passes.

## Capabilities

### New Capabilities

- `multi-group`: the system lobby, admin-created named groups, their creation,
  membership, naming and closing, and how named groups stay off old apps.

### Modified Capabilities

- `group-membership`: group creation (the one group is the system lobby), commit
  authority (a named-group rule), membership state on the server (a group kind),
  automatic add of new users (into the system lobby), removing a user (across
  every group).
- `wire-transport`: protocol version 3 next to 1 and 2.
- `key-directory`: KeyPackage fetch for a named group returns only KeyPackages
  that list `0xF0A3`.
- `message-content`: the system group's name is fixed; a named group's name
  travels as `GroupSettings` like the main group's did.
- `app-ui`: named groups in the chat list, admin group screens, the named-group
  screen.

## Impact

- **Server (`server/`):**
  - A store migration: the group kind gains `system` (the existing `main` row
    becomes it) and `named`; a `closed_at` for named groups; a partial unique
    index still keeps exactly one `system` group.
  - `admin/`: `CreateNamedGroup` and `CloseGroup`.
  - Routing, inactivity, join and re-add already run per group (`dms`); named
    groups fall under them.
- **Core (`client/`):**
  - The `chats` table gains the `named` kind and a group name; the engine already
    holds a map of groups.
  - KeyPackages and leaves list `0xF0A3`; a fresh upload on the first version-3
    run.
  - Per-chat API gains the named-group name, create/close, and the member set.
- **Bridge and app:** `ChatKindInfo` gains `Lobby` and `Group`; chat list, admin
  create-group and close-group screens, named-group screen.
- **Ops:** server upgrade before the app update; the upgrade is backwards
  compatible (version 1 and 2 apps keep working, without named groups). A backup
  is taken before the migration.
- **Depends on:** `dms` applied and merged first. Server and core build on its
  multi-group store and engine, and on protocol version 2.
