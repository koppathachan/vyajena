## Why

Reopening the app is the most common connect, and it is slow. Every
background→reopen runs `core.stop()` then `core.start()` (`app/lib/app/lifecycle.dart`),
which builds a new engine whose session token is in memory only. The token the
last run held is thrown away, so the reopen pays the full four-round-trip
handshake (`GetServerInfo`, `GetChallenge`, `Authenticate`, `Connect`) over the
~260 ms India→US→India relay hairpin. Change `faster-connect` only helps a drop
while one engine keeps running.

## What Changes

- **Persist the resume token.** The core SHALL save its latest session token (and
  its expiry and renewal point) in the OS key store each time it obtains or
  renews one, and load it on `start()`, so a reopen with a still-valid token
  opens `Connect` directly: two round trips (`GetServerInfo` + `Connect`)
  instead of four.
- **Clear it on removal.** The core SHALL delete the saved token when the Pi
  says the device is revoked, and when a fresh signup or link request creates new
  device keys.
- **Session tokens last 6 hours (was 1).** `SESSION_TOKEN_MAX_LIFETIME` becomes
  `6 * 60 * 60`. The Pi issues tokens of that lifetime and the core's renewal
  point (5 min before expiry) follows from it. Without this, a one-hour token
  rarely survives a day of background→reopen.
- **No proto change.** The token is already a connection-independent bearer
  (faster-connect D1); `buf breaking` stays clean and `pbgen` is not run.

## Capabilities

### New Capabilities

<!-- None. -->

### Modified Capabilities

- `wire-transport`: "Session authentication" — tokens SHALL expire after at
  most 6 hours (was 1 hour).
- `client-runtime`: new requirement "Resume across restart" — the core SHALL keep
  its latest session token only in the key store, SHALL reuse it on `start()`
  while it is before its renewal point, and SHALL delete it when the device is
  removed or its keys are recreated.

## Impact

- Code: `protocol/src/wire_transport.rs` (one constant), `client/src/keystore/`
  (`KeyName::SessionToken`), `client/src/net/conn.rs` (encode/decode a
  `ResumeToken`), `client/src/engine/mod.rs` (save on obtain/renew, delete on
  `removed`), `client/src/client.rs` (load in `start()`),
  `client/src/identity/keys.rs` (clear on fresh keys).
- Tests: `client/tests/connection.rs` (renewal tests move from 56 min to 5 h 56 m;
  new restart-resume tests), `client/tests/key_storage.rs`, a `protocol/tests`
  assertion for 6 h, `server/tests/session_authentication.rs`.
- Docs: `docs/architecture-decisions.md` line on token lifetime.
- No proto, no `pb.rs`, no bridge API, no Flutter change; the app picks it up on
  its next build, the Pi on its next deploy (any order: old cores and new Pi, and
  the reverse, still interoperate).
- **Security:** a 6 h token is a longer theft window. It lives only in the key
  store, and revoking the device deletes its sessions on the Pi, so a stolen
  token dies with the device's removal (design D3).
