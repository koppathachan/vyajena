## Why

Play and App Store reviewers must be able to sign in and chat, but they must
never join the real group or see its members (owner, 2026-11-16: "we can't let
reviewers access to internal chats"). The relay made the server public, so a
second, empty server for reviewers is now cheap. app-release needs its address
for Play's App access form.

## What Changes

- **Review server on the server:** a second instance of the same server binary,
  `vyajena-review.service`, under its own system user `vyajena-review`, with its
  own data and keys in `/var/lib/vyajena-review`, its own TLS pin and bootstrap
  invite. It listens on `127.0.0.1:<review-server-port>`. Ids: `review@review.purplechaamba.com`
  (owner).
- **Review route through the relay:** a second rathole service in the same
  tunnel with its own token: `vyajarelay.purplechaamba.com:<review-port>` → the server's
  `127.0.0.1:<review-server-port>`. One more GCP firewall rule (`tcp:<review-port>`, free).
- **Review admin:** a new throwaway iOS simulator ("Vyajena Review") holds the
  review admin; it creates each reviewer's single-use invite before a review.
- **Ops:** `deploy/server/deploy.sh` installs and upgrades both servers;
  `--reset-review` wipes only the review server (no backup: it holds no real
  data). `check.sh` covers the review server and fails if both servers ever
  share a pin; `backup.sh` skips it.
- The real group's server, data and pin are untouched. No `.proto` or app
  changes: the app already dials any `host:port` from an invite.

## Capabilities

### New Capabilities

- `review-server`: the separate server for store reviewers: isolation, route,
  admin and invites, reset, checks.

### Modified Capabilities

- `relay`: the firewall also opens the review port; the relay check expects it.
- `server-deployment`: the server listens on two loopback ports; the hardening check
  covers the review server.

## Impact

- **New files:** `deploy/server/review.toml`, `deploy/server/vyajena-review.service`.
- **Changed:** `deploy/relay/{relay.conf,server.toml,render.sh,secrets.sh,create.sh,check.sh}`,
  `deploy/server/{tunnel.toml,deploy.sh,check.sh,harden.sh}`, `server/tests/deploy_files.rs`.
- **GCP (`vyajena`):** firewall rule `vyajena-relay-review` (tcp:<review-port>, from
  anywhere, logging off). No cost.
- **Relay VM:** new config; rathole restarts (the real group's devices
  reconnect within seconds).
- **server:** new user, unit, config and data dir; the tunnel gains a service.
- **Secrets:** `~/.vyajena-relay/review-token`.
- **Hand-off:** app-release gets `vyajarelay.purplechaamba.com:<review-port>`; the owner
  pastes a reviewer invite into Play.
