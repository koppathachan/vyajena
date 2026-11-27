# Tasks

**Held for owner approval.** Do not start until the owner resolves open decisions
a–d (design.md "Owner decisions to confirm before apply"). The tasks below assume
the recommended answers and will be revised if the owner chooses differently. No
proto change is expected; if apply surfaces one, raise it to the owner first
(config.yaml: proto-first).

## 0. Gate

- [x] 0.1 Owner resolved decisions a–d as recommended and added decision e
  (Basic/More detail level) (owner, 2026-11-27; design.md decisions table).
- [x] 0.2 Owner chose the default for decision e: **Basic** (owner, 2026-11-27). Set
  the default to Basic in task 3.2.
- [ ] 0.3 Tracker pre-step: the unarchived `push` and `push-kind-aware` changes are
  verified merged/complete and archived, so message-delivery's "Content-free wake
  pings" can be a clean base (optionally fold this change's two added
  message-delivery reqs into a MODIFY then). Coordinated by the tracker.

## 1. Server — content-free, metadata-minimal payload (decision d, invariant)

- [x] 1.1 In `server/src/push/`, assert the payload invariant in one place for both
  providers: no content, sender, `group_id` or count; iOS alert + `mutable-content`
  + generic "New message"; Android high-priority data message. Test
  `message-delivery/wake-push-payload-is-content-free-and-metadata-minimal` against
  the fake FCM/APNs servers, asserting the serialized payloads carry only the wake
  and the generic string.
- [x] 1.2 Own-access admin-event wake: the server wakes the affected device
  content-free on removal / close / add-to-new-group, derived from authoritative
  membership. Test
  `message-delivery/content-free-wake-for-an-own-access-admin-event` (removal wakes;
  an unrelated re-key does not).

## 2. Core — the notification summary builder (decisions b, c)

- [x] 2.1 A module that, after catch-up, produces a per-conversation summary
  `{type, unread_count}` (no sender in v1) from the core's existing unread state and
  `group_id`→conversation map; never emits a body or sender. Tests
  `local-notifications/the-device-builds-the-notification-locally`,
  `local-notifications/conversation-type-without-decryption`,
  `local-notifications/unread-count-not-push-count`.
- [x] 2.2 Admin-event summaries from core notices (`RemovedNotice`, `GroupClosed`,
  `DmClosed`, self-add); routine churn excluded. Test
  `local-notifications/admin-event-notifications-are-fixed-content-free-phrases`.
- [x] 2.3 Bridge surface: expose the summary stream/getter and the per-device
  "Notification detail" (Basic / More) setting; regenerate bindings (`cd app &&
  flutter_rust_bridge_codegen generate`), commit generated files.

## 3. App — local notifications (decisions b, c, d)

- [ ] 3.1 `app/lib/push/` builds local notifications from the core summary:
  type-aware wording, per-conversation lines, OS grouping per `group_id`, total
  unread as the app badge; **no sender**; identical content on iOS and Android.
  Tests (widget, on the fake core) `app-ui/type-aware-notification-wording`,
  `app-ui/a-notification-opens-its-conversation`.
- [ ] 3.2 Settings: a "Notification detail" control (Basic / More, default Basic).
  The core summary carries count/type; the app gates rendering on the level. Tests
  `app-ui/notification-detail-level`,
  `local-notifications/respect-the-notification-detail-level`.
- [ ] 3.3 Android: `VyajenaMessagingService.onMessageReceived` handles the
  high-priority data message, runs the core, posts the metadata-only notification
  (count + type, no sender); fallback to catch-up-on-open documented.
- [ ] 3.4 iOS: a **metadata-only** Notification Service Extension target that rewrites
  the alert to count + type from the shared App Group metadata (no in-extension
  decrypt, no sender — D7); generic alert stays if it cannot finish. No
  `spikes/nse-decrypt/` in v1 (only needed if a later change adds iOS sender names).
- [ ] 3.5 Tap → open the conversation (by `group_id`) and clear its notification.

## 4. Compose with mute (no mute code here)

- [ ] 4.1 The local build re-checks the core's mute view before posting, so a
  conversation muted after a wake is still silenced locally (change
  `mute-notifications` gates the server push). Test that a muted conversation's
  summary raises no local notification once mute lands; until then, a placeholder
  test asserting the gate point exists.

## 5. Docs + invariant

- [ ] 5.1 `docs/architecture-decisions.md` "who sees what": add the explicit "push
  provider sees only a content-free wake" line and the iOS NSE / Android data-message
  flow diagram.
- [ ] 5.2 Add the content-free-payload invariant to `openspec/config.yaml` context
  (owner-approved, since it changes the invariant list).

## 6. Done

- [ ] 6.1 `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo fmt --check`, `cd app && flutter analyze && flutter test`,
  `buf lint`, `openspec validate smarter-notifications --strict` all pass; every
  requirement has a test naming it.
