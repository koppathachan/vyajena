# CLAUDE.md: vyajena2

End-to-end encrypted chat of text and pictures for one admin-run group plus DMs between
its members. **Read
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
protocol/               rules the server and core share (limits, timings, signing labels,
                        certificate checks, commit authority), one module per spec
                        capability; tests in protocol/tests/
server/                 the server: one folder per feature in src/, src/pb.rs
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
                        the real app on a simulator (run.sh starts a temp server)
pbgen/                  regenerates the committed pb.rs files from proto/
deploy/server/              the real server: build.sh (arm64 container builds: server +
                        rathole), deploy.sh (harden + install/upgrade/--rollback/
                        --reset over SSH), check.sh (hardening check), backup.sh,
                        harden.sh (run by deploy), vyajena.service,
                        vyajena-tunnel.service, nftables.conf, config.toml and
                        tunnel.toml (templates, filled by deploy/relay/render.sh)
deploy/network.conf.example  the real addresses/ports are NOT in the repo: copy it to
                        ~/.vyajena-relay/network.conf; deploy/network.sh loads it
deploy/relay/           the GCP relay (rathole): relay.conf (names, sizes,
                        versions), secrets.sh, create.sh (gcloud, --dry-run),
                        build.sh, deploy.sh, check.sh, ssh.sh (IAP), render.sh,
                        lib.sh, server.toml, vyajena-relay.service
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
cargo run -p server -- --config server/local.toml   # local server on 127.0.0.1:8443
cd app && flutter analyze
cd app && flutter test                       # widget tests (what CI runs)
app/integration_test/run.sh <device id>      # the app on a simulator/emulator
shellcheck deploy/server/*.sh deploy/relay/*.sh
deploy/server/build.sh && deploy/server/deploy.sh --no-build   # the real server (its address is in ~/.vyajena-relay/network.conf)
deploy/server/check.sh                           # the server's hardening; must pass
deploy/server/backup.sh                          # DB + keys -> ~/vyajena-server-backups/
deploy/relay/create.sh --dry-run             # the relay's gcloud commands, run nothing
deploy/relay/deploy.sh                       # the relay VM (needs gcloud auth login)
deploy/relay/check.sh                        # the relay's hardening; must pass
```

The real server holds the owner's group: deploy to it only when a change says so,
never run tests against it, and never touch its `/var/lib/vyajena` by hand.
Its first start saved the owner's bootstrap invite to an owner-only file; a
fresh database there (only ever through `deploy.sh --reset`, which backs up
first) means a new pin and re-joining every device. Before any `gcloud`
action that creates or changes something, show the owner the commands
(`create.sh --dry-run`) and the cost and wait for a yes; the owner runs
`gcloud auth login` themselves. No real IP or port of the deployment goes into the repo, docs or
specs: they live in `~/.vyajena-relay/network.conf` (placeholders in the text).
The relay's secrets live only in
`~/.vyajena-relay/`: never in the repo, never printed.

Dart tests live in `app/test/` and `app/integration_test/`, never in `lib/`,
tagged `// spec: <capability>/<title-slug>` like the Rust ones. `run.sh`
starts its own temp server and the admin's device on the host; README "Run the
app locally" covers simulators, `adb reverse` and the Xcode 27.2 caveat.

The local server prints its one-time bootstrap invite only on first start, and
its data lives in the git-ignored `server/.server-local/` (README: "Run the server
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
