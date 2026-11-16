## 1. Spike

- [x] 1.1 Spike (git-ignored `spikes/rathole-build/`): build rathole 0.5.0 `--locked` with `server,client,noise` for linux/arm64 and linux/amd64; run relay + Pi tunnel + the real server in Docker with the rendered templates and openssl-made Noise keys; TLS pin and `GetServerInfo` through the relay port, wrong token, wrong relay key, tunnel down; record under D3 and delete

## 2. Tests and the server

- [x] 2.1 `server/tests/configuration.rs`: behind the relay (pi-runtime: Configuration)
- [x] 2.2 `client/tests/relay.rs` with a recording, retargetable test relay: TLS ends on the Pi; a relay to another server is refused (relay: TLS ends on the Pi); relay without a tunnel (client-runtime: Connection lifecycle)
- [x] 2.3 `server/tests/deploy_files.rs`: relay.conf, firewall, tunnel configs, units, secrets, dry run, reset guards (relay: all; pi-deployment: changed and added requirements)

## 3. Deploy scripts

- [x] 3.1 `deploy/relay/`: `relay.conf`, `secrets.sh`, `render.sh`, `build.sh`, `lib.sh`, `ssh.sh`, `create.sh` (`--dry-run`), `deploy.sh` (`--rollback`), `check.sh`, `server.toml`, `vyajena-relay.service` (D2–D8, D10)
- [x] 3.2 `deploy/pi/`: `build.sh` builds rathole; `config.toml`, `tunnel.toml`, `vyajena-tunnel.service` templates; `vyajena.service` loopback only; `nftables.conf` without 8443; `deploy.sh` installs the tunnel and has `--reset` (D5, D9); `check.sh` for the new state (D10)
- [x] 3.3 `shellcheck deploy/pi/*.sh deploy/relay/*.sh` clean (`deploy/.shellcheckrc`)

## 4. The relay on GCP (owner go-ahead first)

- [x] 4.1 Owner: billing open on `purple-eccdf` (budget alert), `gcloud auth login`, yes to the `create.sh --dry-run` commands and ~$3.65/month
- [x] 4.2 `secrets.sh`, `create.sh`, `build.sh`, `deploy.sh`; record the VM, zone and address in the hand-over note (not the repo)
- [x] 4.3 `deploy/relay/check.sh`: 0 failed (tunnel warning expected before 5.2); record the exposure score under D8 (tunnel warnings expected until 5.2)

- [x] 4.4 Move to project `vyajena` (owner, D12): owner links billing; delete the `purple-eccdf` relay (VM, address, rules, subnet, VPC); `create.sh`, `deploy.sh`, `check.sh` in `vyajena`
- [x] 4.5 DNS name (owner, D13): `RELAY_HOST` rendered into invites; owner adds the A record `vyajarelay.purplechaamba.com` → the new static IPv4; `deploy/relay/check.sh` passes `dns`

## 5. Pi reset onto the relay (owner yes first)

- [x] 5.1 Owner: yes to deleting the group (every device re-joins)
- [x] 5.2 `deploy/pi/build.sh`, `deploy/pi/deploy.sh --no-build --reset` (backup first); new pin and bootstrap invite in an owner-only file
- [x] 5.3 `deploy/pi/check.sh` and `deploy/relay/check.sh`: 0 failed; from the Mac, TLS to `vyajarelay.purplechaamba.com:443` presents the new pin and `Devices.GetServerInfo` answers (throwaway client, not the owner's devices); record in design.md

## 6. Docs and validation

- [x] 6.1 README (relay section, "Run on the Pi" without the LAN phase), `CLAUDE.md` layout and commands, `docs/architecture-decisions.md` (home-LAN phase ends, relay facts, open decision closed, cost and latency)
- [x] 6.2 `cargo test --workspace`, clippy `-D warnings`, `cargo fmt --check`, `buf lint`, shellcheck clean; `openspec validate relay --strict` passes
- [x] 6.3 Hand-over note `relay-handover.md` in the tracker's notes folder
