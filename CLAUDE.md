# CLAUDE.md: vyajena2

End-to-end encrypted, text-only chat for one admin-run group. **Read
`openspec/config.yaml` first**: it holds the project context, the invariants you
must never break, and the Rust conventions. This file only adds how to work here.

## Workflow

1. Every non-trivial change goes through OpenSpec: `/opsx:propose` → specs →
   design → tasks → `/opsx:apply` → `/opsx:archive`. Check `openspec list` for the
   active change and read its proposal, design, tasks and specs before coding.
2. Proto-first: change `proto/vyajena/v1/` (and its spec) before dependent code.
3. Do `tasks.md` in order; tick a task only when its tests pass.
4. Tests live in the crate's `tests/` folder, never in source files, and name
   the requirement they cover: `// spec: <capability>/<title-slug>`, e.g.
   `// spec: group-membership/commit-authority`.
5. A wrong spec is fixed in the spec first, with the reason in the change's
   `design.md`. Policy and open decisions go to the owner, not decided alone.
6. Spikes are throwaway: `spikes/<name>/` is git-ignored and outside the
   workspace. Record the result in `design.md`, then delete the spike.

## Layout

```
proto/vyajena/v1/       wire contract (package vyajena.v1); buf.yaml at the root
protocol/               rules the Pi and core share (limits, timings, signing labels,
                        certificate checks, commit authority), one module per spec
                        capability; tests in protocol/tests/
server/                 the Pi server: one folder per feature in src/, src/pb.rs
                        (generated server stubs), tests/ with tests/support/pb.rs
                        (generated client stubs); example config server/local.toml
client/                 the device core (library): one folder per feature in src/
                        (keystore, store, identity, net, mls, membership, keys,
                        messaging; engine/ owns the connection and the group),
                        src/pb.rs (generated client stubs), tests/ end to end
                        against the real server (tests/support/)
pbgen/                  regenerates the committed pb.rs files from proto/
openspec/               config.yaml, specs/, changes/
docs/                   architecture-decisions.md (with Mermaid diagrams)
```

## Commands

```
buf lint
buf breaking --against '.git#branch=main'
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
openspec validate <change> --strict
cargo run -p server -- --config server/local.toml   # local Pi on 127.0.0.1:8443
```

The local server prints its one-time bootstrap invite only on first start, and
its data lives in the git-ignored `server/.pi-local/` (README: "Run the server
locally"). Never start it with `server/local.toml` for testing: that uses up the
owner's invite. Tests start their own servers in temp folders.

After changing any `.proto` file, regenerate the committed gRPC code with this
one command, and commit the result with the proto change (CI never runs it):

```
cargo run -p pbgen
```

It writes `server/src/pb.rs` (messages + server stubs),
`server/tests/support/pb.rs` (messages + client stubs) and `client/src/pb.rs`
(messages + client stubs, the device core). Never edit those files by hand.

## Working with other sessions

A tracker session coordinates work, one writer at a time in this checkout.
Commit only your own files; never stash, reset or switch branches; don't push
unless asked.

## Definition of done (per task)

Tests naming the task's requirements pass, clippy and `buf lint` are clean,
`openspec validate <change> --strict` passes, and the task is ticked.
