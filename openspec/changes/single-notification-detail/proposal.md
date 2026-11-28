## Why

The "Notification detail" setting (a per-device Basic/More selector, default Basic)
adds a choice few users want and buries the better behaviour behind a default that
says only "New message". The owner's decision: there is one kind of notification —
the detailed one we call **More** (per-conversation count and type, never a sender or
any message text) — and it is the only behaviour. The Basic level and the selector
go away.

This still shows no sender and no message content: the metadata-only guarantee of
`smarter-notifications` is unchanged. Only the *choice* and the content-free "Basic"
wording are removed.

## What Changes

- **Notifications always render the detailed metadata.** `renderNotifications` always
  produces the per-conversation count and type (e.g. "3 new in Work", "2 new DMs"),
  and the admin-event phrases, on both iOS and Android. There is no "Basic" generic
  line any more.
- **The "Notification detail" selector is removed from settings.** The Profile screen
  keeps a short, static "Notification detail" description of what notifications show,
  with no Basic/More control.
- **The per-device detail level is removed end to end.** Gone: the core
  `NotificationDetail` enum, its store get/set, the `Client::notification_detail` /
  `set_notification_detail` API, the bridge `NotificationDetailLevel` type + its two
  methods and mappers, and the Dart `CoreService.notificationDetail` /
  `setNotificationDetail`. The Flutter bindings are regenerated.
- **No migration.** Migration `0015_notifications.sql` still creates the
  `notification_detail` table; it is simply no longer read or written (a harmless
  empty table). Removing it is left to a future schema cleanup.

Server-free and push-free: this only changes how a device words its own local
notifications. It is an app + core + bridge change; no proto, no server.

## Open decisions resolved (owner, 2026-11-28)

- One notification kind only — the detailed "More" content — and it is the default.
- The Basic/More selector is removed from Settings; a static description remains.

## Capabilities

### Modified Capabilities

- `app-ui`: the per-device "Notification detail" level and its selector are removed;
  notifications always show the per-conversation count and type, never a sender or
  text, the same on iOS and Android.
- `local-notifications`: the device always renders the count-and-type metadata; there
  is no detail level to respect.

## Impact

- **Proto / server:** none.
- **Core (`client`):** remove `NotificationDetail` (`notification.rs`), its store
  methods (`store/notifications.rs`), `Client::notification_detail` /
  `set_notification_detail` (`client.rs`), and the `lib.rs` export.
- **Bridge:** remove `NotificationDetailLevel` (`api/types.rs`), the two `api/core.rs`
  methods, and the `map.rs` mappers; **regenerate** `frb_generated.rs` and
  `app/lib/src/rust/` with `flutter_rust_bridge_codegen generate`.
- **App:** `notification_text.renderNotifications` drops its `level` parameter and the
  Basic branch; `notification_service` stops reading the level; `profile_screen`
  drops the selector and its state/handlers; `core_service` /`rust_core_service` drop
  the two methods.
- **Tests:** `client/tests/local_notifications.rs` drops the detail-level test;
  `app/test/push/notifications_test.dart` drops the level and selector tests and keeps
  the always-detailed rendering + the "no selector" widget test; the fake core drops
  the detail field/methods.
- **Ops:** ships in an **app release**; no server deploy. Existing installs that had
  chosen Basic now get the detailed notifications (and the leftover `notification_detail`
  row is ignored).

## Relationship to other changes

- Supersedes the "Notification detail level" (app-ui) and "Respect the notification
  detail level" (local-notifications) requirements introduced by the still-unarchived
  `smarter-notifications` change. **Archive ordering:** archive `smarter-notifications`
  before (or together with) this change; this change's MODIFIED requirements are the
  full end state and supersede the two-level text.
