## Why

Three UX bugs found in the 2026-11-24 self-hosting test, all in onboarding/media:

- **A typed invite fails.** The invite field auto-capitalizes the leading `v` of
  `vyajena-invite:`, and the core's prefix strip is case-sensitive, so a *typed*
  invite fails base64url parsing ("This is not a valid invite"). Pasting works.
- **Duplicate pending invites.** `Admin.CreateInvite` lets an admin mint two unused
  invites for the same username (seen 5s apart), cluttering the invite list.
- **Media hangs silently.** When the server has no `[blob_store]`, voice messages
  and images sit on a perpetual hourglass with no error: the core classifies the
  server's `UNAVAILABLE` as retryable and retries every 3s until the blob's 7-day
  TTL expires.

## What Changes

- **A typed invite is accepted (app-ui).** The invite field disables
  auto-capitalize/auto-correct, and the core matches the `vyajena-invite:` prefix
  case-insensitively (the base64url payload stays case-sensitive).
- **At most one pending invite per username (device-identity).** `Admin.CreateInvite`
  returns the existing unused invite for a username instead of minting a duplicate.
  The server enforces it (robust against a double tap / two clients).
- **Media the server will not store fails promptly (client-runtime + app-ui).** The
  core stops retrying after a short bounded number of consecutive `UNAVAILABLE`
  answers and fails the message with a clear reason; the app shows the failed state
  with a resend affordance instead of an endless hourglass. A brief outage still
  uploads.

## Proto files touched

- **None.** The invite prefix is parsed in `bridge/src/api/text.rs`; the dedup is in
  the server's `Admin.CreateInvite` handler over existing invite rows; the upload
  retry policy is in `client/src/engine/uploads.rs`. No wire change.

## Capabilities

### New Capabilities

<!-- None. -->

### Modified Capabilities

- `app-ui`: new requirements "A typed invite is accepted" and "Media the server
  will not store shows an error".
- `device-identity`: new requirement "At most one pending invite per username".
- `client-runtime`: new requirement "Media the server will not store fails
  promptly".

## Impact

- **App:** `app/lib/onboarding/onboarding_screen.dart` (field flags); the failed
  media state + resend already render, so no new media UI.
- **Bridge:** `bridge/src/api/text.rs` (case-insensitive prefix); regenerate
  bindings only if the public API changes (it does not).
- **Server:** `server/src/admin/mod.rs` + `server/src/admin/invites.rs` (pending
  invite lookup + idempotent return).
- **Core:** `client/src/engine/uploads.rs` (bounded `UNAVAILABLE` retry → fail).
- No server deploy; builds serialise on the shared external-disk cargo target.
