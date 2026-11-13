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
bridge/                 flutter_rust_bridge glue around client::Client (no logic):
                        src/api/ (what Dart sees), src/frb_generated.rs (generated),
                        tests/ against the real server (reuses client/tests/support);
                        tests/app_peer.rs is the host side of the app's integration
                        test (ignored by cargo test)
app/                    the Flutter app, iOS + Android only: lib/<feature>/ screens,
                        lib/core/ (CoreService, the one door to the bridge),
                        lib/src/rust/ (generated Dart bindings), rust_builder/
                        (cargokit: builds bridge/ from Xcode and Gradle); test/
                        widget tests on a fake core (test/support/), integration_test/
                        the real app on a simulator (run.sh starts a temp Pi)
pbgen/                  regenerates the committed pb.rs files from proto/
deploy/pi/              the real Pi: build.sh (arm64 container build), deploy.sh
                        (harden + install/upgrade/--rollback over SSH), check.sh
                        (hardening check), backup.sh, harden.sh (run by deploy),
                        vyajena.service, nftables.conf, config.toml
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
cd app && flutter analyze
cd app && flutter test                       # widget tests (what CI runs)
app/integration_test/run.sh <device id>      # the app on a simulator/emulator
shellcheck deploy/pi/*.sh
deploy/pi/build.sh && deploy/pi/deploy.sh --no-build   # the real Pi (192.168.1.8)
deploy/pi/check.sh                           # the Pi's hardening; must pass
deploy/pi/backup.sh                          # DB + keys -> ~/vyajena-pi-backups/
```

The real Pi holds the owner's group: deploy to it only when a change says so,
never run tests against it, and never touch its `/var/lib/vyajena` by hand.
Its first start printed the owner's bootstrap invite; a fresh database there
means a new pin and re-joining every device.

Dart tests live in `app/test/` and `app/integration_test/`, never in `lib/`,
tagged `// spec: <capability>/<title-slug>` like the Rust ones. `run.sh`
starts its own temp Pi and the admin's device on the host; README "Run the
app locally" covers simulators, `adb reverse` and the Xcode 27.2 caveat.

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

After changing anything in `bridge/src/api/`, regenerate the committed
Flutter bindings with this one command, run in `app/`, and commit the result
with the change (CI never runs it):

```
cd app && flutter_rust_bridge_codegen generate
```

It reads `app/flutter_rust_bridge.yaml` and writes `bridge/src/frb_generated.rs`
and `app/lib/src/rust/` (including `*.freezed.dart`, through build_runner).
It needs `flutter_rust_bridge_codegen` 2.12.0, the same version as the
`flutter_rust_bridge` dependency on both sides. Never edit those files by hand.

## Working with other sessions

A tracker session coordinates work, one writer at a time in this checkout.
Commit only your own files; never stash, reset or switch branches; don't push
unless asked.

## Definition of done (per task)

Tests naming the task's requirements pass, clippy and `buf lint` are clean,
`openspec validate <change> --strict` passes, and the task is ticked.
