## Why

The repository named one particular deployment: a Raspberry Pi at a home address,
its router, MAC and SSH user, and the relay's tunnel, review and loopback ports. The
software is generic (any Linux host can run the server), and publishing a repo
that carries the real addresses and ports of a live group is needless exposure.
The owner asked (2026-11-20) to remove the information about ports and IPs
everywhere, docs included, and to stop calling the server "the Pi".

## What Changes

- **Real addresses and ports leave the repository.** They live in
  `~/.vyajena-relay/network.conf` (mode 0600, outside the repo), made from
  `deploy/network.conf.example`, loaded by `deploy/network.sh` for every script in
  `deploy/relay/` and `deploy/server/`. Templates and the hardening check use
  placeholders and variables; docs and specs use `<server-address>`, `<tunnel-port>`
  and the like. 443 (devices) and 22 (SSH) stay fixed. The default local-run port
  `127.0.0.1:8443` of `server/local.toml` stays: it is the documented development
  default, not the deployment's.
- **New ports, chosen by the owner and kept out of the repo:** the tunnel, review
  and loopback ports change, so the real ones differ from the old public values.
  Applied at the Mumbai cut-over (change `relay-mumbai`).
- **"The Pi" becomes "the server"** in specs, docs, comments, tests, UI strings and
  names: `deploy/pi/` → `deploy/server/`, capabilities `pi-deployment` and
  `pi-runtime` → `server-deployment` and `server-runtime`, test helpers
  (`TestPi` → `TestServer`), the TLS SNI name `vyajena2-pi` → `vyajena2-server`,
  `PI=` → `SERVER=`, `~/vyajena-pi-backups` → `~/vyajena-server-backups`.
- **The database file** `pi.sqlite` is now `server.sqlite`; deploy moves an existing
  one (and its WAL) once, with the services stopped, before restarting.
- No `.proto` field or wire change; proto comments only. Generated `pb.rs` and the
  Flutter bindings were regenerated for the comment text.

## Capabilities

### New Capabilities

<!-- None. -->

### Modified Capabilities

- `server-deployment`: new requirement "Deployment details stay out of the
  repository". (The capability itself was renamed from `pi-deployment` in place.)

## Impact

- ~380 files (mostly wording), `deploy/` scripts and templates, `deploy/network.*`,
  tests, README, CLAUDE.md, specs and archived changes.
- **Live system, only on the owner's say:** the next `deploy/server/deploy.sh`
  renames the database file, renders the new ports into configs, units and
  nftables, and moves the server's loopback ports. `deploy/relay/create.sh` updates
  the relay's tunnel and review firewall ports (VPC-wide; design D4).
- Left on purpose, and why (design D3): SQL column and table names `pi_sender_*`,
  `pi_removes` (persisted in devices' and the server's databases; renaming would
  break rollback), the history of git, and the two files with the owner's
  uncommitted edits (`openspec/changes/push/tasks.md`, `Runner.xcscheme`).
