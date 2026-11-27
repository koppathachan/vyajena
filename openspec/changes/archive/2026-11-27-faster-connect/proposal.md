## Why

A reconnect is slow. From India to the US relay and back each round trip is
about half a second, and a cold connect spends three of them before any message
flows: `GetChallenge`, `Authenticate`, then opening the `Connect` stream. The
device already holds a renewing session token that the server accepts on any
connection, so two of those three round trips can be skipped on reconnect.

## What Changes

- **Session-token resume.** On reconnect the core reuses its still-valid
  session token to open the `Connect` stream directly, skipping `GetChallenge`
  and `Authenticate`. This saves two round trips (about one second India↔US) on
  every reconnect within the token's lifetime.
- **Fallback to the full handshake.** The core runs the full challenge/auth
  handshake when it has no usable token (first connect, or the token is at or
  past its renewal point) or when the server rejects the token on the stream
  (`UNAUTHENTICATED`). Rejection then falling back to the full handshake is also
  how a revoked device still reaches `PERMISSION_DENIED` and stops.
- **No proto change, no new server behavior.** The session token is already a
  connection-independent bearer token: the server's auth layer authenticates every
  RPC, `Messaging.Connect` included, from the `authorization` token alone,
  with no per-connection challenge state. Resume reuses that. The change is the
  core's connect path plus tests that pin the behavior on both sides. See
  design D1–D2.
- The wire contract is unchanged, so `buf breaking` is trivially clean; no
  `.proto` edit and no `pbgen` run are needed (design D1).

## Capabilities

### New Capabilities

<!-- None. This change adds no capability. -->

### Modified Capabilities

- `wire-transport`: a new requirement, "Session resume", states that the server
  SHALL accept a valid session token to open `Messaging.Connect` whatever
  connection obtained it, and rejects an expired or unknown one with
  `UNAUTHENTICATED`.
- `client-runtime`: "Connection lifecycle" gains that the core SHALL reuse a
  still-valid session token on reconnect to skip `GetChallenge`/`Authenticate`,
  and SHALL fall back to the full handshake when it has no usable token or the
  Server rejects it.

## Impact

- Code: `client/src/net/conn.rs` (extract and re-bind a token to a new
  connection), `client/src/engine/mod.rs` (keep the token across drops; resume
  path with fallback in `open_link`).
- Server: no code change; a test confirms a token minted on one connection
  opens `Connect` on another, and an expired one is refused.
- No proto, no generated `pb.rs`, no bridge, no Flutter, no deploy change.
- Open decision for the owner, recorded in design.md (D4), not acted on here:
  moving the relay from the US to India would cut each round trip from ~0.5 s to
  tens of milliseconds — a far larger win than this change — but leaves the GCP
  free tier.
