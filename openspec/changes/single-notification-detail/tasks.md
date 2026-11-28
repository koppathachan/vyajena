# Tasks — single-notification-detail

App + core + bridge. No proto, no server. Tick a task only when its tests pass.

## 1. Core (`client`)
- [x] 1.1 Remove `NotificationDetail` enum + impl (`client/src/notification.rs`) and
      its `lib.rs` export.
- [x] 1.2 Remove `notification_detail` / `set_notification_detail` from the store
      (`client/src/store/notifications.rs`) and from `Client` (`client/src/client.rs`).
- [x] 1.3 Leave migration `0015_notifications.sql` as-is (the `notification_detail`
      table is unread; no new migration).

## 2. Bridge
- [x] 2.1 Remove `NotificationDetailLevel` (`api/types.rs`), the two `api/core.rs`
      methods, and the `map.rs` mappers.
- [x] 2.2 Regenerate: `cd app && flutter_rust_bridge_codegen generate`; commit
      `bridge/src/frb_generated.rs` and `app/lib/src/rust/`.

## 3. App
- [x] 3.1 `notification_text.dart`: `renderNotifications` drops the `level` param and
      the Basic branch; always renders count+type.
- [x] 3.2 `notification_service.dart`: stop reading `core.notificationDetail()`.
- [x] 3.3 `profile_screen.dart`: remove the Basic/More selector, `_detail`,
      `_loadDetail`, `_setDetail`; keep a static "Notification detail" description.
- [x] 3.4 `core_service.dart` / `rust_core_service.dart`: remove `notificationDetail`
      / `setNotificationDetail`.

## 4. Tests
- [x] 4.1 `client/tests/local_notifications.rs`: remove the detail-level test.
- [x] 4.2 `app/test/push/notifications_test.dart`: drop the level + selector tests;
      keep always-detailed rendering, the data-wake count+type, and a "no selector"
      widget test. Fake core drops the detail field/methods.

## 5. Gate
- [x] 5.1 `cargo build -p client -p bridge` and `cargo test -p client --test
      local_notifications` green.
- [x] 5.2 `cd app && flutter analyze` clean; `flutter test` green.
- [x] 5.3 `cargo clippy -p client -p bridge --all-targets -- -D warnings`,
      `cargo fmt --check` clean.
- [x] 5.4 `openspec validate single-notification-detail --strict` passes.
- [x] 5.5 No proto/server files changed.
