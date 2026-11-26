## 0. Proto / spikes

- [ ] 0.1 No proto change in the MVP (design D2, D3): report rides the existing DM
  path and `Content.Text`; block is local. `buf lint` / `buf breaking` have nothing
  to check and no `pbgen` run is needed. The typed `Report` body (D3) and the
  server block marker (D4) are Phase-2 and out of this change.
- [ ] 0.2 No spike needed: every piece reuses a shipped mechanism (DM path,
  reply-quote, admin removal). `spikes/` stays empty.

## 1. Core — block (device-local)

- [ ] 1.1 Add a device-local `blocked_users(user_id)` table to the core's SQLite
  store (additive, local-only migration; never synced).
- [ ] 1.2 `Client::block_user(user_id)` / `Client::unblock_user(user_id)` add and
  remove an id idempotently; `Client::blocked_users()` returns the set.
  Tests: `moderation/block-a-user`.
- [ ] 1.3 Filter blocked senders in the display/event path: omit a blocked user's
  group and lobby messages from emitted history, hide a DM whose peer is blocked, and
  suppress in-app notifications for blocked senders; reading the sender from the
  envelope's authenticated `UserId`. Messages stay stored and reappear on unblock.
  Tests: `moderation/a-blocked-user-is-hidden`.

## 2. Core — report (reuse the DM path + reply-quote)

- [ ] 2.1 `Client::report(target)` where `target` is a group/lobby message, a DM
  message, or a user: compose the canonical report payload (design D2) as a
  `Content.Text`.
- [ ] 2.2 Group/lobby message report carries reported `UserId` + group id + `msg_id`
  + reason (reference only). Tests: `moderation/report-a-message`.
- [ ] 2.3 DM message report quotes the offending text via the existing reply-quote
  rules (fold whitespace, cap at `MAX_REPLY_QUOTE_CHARS`, trailing "…"); no DM key or
  ciphertext leaves the device. Tests: `moderation/report-a-dm-message`.
- [ ] 2.4 Deliver the report to the admin by opening (or reusing) a DM with the
  admin's known `UserId` and sending over the existing DM send path; it is delivered
  on the admin's next connect if offline. Tests: `moderation/report-reaches-the-admin`.

## 3. Bridge

- [ ] 3.1 Expose `block_user`, `unblock_user`, `blocked_users`, and `report` in
  `bridge/src/api/`; regenerate bindings (`flutter_rust_bridge_codegen generate`) and
  commit the generated files.

## 4. App — UI

- [ ] 4.1 Block/Unblock action on a message long-press menu and on a user's profile,
  with a pre-block confirm that it hides on this device only; a blocked marker where
  the user appears. Hidden content disappears/reappears with block/unblock.
  Tests: `app-ui/block-and-unblock-a-user`.
- [ ] 4.2 Report action on a message long-press menu and on a user's profile: ask for
  a short reason, call the core, confirm "report sent". Offer Report and Block
  together on a message. Tests: `app-ui/report-a-message-or-a-user`.
- [ ] 4.3 Admin's app: present an incoming report distinctly from an ordinary DM —
  reported user, reason, referenced (group) or quoted (DM) content — with a path to
  the existing remove-user screen. Tests: `app-ui/the-admin-sees-reports`.
- [ ] 4.4 "Safety & moderation" entry in the Profile tab showing the policy (how to
  report/block, the 24-hour commitment, the contact email).
  Tests: `app-ui/safety-and-moderation-entry`.

## 5. Policy / docs (store blocker)

- [ ] 5.1 Record the moderation policy text (design.md) as the in-app "Safety &
  moderation" content and in the App Store + Play listing copy; owner confirms the
  final safety contact address before the public release.
  Tests: `moderation/published-moderation-policy` (the in-app entry renders the
  policy and contact email).

## 6. Done

- [ ] 6.1 `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo fmt --check`, `cd app && flutter analyze && flutter test` all
  clean; every requirement has a test naming it.
- [ ] 6.2 `openspec validate report-and-block --strict` passes.
