# Design — single-notification-detail

## Decision
One notification behaviour — the detailed "More" content (per-conversation count +
type, no sender, no text) — and no per-device level or selector. Owner decision,
2026-11-28.

## Why full removal, not a fixed default
The detail level was a per-device setting threaded through core → store → bridge →
Dart. Leaving the plumbing while hiding the selector would keep dead public API and a
spec that no longer matches the code. Since the level is consumed only by Dart
rendering (`renderNotifications`) and read back through the bridge, removing it end to
end is clean and low-risk: the core `NotificationSummary` (type + count) is unchanged;
only the enum, its store/accessor/bridge surface, and the selector go.

## No migration
Migration `0015_notifications.sql` created the `notification_detail` table. Dropping a
table needs a new migration and a schema bump; the table is tiny and now simply unread.
Leaving it avoids migration risk for no user benefit. A future schema cleanup can drop
it. (Documented so it is not mistaken for an oversight.)

## Bridge regeneration
`api/types.rs` (`NotificationDetailLevel`), `api/core.rs` (two methods) and `map.rs`
(two mappers) are removed, then `flutter_rust_bridge_codegen generate` (2.12.0)
rewrites `bridge/src/frb_generated.rs` and `app/lib/src/rust/`. The generated files
are committed with the change (CI does not regenerate).

## Behaviour for existing installs
A device that had chosen Basic now gets the detailed notifications; nothing migrates
and nothing breaks. The stored Basic value in `notification_detail` is ignored.

## Spec placement
The two requirements being modified live in the still-unarchived `smarter-notifications`
change, not yet in `openspec/specs/`. This change's MODIFIED requirements are written
as the full end state and supersede the two-level text; archive `smarter-notifications`
before or with this change (same pattern as `push-silent-app-envelopes` vs
`push-kind-aware`).

## Tests
- Dart: `renderNotifications` always renders count+type; a data wake posts count+type;
  the Profile screen shows the description and no selector. (`notifications_test.dart`)
- Rust: the detail-level test is removed (`local_notifications.rs`); the summary/mute
  tests are unchanged.
- Gates: `flutter analyze` (strict), `flutter test`, `cargo build/test -p client -p
  bridge`, `openspec validate single-notification-detail --strict`.
