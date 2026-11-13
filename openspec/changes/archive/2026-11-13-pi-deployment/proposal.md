## Why

The Pi server, the core and the Flutter app all work on the owner's Mac against a
local Pi. The owner wants the real Raspberry Pi running vyajena2 now, so phones
and simulators on the home Wi-Fi chat through it. The old vyajena (Go + Tor) was
purged from the Pi on 2026-11-13 (backup kept on the owner's Mac).

## What Changes

- New `deploy/pi/`: build the Pi server for 64-bit ARM Linux on the Mac (no build
  tools on the Pi), install or upgrade it on the Pi with one command, and check the
  Pi's hardening with another.
- The server runs as a sandboxed systemd service under its own system user, with
  its database and keys in `/var/lib/vyajena` (mode 0600), kept across upgrades.
- Host hardening from the architecture doc: SSH keys only and no root login,
  nftables dropping inbound traffic, unattended security updates, nothing else
  listening.
- **Home-LAN phase:** until the relay exists, the Pi accepts gRPC on port 8443
  from the home network only, and invites name the Pi's LAN address. The
  architecture doc's "nftables drops all inbound" holds again once the relay
  change moves the server to loopback behind the tunnel.
- The server shuts down cleanly on `SIGTERM` (systemd's stop signal), not only
  on Ctrl-C.
- A backup command copies the Pi's database and keys to the Mac.
- No `.proto` changes. No open decision resolved: #6 (relay VM) and real push stay
  for the relay change.

## Capabilities

### New Capabilities

- `pi-deployment`: how the Pi server is built, installed, upgraded, sandboxed,
  reached on the home network, checked and backed up.

### Modified Capabilities

- `pi-runtime`: adds Graceful stop (`SIGTERM`).

## Impact

- **New files:** `deploy/pi/` (build, deploy, check and backup scripts; the
  systemd unit; the nftables rules; an example config).
- **Code:** `server/src/main.rs` stops on `SIGTERM`.
- **Pi (192.168.1.8, Raspberry Pi 4, Debian 13 arm64):** new `vyajena` system
  user, `/usr/local/bin/vyajena-server`, `/etc/vyajena/config.toml`,
  `/var/lib/vyajena/`, `vyajena.service`, nftables and unattended-upgrades
  configured, `PermitRootLogin no`.
- **Docs:** README "Run on the Pi", `CLAUDE.md` commands, architecture doc (the
  LAN phase).
- **Mac:** Docker Desktop builds the arm64 binary in a Linux container.
