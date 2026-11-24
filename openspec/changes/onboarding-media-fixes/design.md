## Context

Three independent bug fixes bundled as one "onboarding and media rough edges"
change (all surfaced by the 2026-11-24 self-hosting test). None needs a wire change.

## Decisions

### D1: Typed invite — fix both the field and the prefix

- **Field (app):** `onboarding_screen.dart`'s invite `TextField` gets
  `autocorrect: false`, `enableSuggestions: false`, and
  `textCapitalization: TextCapitalization.none`, so the keyboard stops capitalizing
  the leading `v`.
- **Prefix (core):** `bridge/src/api/text.rs::payload_bytes` strips the
  `vyajena-invite:` (and link/approval) prefixes case-insensitively. This is the
  durable fix: even a hand-edited or quoted invite with an odd-case prefix parses.
  Only the prefix is case-folded; the base64url payload after it stays exact. Both
  together: the field fix prevents the common case; the prefix fix is defence in
  depth and covers paste-from-odd-sources too.

### D2: Dedup pending invites — idempotent return, server-side

`Admin.CreateInvite` is the enforcement point (a client guard alone is racy — the
test saw two invites 5s apart). Before creating, the handler looks up an existing
unused, unrevoked, unexpired, non-bootstrap invite for the username
(`invites::usable_for_username`, added alongside the existing `usable_bootstrap`).
If one exists, the handler returns **that** invite and rebuilds its payload, instead
of minting a second. So asking twice yields the same invite — "dedup" with no error
to handle in the UI, and robust against a double tap or two admin devices.

A used/revoked/expired invite does not block a fresh one (it is not pending), and
the existing "user already exists" guard is unchanged.

### D3: Media hang — bounded UNAVAILABLE retry, client-side

The core treats every `UNAVAILABLE` from `UploadBlob` as retryable and retries
every `UPLOAD_RETRY_DELAY` (3s) until `BLOB_TTL` (7 days), so a server with no
`[blob_store]` gives a silent 7-day hourglass. The fix, entirely in
`client/src/engine/uploads.rs`:

- `classify` distinguishes `UNAVAILABLE` from other retryable statuses.
- The `Upload::RetryAt` state carries a consecutive-`UNAVAILABLE` counter. After a
  small bound (`MAX_BLOB_UNAVAILABLE_RETRIES`, ~10 attempts ≈ 30s) of back-to-back
  `UNAVAILABLE` answers, the core fails the message with a clear reason ("the server
  is not storing pictures / voice messages"), reusing the existing `fail(...)` path
  and the `Failed` send status the app already renders with a resend affordance.
- A non-`UNAVAILABLE` retryable status (unauthenticated, timeout) resets the counter
  and keeps the existing policy; a brief `UNAVAILABLE` that clears within the bound
  uploads normally.

The task's alternative — a `ServerInfo` blob-availability flag — is cleaner for the
"configured off" case but adds a proto field, server code, and an app pre-check, and
does not help the "drive transiently away for a long time" case. The client-side
bound fixes both with no wire change and is the task's preferred route; a
`ServerInfo` flag can be layered on later if a pre-send refusal is wanted. Recorded
here rather than built.

Threshold trade-off: ~30s may fail a genuine long outage early, but the user gets
feedback and a resend button — far better than a 7-day silent hourglass — and a
longer real outage is rare next to the common "operator forgot `[blob_store]`" case
this fixes.

## Migration Plan

No proto, no wire, no data migration, no server deploy. App field flags are
cosmetic; the prefix and dedup and retry changes are behaviour-only. Builds
serialise on the shared external-disk cargo target.

## Open Questions

- None. A `ServerInfo` media-availability flag (D3 alternative) is a possible future
  nicety, not in scope.
