## Why

Every vyajena2 component (Rust core, Pi server, relay config) depends on the wire
contract, and the project is proto-first: nothing that talks over the network can
be written until the `.proto` files exist. Two open decisions shape that contract
directly and are resolved here: **multi-device is in the MVP**, and the **max
message size is 64 KB of ciphertext**.

## What Changes

- Add the `vyajena.v1` protobuf package under `proto/`: the `Messaging`,
  `Devices`, `Keys` and `Admin` gRPC services, the `Envelope`, and the inner
  (client-to-client) message content format.
- Add a Rust crate that generates and compiles the gRPC code (prost/tonic), plus
  `buf` lint and breaking-change checks in CI.
- **Resolve open decision #1 (multi-device, yes):** an MLS member is a *device*.
  Each user has an Ed25519 **user identity key** that signs a **device
  certificate** for each of their devices. The first device holds the identity
  key and links further devices.
- **Resolve open decision #5 (max size, 64 KB):** application ciphertext is
  capped at 65,536 bytes, enforced by the client before sending and by the Pi
  on receipt.
- **Relax an invariant (approved by the owner):** "admin-only membership Commits"
  becomes "admin devices may commit any change; any other device may commit
  only self key updates, or adding/removing devices of its own user".
- **Replace two architecture rules with one:** "queue full rejects the send" and
  "removed after 30 days offline" become "a device offline for 7 days or with a
  full queue is removed, and is re-added automatically when it returns".
- Handshake messages (Commits, Proposals) travel as MLS `PublicMessage` so the Pi
  can enforce membership policy; application messages stay `PrivateMessage`.
- Amend the architecture doc where this design deviates from or clarifies it:
  catch-up delivers commits and messages merged in group order (not "commits
  first"), the admin's user id is visible to the Pi in the group context, and
  group name/settings travel as encrypted application messages.

## Capabilities

### New Capabilities

- `wire-transport`: gRPC over HTTP/2 + TLS with key pinning, the single
  `Messaging.Connect` stream, session auth, heartbeats, versioning and size limits.
- `device-identity`: user identity keys, device certificates, invite-based
  signup, device linking and revocation, push token registration.
- `key-directory`: per-device MLS KeyPackage upload, fetch and replenishment.
- `message-delivery`: envelopes, acceptance, idempotency, the strict epoch rule,
  per-device queues, the commit log, catch-up, expiry, acks and wake pings.
- `group-membership`: group creation, who may commit what, the Pi's enforcement
  of it, user removal, and keeping inactive devices out with automatic re-add.
- `message-content`: the encrypted inner payload format (text, group settings,
  user profiles) that only clients read.

### Modified Capabilities

None. `openspec/specs/` is empty; this is the first change.

## Impact

- **New code:** `proto/vyajena/v1/*.proto`, `buf.yaml`, a Cargo workspace with
  `crates/vyajena-proto`, a CI workflow.
- **Docs:** `docs/architecture-decisions.md` (decisions #1 and #5 checked off,
  amendments above), `openspec/project.md` (invariants, open decisions).
- **Downstream:** the Pi server and the Rust core build against this contract in
  follow-up changes. Those changes implement the behavior these specs require;
  this change delivers the contract and validates the two riskiest assumptions
  (OpenMLS `PublicGroup` on the Pi, device certificates in MLS credentials).
- **Flutter:** none. Dart never sees protobuf; its interface to the Rust core is
  the `flutter_rust_bridge` API, which is a separate contract.
