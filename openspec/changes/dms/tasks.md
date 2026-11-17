## 0. Before starting

- [x] 0.1 `mentions` (PR #4) merged into `main` (050e355)
- [x] 0.2 Owner approves the invariant changes (DM commit authority; "one group plus DMs"): approved 2026-11-17

## 1. Spikes

- [x] 1.1 S1: a version 1 core given a second group's Welcome (expect: main group replaced); result in design D5
- [x] 1.2 S2: two `PublicGroup`s in one Pi OpenMLS storage, two `MlsGroup`s in one core storage, no key clash; result in design D8

## 2. Proto and shared rules

- [x] 2.1 `dms.proto` (`Dms.CreateDm`), `ServerFrame.dm_closed` (`DmClosed`), `REJECTION_REASON_GROUP_CLOSED`, `FetchKeyPackagesRequest.group_id`; `buf lint`, `buf breaking`; `cargo run -p pbgen`
- [x] 2.2 `protocol`: `PROTOCOL_VERSION = 2`, supported `[1, 2]`; DM extension `0xF0A2` encode/parse; DM commit authority rule; tests `group-membership/commit-authority`, `direct-messages/dm-members`

## 3. Pi

- [x] 3.1 Store migration: many groups (`kind`, pair, `closed_at`, one `main`), `group_id` on members, commit log and queues, `key_packages.dm_capable`; migration test from a version-1 database
- [x] 3.2 `Hello` accepts 1 and 2; `dm_capable` recorded on upload; routing, acks, expiry and gap notices per group; nothing of a DM for version 1 devices; tests `direct-messages/old-apps-never-join-a-dm`, `wire-transport/protocol-version-negotiation`
- [x] 3.3 `dms/` module: `CreateDm` checks; tests `direct-messages/opening-a-dm`, `group-membership/group-creation`
- [x] 3.4 Commit checks per group kind; DM Adds only of the pair's leaves listing `0xF0A2`; tests `group-membership/commit-authority`, `direct-messages/dm-members`
- [x] 3.5 `FetchKeyPackages` with `group_id`; tests `key-directory/keypackage-fetch`
- [x] 3.6 `JoinNotice` and `RejoinNotice` per DM; inactivity across groups; tests `direct-messages/dm-members-kept-complete`, `direct-messages/inactivity-across-groups`
- [x] 3.7 `RemoveUser` closes DMs, `DmClosed` until acked, `GROUP_CLOSED`; tests `direct-messages/dm-closed-on-user-removal`
- [x] 3.8 `membership-state-on-the-pi` test for a DM; the push ping unchanged for DM traffic (`direct-messages/dm-content-stays-encrypted`)

## 4. Core

- [x] 4.1 Store migration: `chats`, `messages.chat_id`, per-group rows; existing data goes to the main chat
- [x] 4.2 Engine: map of groups, routing by `group_id`, outbox, retries, rotation, acks per group; Welcome replaces only the same group; tests `direct-messages/several-groups-in-the-core`
- [x] 4.3 KeyPackages and leaves list `0xF0A2`, fresh upload on first run; `open_dm`, `created = false` handling, auto-add and re-add in DMs, DM checks on Welcome and commits, `DmClosed`; speaks version 1 to an old Pi; end-to-end tests for every `direct-messages` requirement
- [x] 4.4 Per-chat API: `chats`, `history(chat)`, `send_message(chat, …)`, unread and unseen mentions per chat, events with `ChatId`

## 5. Bridge

- [x] 5.1 `ChatRef`, `ChatSummary`, chat ids on calls and events; `flutter_rust_bridge_codegen generate`; bridge end-to-end test

## 6. App

- [x] 6.1 Chat list home; widget tests `app-ui/chat-list`
- [x] 6.2 "Message" on members, waiting state; widget tests `app-ui/open-a-dm`
- [x] 6.3 DM screen: title, no admin actions, no `everyone`, closed banner; widget tests `app-ui/dm-screen`

## 7. Docs and check

- [x] 7.1 `docs/architecture-decisions.md` (scope, DMs out of Future, commit authority, protocol v2, DM group id), `openspec/config.yaml` context, CLAUDE.md first line
- [ ] 7.2 `cargo test --workspace`, clippy, fmt, `buf lint`, `buf breaking`, `flutter analyze`, `flutter test`, `openspec validate dms --strict`
