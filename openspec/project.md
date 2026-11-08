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
- Strict epoch rule: the Pi rejects any envelope not in the group's current
  epoch; OpenMLS keeps `max_past_epochs = 0`.
- Commit authority, checked by the Pi and every client: admin devices may
  commit anything; other devices only their own leaf update, adding or removing
  their own user's devices, and Pi-issued inactivity Removes. Handshakes are MLS
  `PublicMessage` so the Pi can check them with OpenMLS `PublicGroup`.
- OpenMLS does not check credentials: the core verifies every new or changed
  leaf's device certificate against the user's pinned identity key, and that
  the certificate's MLS key equals the leaf's signature key.
- Group name, settings and user profiles travel only inside encrypted
  messages; the Pi stores none of them.
- No encryption at rest; keys live in the OS key store, not the DB.

## Important Constraints

- Max 64 KB of application ciphertext; 256 KB per handshake envelope, a
  Commit's Welcome included.
- Queue cap 1,000 application messages per recipient device; message expiry
  7 days as a backstop for devices that connect but never ack.
- Inactivity: a device offline 7 days, or whose queue reaches 1,000, is removed
  (Pi external Remove) and re-added automatically when it returns. No send is
  ever rejected because of another device's queue.
- Idempotent sends via client-generated `msg_id`.
- Limits, counts and timings come from the constants in `vyajena-proto`.
- GCP free-tier limits: 1 GB/month outbound; watch for IPv4 charges.

## Open Decisions

Tracked in `docs/architecture-decisions.md` → "Open decisions". Resolve each via
an OpenSpec change before it lands in code:

- ~~Multi-device in the MVP?~~ Resolved: yes, each device is an MLS member
  certified by its user's identity key (change `core-wire-protocol`).
- Fingerprint verification in the MVP?
- Desktop clients, or mobile only?
- Key rotation for idle groups (every 7 days proposed)?
- ~~Max message size 64 KB?~~ Resolved: 64 KB of application ciphertext
  (change `core-wire-protocol`).
- Relay on a shared VM or its own?
- Build order.
