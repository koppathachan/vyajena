# Project Context

## Purpose

vyajena2 is an end-to-end encrypted, old-school text messaging app for one
admin-run group. It is the successor to the original `vyajena` learning
experiment. The MVP is invite-only, text-only, and store-and-forward through a
self-hosted Raspberry Pi server. Plaintext exists only inside the Rust core on
each device; the relay and server see only ciphertext.

See `docs/architecture-decisions.md` for the full architecture rationale.

## Tech Stack

- **Flutter UI** — draws screens only; never opens a network connection.
- **Rust core** — OpenMLS (RFC 9420), networking, local SQLite, keys in
  Keystore/Keychain. Linked to Flutter via `flutter_rust_bridge`.
- **GCP relay** — dumb TCP passthrough (rathole + Noise) so no port opens at
  home. Holds no state.
- **Pi server** — Rust, tokio + tonic, SQLite in WAL mode, one static binary.
  Routes, orders and queues ciphertext; dispatches content-free wake pings
  (FCM / APNs).
- **Wire protocol** — `.proto` files are the contract, independent of the Rust
  code. One persistent bidirectional gRPC stream over HTTP/2 + TLS on TCP 443.

## Project Conventions

### Spec-driven development (OpenSpec is the way)

All non-trivial work flows through OpenSpec. Do not hand-write features ad hoc.

- Start a change with `/opsx:propose "<idea>"`, build the artifacts
  (proposal → specs → design → tasks), implement, then `/opsx:archive`.
- The `.proto` wire contract and the architecture decisions are the source of
  truth; specs refine them, they do not contradict them.

### Code style

- **Proto-first.** Define or update the `.proto` contract before writing Rust
  or Dart that depends on it.
- Rust core owns all networking and crypto; Flutter stays a thin view layer.
- Server ids are `user@<server>` from day one so federation can be added later.

### Architecture invariants (do not break without a decision)

- Plaintext never leaves the Rust core; the relay and Pi see only ciphertext.
- The Pi is the single authority for routing, ordering and membership, and
  **never** for message history.
- Strict epoch rule: the Pi rejects any message tagged with an old epoch.
- Admin-only membership Commits; members may only update their own keys.
- No encryption at rest; keys live in the OS key store, not the DB.

## Important Constraints

- Message expiry 7 days; queue cap 1,000 per recipient device; max message
  size 64 KB (proposed).
- Idempotent sends via client-generated `msg_id`.
- GCP free-tier limits: 1 GB/month outbound; watch for IPv4 charges.

## Open Decisions

Tracked in `docs/architecture-decisions.md` → "Open decisions". Resolve each via
an OpenSpec change before it lands in code:

- Multi-device in the MVP?
- Fingerprint verification in the MVP?
- Desktop clients, or mobile only?
- Key rotation for idle groups (every 7 days proposed)?
- Max message size 64 KB?
- Relay on a shared VM or its own?
- Build order.
