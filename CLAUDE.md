# CLAUDE.md: vyajena2

You are working on **vyajena2**, an end-to-end encrypted, text-only chat for one
admin-run group. Four parts: Flutter UI (view only), Rust core on each device
(OpenMLS, networking, SQLite, OS key store), a dumb GCP TCP relay, and a Rust Pi
server (tokio + tonic, SQLite WAL). The admin and owner is Akhil (`koppathachan`).

Source of truth, in this order: `docs/architecture-decisions.md`, the `.proto`
contract in `proto/vyajena/v1/`, then the OpenSpec specs. Specs refine the first
two; they never quietly contradict them.

## Workflow (spec-driven, OpenSpec)

1. Read `openspec/project.md`, then the active change: `openspec list`, then
   `openspec/changes/<change>/{proposal,design,tasks}.md` and its `specs/`.
2. Every non-trivial change goes through OpenSpec: `/opsx:propose` → specs →
   design → tasks → implement (`/opsx:apply`) → `/opsx:archive`. No ad hoc
   features.
3. **Proto-first.** Change the `.proto` contract (and its spec) before any Rust
   that depends on it. Flutter never sees protobuf.
4. Work through `tasks.md` **in order**. Tick `- [x]` only when the task's tests
   pass.
5. Every spec requirement needs at least one test that names it:
   `// spec: <capability>/<requirement-slug>`, where the slug is the requirement
   title in kebab case, e.g. `// spec: group-membership/commit-authority`,
   `// spec: message-delivery/strict-epoch-rule`.
6. If implementation shows a spec is wrong, **update the spec first** and record
   why in the change's `design.md` (next to the decision it touches, or under
   Open Questions if the owner must decide). Then change the code.
7. **Spikes are research.** Code goes in a standalone project under
   `spikes/<name>/` (excluded from the workspace); the finding goes into the
   change's `design.md` under the decision it tests, as "Spike result". Don't
   guess past a spike.
8. **Don't decide policy alone.** Open decisions in the architecture doc, and
   anything that changes what users or the Pi can do, go to the owner. Pick the
   narrowest option that leaves room, and say so.
9. Run `openspec validate <change> --strict` before finishing a milestone.

## Golden rules (never break)

1. **Plaintext never leaves the Rust core.** The relay and the Pi only ever see
   ciphertext. Never log plaintext, decrypted content, or key material anywhere.
2. **Flutter never opens a network connection.** All networking is in the Rust
   core; the UI talks to it only through `flutter_rust_bridge`.
3. **The Pi is the authority for routing, ordering and membership, never for
   history.** It stores only what `group-membership/membership-state-on-the-pi`
   and the architecture doc list. Group names, settings and profiles stay inside
   encrypted messages.
4. **Strict epoch rule.** The Pi rejects any envelope not in the group's current
   epoch. OpenMLS keeps `max_past_epochs = 0`.
5. **Commit authority is checked twice.** Admin devices may commit anything;
   other devices only their own leaf update and adding or removing their own
   user's devices, plus Pi-issued inactivity Removes. The Pi and every client
   enforce the same rule.
6. **OpenMLS does not check credentials.** The core verifies every new or
   changed leaf's device certificate against the user's pinned identity key,
   and that the certificate's MLS key equals the leaf's signature key.
7. **Accepted means durable.** The Pi never sends `Accepted` before the envelope
   and all its queue and log entries are committed to disk.
8. **Never drop silently.** Every rejection or loss reaches someone:
   `SendResult`, `GapNotice`, `RemovedNotice`.
9. **Keys live in the OS key store** (Android Keystore / iOS Keychain), never in
   SQLite, files or logs. There is no other encryption at rest.
10. **Network bytes are untrusted input.** No `unwrap()`/`expect()`/panicking
    indexing on anything from the wire, MLS, or the database. Use checked
    parsing that returns an error.
11. `#![forbid(unsafe_code)]` in every crate, except generated
    `flutter_rust_bridge` glue, which stays isolated in its own module.
12. Limits, counts, timings and signing context strings come from the constants
    in `vyajena-proto`. Never hardcode 65,536, 262,144, 7 days, etc.

## Layout

```
proto/vyajena/v1/   the wire contract (package vyajena.v1); buf.yaml at the root
crates/
  vyajena-proto     generated gRPC code (tonic + prost, protos via protox) and constants
spikes/
  openmls/          standalone OpenMLS spikes (PublicGroup on the Pi, certificate credentials)
openspec/           project.md, changes/, specs/
docs/               architecture-decisions.md (with Mermaid diagrams)
```

The Pi server, the Rust core and the Flutter app arrive in later OpenSpec
changes; add them to this list when they land.

## Commands

```
buf lint
buf breaking --against '.git#branch=main'
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
(cd spikes/openmls && cargo test)
openspec list
openspec validate <change> --strict
```

## Conventions

- Errors: `thiserror` in libraries, `anyhow` only in binaries. Map to gRPC status
  codes (unary) or `RejectionReason` (stream) at the server boundary.
- Logging: `tracing`. Log ids, epochs, sequence numbers and sizes; never content
  or keys (golden rule 1).
- Ids: users are `<username>@<server_id>`, the username chosen by the admin. A
  device id is the first 16 bytes of SHA-256 of its auth public key.
- Proto comments state each normative rule and name the spec requirement it
  comes from.
- Docs: diagrams are Mermaid blocks in markdown (the architecture doc for the
  whole system, a change's `design.md` for that change). Specs stay plain text.
- Keep each crate's public API small. The Pi knows nothing of plaintext; the
  core knows nothing of the Pi's storage.

## Working with other sessions

Work is often split across sessions coordinated by a tracker session, one
writer at a time in this checkout. Commit only the files you changed, on the
branch you were given; never stash, reset or switch branches; don't push unless
asked.

## Definition of done (per task)

- Tests that name the task's spec requirements pass.
- `cargo clippy --workspace -- -D warnings` and `buf lint` are clean.
- `openspec validate <change> --strict` passes.
- The task is ticked in `tasks.md`, and any spec or design change is recorded.
