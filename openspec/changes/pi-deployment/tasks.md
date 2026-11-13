## 1. Spike and server changes

- [x] 1.1 Spike (git-ignored `spikes/`): build the server in a `linux/arm64` `rust:*-bookworm` container on the Mac and run it on the Pi from a temp folder on a spare port (not 8443, nothing installed); record under D2 and delete
- [x] 1.2 Graceful stop on `SIGTERM` in `server/src/main.rs`; `server/tests/` test (pi-runtime: Graceful stop)
- [x] 1.3 `vyajena-server backup --to <path>` subcommand (SQLite online backup) with a `server/tests/` test (pi-deployment: Backup to the Mac)

## 2. Deploy scripts

- [x] 2.1 `deploy/pi/build.sh` (D2) and `deploy/pi/config.toml` (D4)
- [x] 2.2 `deploy/pi/vyajena.service` and `deploy/pi/nftables.conf` (D3, D4)
- [x] 2.3 `deploy/pi/harden.sh` (user, sshd `PermitRootLogin no`, nftables, unattended-upgrades without auto-reboot, static `192.168.1.8` via NetworkManager) and `deploy/pi/deploy.sh` with `--rollback` (D5)
- [x] 2.4 `deploy/pi/check.sh` (pi-deployment: Hardening check) and `deploy/pi/backup.sh` (D6); all scripts `shellcheck` clean

## 3. On the Pi

- [ ] 3.1 First deploy to `192.168.1.8`; `check.sh` passes (including pending-reboot and address-clash reports); record the `systemd-analyze security` score and threshold in D3
- [ ] 3.2 The iOS Simulator and the Android emulator (no `adb reverse`) join with invites naming `192.168.1.8:8443` and chat; record in design.md
- [ ] 3.3 Upgrade with a new build keeps the pin and users; `--rollback` works; `backup.sh` produces a valid snapshot; record in design.md

## 4. Docs and validation

- [ ] 4.1 README "Run on the Pi" (build, deploy, check, backup, bootstrap invite from the journal, the static address); `CLAUDE.md` layout and commands; architecture doc: the home-LAN phase and what the relay change undoes
- [ ] 4.2 `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, `buf lint`, `shellcheck deploy/pi/*.sh` clean; `openspec validate pi-deployment --strict` passes
