## Why

The Pi server works, but nothing can talk to it: every message is MLS ciphertext,
so chatting needs a device-side core. It is step 2 of the owner's build order and
the base for the terminal client (step 3) and the Flutter app (step 5).

## What Changes

- New `client/` crate: the device core as a Rust library. It runs OpenMLS, holds
  keys, stores messages, talks to the Pi over pinned TLS, and exposes one async
  API plus an event stream that the terminal client and Flutter both use.
- Client stubs are generated into a committed `client/src/pb.rs` by the existing
  `cargo run -p pbgen`, never in CI.
- Implements every client-side requirement of the main specs: all of
  `message-content`, identity pinning in `key-directory`, the client half of the
  commit-authority check, and the device side of signup, linking, sending,
  catch-up, inactivity and automatic re-add.
- **Resolves open decision #4 (key rotation):** each device commits an update of
  its own leaf and replaces its last-resort KeyPackage every 7 days, with jitter.
- **Records open decision #2 (verification):** the owner chose question-and-answer
  verification over a PAKE instead of fingerprints. It ships in its own
  `qa-verification` change after the terminal client; this change ships identity
  pinning and the key-changed warning it builds on.
- **Changes an invariant (owner-approved):** on desktop, keys live in the OS
  keychain (macOS Keychain, Linux Secret Service, Windows Credential Manager);
  only on a machine without one, and only with an explicit opt-in, they live in a
  key file with mode 0600.
- No `.proto` changes.

## Capabilities

### New Capabilities

- `client-runtime`: how the device core keeps keys and messages, connects to the
  Pi, reports send status, and hands events to the UI in order.

### Modified Capabilities

- `group-membership`: adds own-leaf rotation every 7 days (decision #4).
- `key-directory`: adds last-resort KeyPackage replacement every 7 days
  (decision #4).

## Impact

- **New code:** `client/` (library, feature folders, committed `pb.rs`, tests),
  a client target in `pbgen`, constants in `protocol/`.
- **Dependencies:** openmls 0.9 and its storage, tonic 0.14 client, rustls,
  rusqlite (bundled), keyring, ed25519-dalek.
- **Docs:** `docs/architecture-decisions.md` (#2 and #4 checked off, desktop key
  storage), `openspec/config.yaml` context (the key-storage invariant),
  `CLAUDE.md` layout, README status.
- **Tests:** end-to-end tests run Alice, Bob and the admin as real clients against
  the real server in-process.
