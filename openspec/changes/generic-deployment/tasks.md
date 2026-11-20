## 1. Proto and shared rules

- [x] 1.1 No wire change: proto comments only; `cargo run -p pbgen` and `flutter_rust_bridge_codegen generate` re-run

## 2. Spec

- [x] 2.1 `server-deployment`: ADD "Deployment details stay out of the repository"; `openspec validate generic-deployment --strict`

## 3. Generic naming

- [x] 3.1 "Pi" → "server" in code, tests, docs, specs, UI strings; files and folders renamed (`deploy/server/`, `server-deployment`, `server-runtime`, `TestServer`)
- [x] 3.2 `server.sqlite`; deploy's one-time move of an existing `pi.sqlite` (`remote_rename_legacy_database`)

## 4. Details out of the repo

- [x] 4.1 `deploy/network.conf.example`, `deploy/network.sh`; relay and server scripts source it
- [x] 4.2 Templates use placeholders; `render.sh server-file` renders `harden.sh`, `nftables.conf`, `push.toml`, the egress unit; `deploy.sh` passes the ports to the host side
- [x] 4.3 `check.sh` takes its addresses and ports from `network.conf`; check names carry no numbers
- [x] 4.4 `create.sh --update-ports` brings the tunnel and review firewall rules to the new ports (not by default)
- [x] 4.5 Docs, specs and archived changes: placeholders for addresses and ports
- [x] 4.6 Tests: example settings in the sandbox; templates fill; no deployment value is committed

## 5. Verify

- [x] 5.1 `cargo build`, `cargo test`, `cargo clippy -D warnings`, `cargo fmt --check`, `buf lint`, `shellcheck`, `openspec validate --all --strict`, `flutter analyze`, `flutter test`
- [x] 5.2 Report the commit list; do not push without the owner's yes

## 6. Owner steps (nothing runs without a yes)

- [x] 6.1 Make `~/.vyajena-relay/network.conf` from the example with real values and new ports; back it up
- [x] 6.2 Do `relay-mumbai` 4.x with it; then `deploy/server/deploy.sh --no-build` and both `check.sh` 0 failed
