## Why

Everything today goes to the whole group. Two members cannot talk on their
own. The architecture doc lists DMs as the first "Future" item ("2-member MLS
groups"). The owner asked for a DM proposal on 2026-11-17, right after
mentions and replies.

## What Changes

- **A DM is its own MLS group** with all devices of exactly two users,
  next to the one main group. Same encryption, same Pi, same delivery
  rules.
- **Who:** any member may open a DM with any other member, with no approval
  (owner, 2026-11-17).
- **Admin:** has no power over other people's DMs. The admin's devices are
  not in them, and the admin cannot see, read or close them. Removing a user
  from the server closes every DM of that user (owner, 2026-11-17). The
  other side keeps its history, read-only.
- **Pi metadata:** the Pi knows which two users share a DM, because it
  routes it. It never sees the text (owner accepted, 2026-11-17).
- **No block** in this change (owner, 2026-11-17).
- **Protocol version 2:**
  - An old (version 1) app would replace its main group with any second
    group it is welcomed to (design D7), so DMs only reach devices that
    speak version 2.
  - The Pi accepts both versions and never puts a version 1 device into a
    DM.
  - A device that upgrades is added to its DMs automatically.
- **New devices and returning devices** join their user's DMs
  automatically, the same way they join the main group today
  (`JoinNotice`, `RejoinNotice` per group).
- **App:**
  - The home screen becomes a chat list: the group first, then DMs, each
    with unread and `@` badges.
  - "Message" on a member opens the DM, or creates it on first use.
  - A DM screen is the chat screen without admin actions and without
    `@everyone`.
  - A closed DM shows a read-only banner.
- **Push:** unchanged: content-free, exactly "New message" (push
  invariant). It doesn't say which chat.
- **Out of scope:** blocking, muting one chat (not possible with
  content-free pushes, design D11), leaving a DM, group chats beyond the
  main group, history for newly added devices.
- Details and decisions: `design.md`.

## Open decisions resolved

- Who may DM whom: any member, any member (owner, 2026-11-17).
- Pi knowing DM pairs: accepted (owner, 2026-11-17).
- Admin power over DMs: none; removing a user closes their DMs (owner,
  2026-11-17).
- Blocking: not in this change (owner, 2026-11-17).
- DM group id: random, not "hash of the sorted user pair" as the
  architecture doc sketched (design D2).

## Invariants changed (approved by the owner, 2026-11-17)

- **Commit authority** gains a DM rule (design D4). The main-group rule does
  not change. `docs/architecture-decisions.md` and
  `openspec/config.yaml` context are updated in tasks.
- **"One admin-run group"** becomes "one admin-run group plus DMs between its
  members" in the doc, config context and CLAUDE.md.

## Proto files touched

- `proto/vyajena/v1/dms.proto` (new): service `Dms`, `CreateDm`.
- `proto/vyajena/v1/messaging.proto`: `ServerFrame.dm_closed` (`DmClosed`).
- `proto/vyajena/v1/common.proto`: `REJECTION_REASON_GROUP_CLOSED`.
- `proto/vyajena/v1/keys.proto`: `FetchKeyPackagesRequest.group_id`.
- `protocol/`: `PROTOCOL_VERSION = 2`, supported versions `[1, 2]`, the DM
  group-context extension, the DM commit authority rule.
- All additive: `buf breaking` passes.

## Capabilities

### New Capabilities

- `direct-messages`: opening a DM, who is in it, closing it, and how the
  core and the Pi keep several groups.

### Modified Capabilities

- `group-membership`: group creation, commit authority, membership state
  on the Pi, automatic add and re-add now per group.
- `wire-transport`: protocol version 2 next to 1.
- `key-directory`: KeyPackage fetch for a DM returns only version 2
  devices.
- `app-ui`: chat list, "Message" on a member, DM screen.

## Impact

- **Pi (`server/`):**
  - A store migration: many groups and their kind; the DM pair; members,
    public state, commit log and queues keyed by group; each device's last
    protocol version.
  - New `dms/` module.
  - Routing, inactivity, join and re-add work per group.
- **Core (`client/`):**
  - A store migration: chats; messages per chat; MLS state per group.
  - The engine handles several groups.
  - Per-chat API and events.
- **Bridge and app:** chat ids everywhere; chat list and DM screens.
- **Ops:** Pi upgrade before the app update. The Pi upgrade is backwards
  compatible (version 1 apps keep working, in the main group only).
- **Depends on:** `mentions` (PR #4) merged first. The app code builds on it.
