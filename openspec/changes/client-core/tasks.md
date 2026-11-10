## 1. Spikes and code generation

- [x] 1.1 Spike (git-ignored `spikes/`): persistent OpenMLS storage on SQLite (an existing crate or a `StorageProvider` over rusqlite) that survives restart mid-group, with every value encrypted under a storage key from the key store (D3); record the result in design.md and delete the spike
- [x] 1.2 Spike: `keyring` on macOS (prompts, persistence, headless behaviour); record the result and delete the spike
- [x] 1.3 Add a `client/src/pb.rs` target (messages + client stubs) to `pbgen`; generate and commit it

## 2. Skeleton and shared rules

- [x] 2.1 Create the `client` crate with the D1 folders, the public `Client` type, event types and an injectable clock; add it to the workspace
- [x] 2.2 Add the 7-day leaf and last-resort age constants to `protocol/` with tests

## 3. Key storage and local store

- [x] 3.1 `KeyStore` trait with OS keychain, opt-in 0600 file and memory implementations (client-runtime: Key storage)
- [x] 3.2 SQLite store: messages with send status, own sent plaintext, pinned identity keys, OpenMLS state encrypted under the storage key (D3; client-runtime: Key storage, Local message store)

## 4. Identity, devices and networking

- [x] 4.1 Identity key, device auth and MLS keys, certificates; register from an invite payload; link request and approval; revoke; push token placeholder
- [x] 4.2 Pinned TLS to the relay address, challenge auth with the TLS exporter, token renewal, one `Connect` stream with heartbeats and reconnect with backoff (client-runtime: Connection lifecycle)

## 5. MLS and membership

- [x] 5.1 Admin: create the group (0xF0A1, Pi as external sender), add a user (fetch KeyPackages, commit with Welcome), remove a user, invites
- [x] 5.2 Process commits and Welcomes: certificate and leaf-key checks, client-side commit authority, identity pinning with key-changed events (key-directory: Clients verify the identity chain)
- [x] 5.3 Automatic re-add on `RejoinNotice`; admin commits pending Pi Removes
- [x] 5.4 KeyPackages: initial upload with 0xF0A1, replenish on low notice, last-resort refresh; own-leaf rotation with jitter (decision #4)
- [ ] 5.5 Proto `Keys.GetIdentityKeys` (proto first, then `cargo run -p pbgen`), the Pi handler, and the core learning identity keys through it instead of `FetchKeyPackages` (key-directory: Identity key lookup; owner 2026-11-11)
- [ ] 5.6 Members accept an admin-committed Add whose device needs a new identity key after looking it up: re-pin and emit `IdentityKeyChanged`; reject the same from a non-admin (key-directory: Clients verify the identity chain; group-membership: Device-level membership)
- [ ] 5.7 Pi delivers every entry below an accepted commit's `server_seq` on the committer's stream before its `Accepted`; remove the core's 200 ms settle wait and update D4 (message-delivery: Ordered delivery and catch-up)

## 6. Messaging

- [ ] 6.1 Content bodies: Text with the size check, GroupSettings (admin only), Profile (own user only), unknown bodies as a placeholder (message-content)
- [ ] 6.2 Send with status, `STALE_EPOCH` re-encrypt and resend with the same `msg_id` (client-runtime: Send status)
- [ ] 6.3 Receive in `server_seq` order, store before ack, gap / removed / rejoin notices, one ordered event stream (client-runtime: Events in order)

## 7. End-to-end tests

- [ ] 7.1 Harness: real server in-process, clients with memory key stores and temp databases, controllable clocks
- [ ] 7.2 Tagged tests for every client-side requirement and every new requirement in this change, including the dropped "unknown content body decodes as unset" test; a coverage appendix in design.md

## 8. Docs and validation

- [ ] 8.1 `docs/architecture-decisions.md`: check off #2 (Q&A over PAKE, coming in `qa-verification`) and #4, and record desktop key storage; update the key-storage invariant in `openspec/config.yaml`; `CLAUDE.md` layout and README status
- [ ] 8.2 `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, `buf lint` clean; `openspec validate client-core --strict` passes
