## 1. Tooling

- [x] 1.1 Create `proto/vyajena/v1/` and `buf.yaml` with DEFAULT lint rules and FILE breaking rules
- [x] 1.2 Create a Cargo workspace at the repo root with an empty `crates/vyajena-proto` crate
- [x] 1.3 Wire `tonic-build` / `prost-build` in `crates/vyajena-proto/build.rs` to compile every file under `proto/`

## 2. Contract

- [x] 2.1 Write `common.proto`: `UserId`, `DeviceRef`, `DeviceCertificate` and its signed wrapper, rejection reason enum
- [x] 2.2 Write `devices.proto`: `Devices` service (GetServerInfo, Register, GetChallenge, Authenticate, LinkDevice, GetSelf, RevokeDevice, UpdatePushToken) and `ServerInfo` with limits and external sender key
- [x] 2.3 Write `keys.proto`: `Keys` service (UploadKeyPackages, FetchKeyPackages) returning KeyPackage + certificate + identity key per device
- [x] 2.4 Write `admin.proto`: `Admin` service (CreateInvite, ListInvites, RevokeInvite, CreateGroup, RemoveUser) and the `InvitePayload` (relay address, TLS pin, code)
- [x] 2.5 Write `messaging.proto`: `Messaging.Connect` bidi stream with `ClientFrame` (Hello, Envelope, Ack, Heartbeat) and `ServerFrame` (Envelope, SendResult, GapNotice, LowKeyPackages, RemovedNotice, RejoinNotice, Heartbeat), and `Envelope` with `Kind`
- [x] 2.6 Write `content.proto`: `Content` with `sent_at` and `oneof body { Text, GroupSettings, Profile }`
- [x] 2.7 Document every normative rule in proto comments (size caps, signing context strings, who may call what), referencing the spec requirement it comes from
- [x] 2.8 Export the limits (65,536 / 262,144 bytes, KeyPackage counts, heartbeat timings) and context strings as constants from `vyajena-proto`
- [x] 2.9 `buf lint` passes and `cargo build -p vyajena-proto` succeeds

## 3. CI

- [x] 3.1 Add a GitHub Actions workflow running `buf lint`, `buf breaking --against main` and `cargo build --workspace` on pull requests and pushes to main

## 4. De-risking spikes

- [x] 4.1 Spike: OpenMLS `PublicGroup` on a non-member verifies a `PublicMessage` commit (Add, Remove, self Update, external Remove proposal, and a re-add commit combining the pending external Remove with an Add) and applies it; record the outcome in `design.md`
- [x] 4.2 Spike: an OpenMLS `BasicCredential` whose identity is a serialized `DeviceCertificate` round-trips through KeyPackage creation, Add and Welcome; record the outcome in `design.md`
- [x] 4.3 If either spike fails, update D4/D5 and the affected specs before continuing

## 5. Docs

- [ ] 5.1 In `docs/architecture-decisions.md`, check off multi-device and max message size, and record the relaxed commit rule, the catch-up ordering fix, admin id in the group context, group settings in encrypted messages, admin-owned usernames with user-owned profiles, and the 7-day / full-queue inactivity rule with automatic re-add (replacing `QUEUE_FULL` rejection and the 30-day removal, including the delivery state diagram)
- [ ] 5.2 Update `openspec/project.md` invariants and open decisions to match
- [ ] 5.3 Run `openspec validate core-wire-protocol --strict` and fix any findings

## 6. Contract gaps

- [x] 6.1 Bind each Welcome to its commit: `Envelope.welcome` (`WelcomeBundle`) on the `COMMIT` that creates it, accepted or rejected together; the Pi emits `WELCOME` envelopes only after accepting the commit; a client-sent `WELCOME` is `MALFORMED`; the 256 KB cap covers the whole `COMMIT` envelope (message-delivery, group-membership, wire-transport, D7 and linking diagrams)
- [x] 6.2 Add optional `device_ids` to `FetchKeyPackagesRequest`; the Pi returns and consumes KeyPackages only for those devices (key-directory, D7 re-add)
- [x] 6.3 Return `is_admin` from `Devices.GetSelf` so the admin's app knows to call `Admin.CreateGroup` (device-identity, D13)
- [x] 6.4 Pin the TLS exporter label `EXPORTER-vyajena2-auth-v1` (32 bytes), the 32-byte single-use challenge nonce valid 60 seconds, and status codes `PERMISSION_DENIED` (Register certificate for another user), `NOT_FOUND` (fetch for an unknown user) and `ABORTED` (older `Connect` stream replaced) (wire-transport, device-identity, key-directory, protos, `vyajena-proto` constants)
