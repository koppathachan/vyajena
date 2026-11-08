## 1. Code generation and skeleton

- [x] 1.1 Check `buf generate` with the community prost and tonic plugins against tonic 0.14; if the output does not compile, add a small workspace generator binary using `tonic-prost-build` + `protox` instead (D2)
- [x] 1.2 Generate and commit `server/src/pb.rs` (server stubs) and `server/tests/support/pb.rs` (client stubs); document the one regenerate command in `CLAUDE.md`
- [x] 1.3 Create the `server` crate with the D1 folders, an empty tonic Router and tracing; add it to the workspace

## 2. Shared rules in protocol/

- [x] 2.1 `device_identity`: certificate signature check and device id derivation, with tests in `protocol/tests/`
- [x] 2.2 `group_membership`: the commit-authority rule on plain types (D3), with tests covering every allowed and rejected case in `group-membership: Commit authority`

## 3. Runtime and storage (pi-runtime)

- [x] 3.1 TOML config with field-level errors (Configuration)
- [x] 3.2 First start: TLS and external sender keys with mode 0600, pin logged, keys reused on restart (First start)
- [x] 3.3 SQLite store on one writer thread: WAL, synchronous FULL, mode 0600, embedded migrations (Durable storage; D5)
- [x] 3.4 Injectable clock (D7) and the example local config on `127.0.0.1:8443` (Running locally)
- [x] 3.5 Bootstrap admin invite created and printed on first start (device-identity: Bootstrap admin)

## 4. Devices and authentication

- [x] 4.1 `GetServerInfo`, `Register` with invites and admin-owned usernames, `GetChallenge`, `Authenticate` with TLS exporter binding and hashed session tokens
- [x] 4.2 Auth interceptor for every other RPC (wire-transport: Session authentication)
- [x] 4.3 `LinkDevice`, `GetSelf` (with `is_admin`), `RevokeDevice`, `UpdatePushToken`

## 5. Keys

- [x] 5.1 `UploadKeyPackages` with full validation, `FetchKeyPackages` with optional device ids, one-time consumption and last-resort fallback

## 6. Admin and membership

- [x] 6.1 `CreateInvite`, `ListInvites`, `RevokeInvite`, `RemoveUser`
- [x] 6.2 `CreateGroup` building the OpenMLS `PublicGroup` from GroupInfo and ratchet tree; persist public state
- [x] 6.3 Commit checks: signature, committer is the authenticated device, commit authority via `protocol` (Pi validates handshakes without group secrets)

## 7. Messaging

- [ ] 7.1 `Connect`: Hello and version check, one stream per device (`ABORTED` for the older one), heartbeats and idle timeout
- [ ] 7.2 Envelope checks: size caps, framing matches envelope, client-sent WELCOME rejected, dedup with `DUPLICATE_MISMATCH`, strict epoch rule
- [ ] 7.3 The D4 write path: `server_seq`, queue rows, commit log, Welcome fan-out, durable `Accepted`
- [ ] 7.4 Ordered delivery and catch-up merged by `server_seq`, cumulative acks, expiry with gap notices, low-KeyPackages notice

## 8. Inactivity and re-add

- [ ] 8.1 Mark devices inactive after 7 days or a full queue; stop queueing, delete their queue, issue and reissue external Remove proposals
- [ ] 8.2 `RemovedNotice` to a returning device and `RejoinNotice` to its user's and the admin's connected devices

## 9. Push

- [ ] 9.1 Push provider trait, logging provider, at most one ping per device per 60 seconds, no ping to connected devices

## 10. Integration tests and local run

- [ ] 10.1 Test harness: in-process server on a random port, temp database and keys, pinned TLS client, OpenMLS test clients, controllable clock
- [ ] 10.2 One or more tagged tests for every server-enforced requirement in `wire-transport`, `device-identity`, `key-directory`, `message-delivery`, `group-membership` and `pi-runtime`, including the two tests dropped with `vyajena-proto`
- [ ] 10.3 README section "Run the server locally": start it, read the pin and bootstrap invite, run the tests

## 11. Docs and validation

- [ ] 11.1 Check off decision #7 in `docs/architecture-decisions.md` with the build order; update `CLAUDE.md` layout and commands
- [ ] 11.2 `cargo clippy --workspace --all-targets -- -D warnings` and `buf lint` clean; `openspec validate pi-server-core --strict` passes
