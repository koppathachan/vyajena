## Why

The server, the core and the Flutter app all work on the owner's Mac against a
local server. The owner wants the real host running vyajena2 now, so phones
and simulators on the home Wi-Fi chat through it. The old vyajena (Go + Tor) was
purged from the server on 2026-11-13 (backup kept on the owner's Mac).

## What Changes

- New `deploy/server/`: build the server for 64-bit ARM Linux on the Mac (no build
  tools on the server), install or upgrade it on the server with one command, and check the
  server's hardening with another.
- The server runs as a sandboxed systemd service under its own system user, with
  its database and keys in `/var/lib/vyajena` (mode 0600), kept across upgrades.
- Host hardening from the architecture doc: SSH keys only and no root login,
  nftables dropping inbound traffic, unattended security updates, nothing else
  listening.
- **Home-LAN phase:** until the relay exists, the server accepts gRPC on port <server-port>
  from the home network only, and invites name the server's LAN address. The
  architecture doc's "nftables drops all inbound" holds again once the relay
  change moves the server to loopback behind the tunnel.
- The server shuts down cleanly on `SIGTERM` (systemd's stop signal), not only
  on Ctrl-C.
- A backup command copies the server's database and keys to the Mac.
- No `.proto` changes. No open decision resolved: #6 (relay VM) and real push stay
  for the relay change.

## Capabilities

### New Capabilities

- `server-deployment`: how the server is built, installed, upgraded, sandboxed,
  reached on the home network, checked and backed up.

### Modified Capabilities

- `server-runtime`: adds Graceful stop (`SIGTERM`).

## Impact

- **New files:** `deploy/server/` (build, deploy, check and backup scripts; the
  systemd unit; the nftables rules; an example config).
- **Code:** `server/src/main.rs` stops on `SIGTERM`.
- **Server (<server-address>, Linux host, Debian 13 arm64):** new `vyajena` system
  user, `/usr/local/bin/vyajena-server`, `/etc/vyajena/config.toml`,
  `/var/lib/vyajena/`, `vyajena.service`, nftables and unattended-upgrades
  configured, `PermitRootLogin no`.
- **Docs:** README "Run on the server", `CLAUDE.md` commands, architecture doc (the
  LAN phase).
- **Mac:** Docker Desktop builds the arm64 binary in a Linux container.
