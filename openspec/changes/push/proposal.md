## Why

The app only gets messages while it is open: the Pi already decides when a
device needs a wake ping (message-delivery: Content-free wake pings) and the
app can register a token (`Devices.UpdatePushToken`), but the Pi has only a
logging stub. The relay is live and the app is on Play and TestFlight
internal testing, so real push is next in the build order (owner, 2026-11-15).

## What Changes

- **Pi providers:** real FCM HTTP v1 (Android) and direct APNs (iOS) behind the
  existing `PushProvider` trait. FCM signs in with a service account of the GCP
  project `vyajena` (Firebase added to it, no separate project). APNs uses a
  token-based `.p8` key; the Pi signs its own ES256 JWT. No Firebase SDK on iOS.
- **Fixed text:** every ping shows one generic notification, "New message":
  a visible alert on iOS, a notification on Android. Still no content, sender,
  group id or count; still at most one ping per device per 60 s. Opening it
  opens the app, which connects and catches up.
- **Errors:** a token the platform calls invalid is cleared on the Pi; 429 and
  5xx back off and retry; logs name the device and platform only.
- **Revocation:** a revoked device's push token is cleared.
- **Token limits:** `UpdatePushToken` refuses tokens over 4096 bytes and APNs
  tokens that are not hex.
- **Egress:** `vyajena.service` stays loopback-only. A new sandboxed
  `vyajena-egress.service` (same binary, `egress-proxy`) forwards HTTPS
  `CONNECT` to exactly the FCM, Google OAuth and APNs hosts on port 443.
- **Credentials:** only in `/var/lib/vyajena/push/` (0700, files 0600), copied
  by `deploy/pi/deploy.sh` from the owner-only `~/.vyajena-push/` on the Mac.
  Never in the repo. `deploy/pi/check.sh` covers the proxy and the files.
- **Review server:** keeps the logging stub (owner, 2026-11-16).
- **App:** registers its token through the core on every connect, re-registers
  when the token rotates, asks for notification permission once after the
  device joins, clears shown notifications when opened, forgets its token when
  the device is removed. Android: Firebase Messaging, configured at build time
  from `~/.vyajena-push/google-services.json` (not committed; owner,
  2026-11-16). iOS: `UNUserNotificationCenter` + the `aps-environment`
  entitlement.
- **Bridge:** `Core.set_push_token` (regenerated with frb).
- **Out of scope:** on-device decrypted previews (a later change), push for the
  review server, Data safety edits in Play (owner and app-release).

No `.proto` change: `PushToken`, `PushPlatform` and `UpdatePushToken` already
exist.

## Open decisions resolved

- Push provider: FCM HTTP v1 in GCP project `vyajena` + direct APNs (owner).
- Notification text: fixed "New message", previews later (owner).
- Review server push: no (owner, 2026-11-16).
- `google-services.json`: kept out of the public repo (owner, 2026-11-16).
- How the sandboxed server reaches Google and Apple: a hostname-allowlist
  egress proxy (design D5).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `message-delivery`: wake pings show fixed text; collapse into one.
- `device-identity`: token limits; revoked devices lose their token.
- `pi-runtime`: real providers, credentials, error handling, the egress proxy.
- `pi-deployment`: the egress service, push credentials, the check.
- `app-ui`: push registration, permission moment, notification taps.

## Impact

- `server/src/push/` (providers, JWT, proxy), `server/src/main.rs`
  (`--push`, `egress-proxy`), `server/src/messaging/mod.rs` (clear invalid
  tokens), `server/src/devices/mod.rs` (limits), `server/src/auth/mod.rs`
  (revocation), `server/tests/` (fake FCM/APNs servers).
- `client/src/client.rs` (doc only), `bridge/src/api/core.rs` + generated code.
- `app/lib/push/`, `app/lib/app/lifecycle.dart`, Android (Gradle, manifest,
  Kotlin service), iOS (AppDelegate, entitlements).
- `deploy/pi/` (`push.toml`, `vyajena-egress.service`, deploy, check),
  README, `docs/architecture-decisions.md`, `docs/release.md` (push section).
- app-release (owner + its session): Android permissions now include
  `POST_NOTIFICATIONS` and Firebase's; iOS gains Push Notifications; Play Data
  safety / App Store privacy answers may change.
