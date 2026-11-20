## Why

The server is reachable only on the home Wi-Fi (`<server-address>:<server-port>`, the home-LAN
phase of `server-deployment`). Nobody can chat away from home, and Play reviewers
cannot reach the server. The planned connectivity was always a dumb TCP relay on
a free GCP VM that the server dials out to, so no port opens at home. The owner
decided the details on 2026-11-15 (relay only; push is the next change).

## What Changes

- **Relay VM:** a new GCP free-tier e2-micro (`vyajena-relay`, US region, project
  `vyajena`), its own VPC, Debian 13, rathole 0.5.0 as a TCP passthrough.
  Reserved static external IPv4 (about $3.65/month), named
  `vyajarelay.purplechaamba.com` (an A record at the owner's DNS host). Firewall: 443
  and the tunnel port public, SSH only from Google IAP, everything else dropped.
  VPC flow logs off, no service account, a volatile journal: the VM keeps nothing.
- **Tunnel:** a sandboxed `vyajena-tunnel.service` on the server (rathole client)
  dials out to the relay, authenticated with a token and Noise (the server pins the
  relay's Noise key). TLS still ends on the server, so the relay sees TLS bytes only.
- **server:** `listen_address = "127.0.0.1:<server-port>"`; invites name `vyajarelay.purplechaamba.com:443`. The
  LAN <server-port> rule and the subnet `IPAddressAllow` go: nftables drops all inbound
  again except SSH from home.
- **Fresh start, no migration:** a scripted reset (`deploy/server/deploy.sh
  --reset`) backs up first, then deletes the server's database and keys and starts
  fresh. New TLS pin, new bootstrap invite; every device re-joins.
- **Scripts:** `deploy/relay/` with one command each (`build.sh`, `secrets.sh`,
  `create.sh`, `deploy.sh`, `check.sh`), like `deploy/server/`; `deploy/server/` builds
  and installs rathole and checks the new server state. All pass shellcheck.
- **Secrets:** the rathole token, the relay's Noise key pair and the relay SSH key
  are made on the Mac in `~/.vyajena-relay/` (0700/0600), never in the repo.
- No `.proto` changes. Real push (FCM/APNs) is the next change, not this one.

## Open decisions resolved

- **"Relay on the shared VM or its own VM?"** → **Its own VM** (owner,
  2026-11-15): a new free-tier e2-micro used by nothing else.

## Capabilities

### New Capabilities

- `relay`: the GCP relay: host, passthrough, tunnel authentication, firewall,
  admin access, no state, updates, secrets, provisioning and its check.

### Modified Capabilities

- `server-deployment`: build and install rathole; the tunnel service sandboxed;
  nftables back to "drop all inbound"; reachable through the relay instead of
  the home network (Reachable on the home network is removed); scripted reset;
  the hardening check covers the tunnel and the relay path.
- `server-runtime`: Configuration gains the "behind the relay" scenario.
- `client-runtime`: Connection lifecycle gains the "relay without a tunnel"
  scenario.

## Impact

- **New files:** `deploy/relay/` (scripts, rathole unit, server config template),
  `deploy/server/vyajena-tunnel.service`, `deploy/server/tunnel.toml` (template).
- **Changed:** `deploy/server/{build,deploy,check,harden}.sh`, `config.toml`,
  `nftables.conf`, `vyajena.service`.
- **Tests:** `server/tests/configuration.rs`, `server/tests/deploy_files.rs`,
  `client/tests/relay.rs`.
- **GCP (`vyajena`):** Compute and IAP APIs, VPC `vyajena-relay`, three
  firewall rules, a static IPv4, VM `vyajena-relay`. Needs billing on the
  project (owner).
- **server:** wiped and started fresh (backup first); `rathole` and
  `vyajena-tunnel.service` installed.
- **Devices:** every device re-joins with a new invite naming `vyajarelay.purplechaamba.com:443`.
- **Docs:** README, `CLAUDE.md`, `docs/architecture-decisions.md` (home-LAN phase
  ends; relay facts; cost and latency).
