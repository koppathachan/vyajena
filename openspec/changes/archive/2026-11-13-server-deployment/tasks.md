## 1. Spike and server changes

- [x] 1.1 Spike (git-ignored `spikes/`): build the server in a `linux/arm64` `rust:*-bookworm` container on the Mac and run it on the server from a temp folder on a spare port (not <server-port>, nothing installed); record under D2 and delete
- [x] 1.2 Graceful stop on `SIGTERM` in `server/src/main.rs`; `server/tests/` test (server-runtime: Graceful stop)
- [x] 1.3 `vyajena-server backup --to <path>` subcommand (SQLite online backup) with a `server/tests/` test (server-deployment: Backup to the Mac)

## 2. Deploy scripts

- [x] 2.1 `deploy/server/build.sh` (D2) and `deploy/server/config.toml` (D4)
- [x] 2.2 `deploy/server/vyajena.service` and `deploy/server/nftables.conf` (D3, D4)
- [x] 2.3 `deploy/server/harden.sh` (user, sshd `PermitRootLogin no`, nftables, unattended-upgrades without auto-reboot, static `<server-address>` via NetworkManager) and `deploy/server/deploy.sh` with `--rollback` (D5)
- [x] 2.4 `deploy/server/check.sh` (server-deployment: Hardening check) and `deploy/server/backup.sh` (D6); all scripts `shellcheck` clean

## 3. On the server

- [x] 3.1 First deploy to `<server-address>`; `check.sh` passes (including pending-reboot and address-clash reports); record the `systemd-analyze security` score and threshold in D3
- [x] 3.2 The iOS Simulator and the Android emulator (no `adb reverse`) join with invites naming `<server-address>:<server-port>` and chat; record in design.md
- [x] 3.3 Upgrade with a new build keeps the pin and users; `--rollback` works; `backup.sh` produces a valid snapshot; record in design.md
- [x] 3.4 IPv6 exposure test from outside the home network (mobile data): the server's global addresses refuse 22 and <server-port>; recorded in D8 (owner's side session, 2026-11-14)

## 4. Docs and validation

- [x] 4.1 README "Run on the server" (build, deploy, check, backup, bootstrap invite from the journal, the static address); `CLAUDE.md` layout and commands; architecture doc: the home-LAN phase and what the relay change undoes
- [x] 4.2 `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, `buf lint`, `shellcheck deploy/server/*.sh` clean; `openspec validate server-deployment --strict` passes
