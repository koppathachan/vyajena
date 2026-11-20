## 0. Before starting

- [ ] 0.1 `dms` applied and merged into `main` (multi-group store and engine,
      protocol version 2)
- [ ] 0.2 Owner approves the invariant changes (named-group commit authority;
      "a system lobby and admin-created named groups, plus DMs"): approved
      2026-11-20

## 1. Spikes

- [x] 1.1 S1: OpenMLS refuses to add a pre-v3 leaf (no `0xF0A3`) to a named group
      (`InsufficientCapabilities`), so no named Welcome ever reaches a current core
      and the lobby cannot be clobbered; result in design D7 (done 2026-11-21)
- [x] 1.2 S2: read the `dms` multi-group store for any `main | dm`-only
      assumption beyond the commit-authority selector and the one-`system`-group
      index; result in design D9 (done 2026-11-20)

## 2. Proto and shared rules

- [x] 2.1 `admin.proto`: `Admin.CreateNamedGroup`, `Admin.CloseGroup`;
      `messaging.proto`: `ServerFrame.group_closed` (`GroupClosed`); `buf lint`,
      `buf breaking`; `cargo run -p pbgen`
- [x] 2.2 `protocol`: `PROTOCOL_VERSION = 3`, supported `[1, 2, 3]`; named-group
      extension `0xF0A3` encode/parse; named-group commit authority rule; the
      group kind `system | named | dm`; tests
      `group-membership/commit-authority`, `multi-group/named-group-creation`

## 3. Server

- [x] 3.1 Store migration: `mls_group.kind` gains `system` (the existing `main`
      row becomes it) and `named`; the partial unique index becomes
      `WHERE kind = 'system'`; `key_packages.named_group_capable`; migration test
      from a `dms`-era database
- [x] 3.2 `Hello` accepts 1, 2 and 3; `named_group_capable` recorded on upload;
      nothing of a named group for a pre-version-3 device; tests
      `multi-group/named-groups-reach-only-v3-apps`,
      `wire-transport/protocol-version-negotiation`
- [x] 3.3 `CreateNamedGroup` checks (external sender, `0xF0A1` names the admin,
      `0xF0A3` present, caller-only at epoch 0, admin-only); tests
      `multi-group/named-group-creation`, `group-membership/group-creation`
- [x] 3.4 Commit checks per group kind; named-group Adds only of `0xF0A3`-listing
      leaves; tests `group-membership/commit-authority`,
      `multi-group/named-group-membership`
- [x] 3.5 `FetchKeyPackages` with a named `group_id`; tests
      `key-directory/keypackage-fetch`
- [x] 3.6 `CloseGroup`: `closed_at`, `GroupClosed` until acked, `GROUP_CLOSED`;
      reject on the system group with `FAILED_PRECONDITION`; tests
      `multi-group/closing-a-named-group`
- [x] 3.7 `JoinNotice` / `RejoinNotice` unchanged per group; the system group is
      the auto-add target; `RemoveUser` Removes across every group; tests
      `group-membership/automatic-add-of-new-users`,
      `group-membership/removing-a-user`
- [x] 3.8 `membership-state-on-the-server` test covering kind `system | named |
      dm`; the push ping unchanged for named-group traffic

## 4. Core

- [x] 4.1 Store migration: `chats.kind` gains `named`, a `name` column; the
      existing group becomes the `system` chat; existing data kept
- [x] 4.2 KeyPackages and leaves list `0xF0A3`, fresh upload on first version-3
      run; speaks version 2 / 1 to an older server; Welcome and commit checks for
      a named group; tests `multi-group/named-groups-reach-only-v3-apps`
- [x] 4.3 Engine: named groups under the existing per-group map, routing,
      rotation, acks; the system group shows the fixed name "lobby" and ignores
      `GroupSettings`; a named group tracks its `GroupSettings` name; `GroupClosed`
      handling; tests `multi-group/the-system-lobby`,
      `message-content/group-settings-travel-encrypted`
- [x] 4.4 Per-chat API: `create_named_group`, `close_group`, the member set per
      chat, kind and name on `chats`; events carry the `ChatId`; tests
      `multi-group/named-group-creation`, `multi-group/closing-a-named-group`

## 5. Bridge

- [x] 5.1 `ChatKindInfo` becomes `Lobby | Group{name, closed} | Dm{...}`;
      `ChatSummary` name for a `Group`; `create_named_group`, `close_group`;
      `flutter_rust_bridge_codegen generate`; bridge end-to-end test

## 6. App

- [x] 6.1 Chat list: lobby first (labelled "lobby"), named groups, DMs; widget
      tests `app-ui/chat-list`
- [x] 6.2 Admin "Create group" with a member picker, "Rename", "Close group" with
      a confirm; shown only on the admin's devices and only when the server lists
      version 3; widget tests `app-ui/admin-group-screens`
- [x] 6.3 Named-group screen: name title, admin actions, `@everyone` scoped to the
      group, closed banner; widget tests `app-ui/named-group-screen`

## 7. Docs and check

- [x] 7.1 `docs/architecture-decisions.md` (Scope → system lobby + named groups;
      commit authority; protocol v3; named groups out of Future),
      `openspec/config.yaml` context, `CLAUDE.md` first line
- [ ] 7.2 `cargo test --workspace`, clippy, fmt, `buf lint`, `buf breaking`,
      `flutter analyze`, `flutter test`, `openspec validate lobby-multi-group
      --strict`
