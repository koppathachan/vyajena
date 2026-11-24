# Tasks — group-resync

## 1. Client: detect the wedge and self-request re-add (G1)
- [ ] 1.1 In the engine, track when the `Connect` stream has caught up (acked to the
      server's latest `server_seq`) yet an own message is still held behind
      `server_epochs[group]` for a group the device still holds locally.
- [ ] 1.2 On that condition, issue `Keys.RequestReadd` (reason "behind epoch"),
      bounded by `MAX_READD_REQUESTS_PER_DAY`; do not fire for a device that is
      still draining catch-up (merely transiently behind).
- [ ] 1.3 Test (`client/tests/inactivity.rs`): a device removed for inactivity whose
      only re-adder is offline, holding a stale group, self-requests re-add after
      catch-up; when the admin connects it is re-added and the held message goes to
      accepted. // spec: group-membership/orphaned-device-self-recovery

## 2. Client + app: terminal out-of-sync state, never a silent hourglass (G2)
- [ ] 2.1 Add a core `Event` (e.g. `OutOfSync { chat }`) emitted when re-add cannot
      complete (no re-adder, or the daily limit reached); surface through the bridge.
- [ ] 2.2 `outbox`: a message held behind the epoch while out-of-sync is reported in
      a distinct recoverable state (not silent `sending`, not plain `failed`).
- [ ] 2.3 App: an in-chat "This chat is out of sync — re-sync" banner with a
      **Re-sync** action that triggers the re-add request.
- [ ] 2.4 Widget test: the banner + Re-sync action appear on `OutOfSync` and Re-sync
      calls through to the core. // spec: app-ui/out-of-sync-re-sync-affordance
- [ ] 2.5 Core test: a held message surfaces re-sync rather than an indefinite silent
      `sending` once the device is known stuck. // spec: client-runtime/send-status

## 3. Server: durable removal notice (G3)
- [ ] 3.1 Record that an inactive device owes a "removed for inactivity" notice;
      deliver it on the device's next `Connect` even if it was offline when removed.
      SQLite migration `0010+`; no wire field (reuse `RemovedNotice`).
- [ ] 3.2 Test (`server/tests/`): a device offline when removed is told it was
      removed, and since when, on its next connect.
      // spec: group-membership/automatic-re-add

## 4. Admin recovery flow (D4)
- [ ] 4.1 Surface/ document the remove → re-invite → rejoin recovery as a first-class
      flow reachable in the app (Remove a user, then Invite a user); note what is
      lost (old device key + local history; closed DMs re-open).
- [ ] 4.2 Test/doc: `Admin.CreateInvite` after `Admin.RemoveUser` succeeds for the
      same username; the re-registered device is auto-added back to the lobby.
      // spec: app-ui/out-of-sync-re-sync-affordance

## 5. Owner decision (not code)
- [ ] 5.1 Record the admin single-device lockout decision (link-a-second-admin-device
      recommendation now; real admin recovery = `replace-user` Phase 2) in the
      architecture doc's open decisions for the owner.

## 6. Done
- [ ] 6.1 `cargo test`, `cargo clippy -D warnings`, `cargo fmt --check`,
      `cd app && flutter analyze && flutter test`, `openspec validate group-resync --strict`
      all pass; tasks ticked only when their tests pass.
