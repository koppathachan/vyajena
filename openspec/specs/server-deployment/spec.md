# server-deployment Specification

## Purpose
TBD - created by archiving change server-deployment. Update Purpose after archive.
## Requirements
### Requirement: Build for the server
The project SHALL build the server for `aarch64` Linux on the owner's Mac with
one command, inside a Linux arm64 container, producing one release binary that
runs on the server's Debian 13. The same command SHALL build the pinned rathole
version for the tunnel from crates.io sources with its own lock file. The server
SHALL need no compiler or Rust toolchain.

#### Scenario: Build on the Mac
- **WHEN** the owner runs the build command on the Mac
- **THEN** it produces an `aarch64` Linux release binary of the server and of
  rathole 0.5.0, and the server has no `cargo` or `rustc`

### Requirement: Install and upgrade
The project SHALL install or upgrade the server and the tunnel on the server over SSH
with one command: it copies the binaries, the config, the tunnel config and the
systemd units, restarts the services, and checks that they are running. The
tunnel config, which holds the rathole token, SHALL come from the Mac's
`~/.vyajena-relay/` and SHALL be stored on the server readable by root only. The
database and keys in `/var/lib/vyajena` SHALL survive every upgrade, and the
previous server binary SHALL be kept so the owner can roll back.

#### Scenario: First install
- **WHEN** the owner deploys to a server without vyajena
- **THEN** the service starts, creates its keys and database, and the owner can
  read the TLS pin and the bootstrap invite from its journal

#### Scenario: Upgrade keeps users
- **WHEN** the owner deploys a new build after Alice and Bob have joined
- **THEN** the TLS pin is unchanged, Alice and Bob reconnect without re-joining,
  and the old binary is kept as the rollback copy

### Requirement: Sandboxed service
The server SHALL run as the `vyajena` system user, which has no login shell,
under systemd with `NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome`,
`PrivateTmp`, no capabilities, and write access only to `/var/lib/vyajena`, the
filesystem blob-store path when one is configured, and its own private `/tmp`; it
SHALL reach no address but loopback. Its database and private keys SHALL have
mode 0600, and the blob-store path SHALL be owned by `vyajena` with mode 0700.
The tunnel SHALL run as `vyajena-tunnel.service` under a dynamic user with the
same sandbox, no writable path, and no address but loopback and the relay.
systemd SHALL restart either if it crashes. The server enforces this.

#### Scenario: Crash
- **WHEN** the server process is killed
- **THEN** systemd starts it again within seconds, with the same pin

#### Scenario: No writes outside its folders
- **WHEN** the server tries to write anywhere other than `/var/lib/vyajena`, the
  blob-store path or its private `/tmp`
- **THEN** the write fails

#### Scenario: Tunnel dials elsewhere
- **WHEN** the tunnel process tries to connect to any address other than
  loopback and the relay
- **THEN** systemd blocks the connection

### Requirement: Host hardening
The server SHALL allow SSH only with keys and SHALL refuse root logins. nftables SHALL
drop all inbound traffic except replies and SSH from the home network's subnet;
the servers' ports SHALL NOT be reachable from any network. The server SHALL install
security updates unattended and SHALL listen on nothing but SSH and the two
servers (the real one and the review server) on loopback.

#### Scenario: Root login
- **WHEN** someone tries `ssh root@<server-address>` with a valid key
- **THEN** the server refuses the login

#### Scenario: Server port from the home network
- **WHEN** a phone on the home Wi-Fi connects to `<server-address>:<server-port>`
- **THEN** nothing answers: the server listens on loopback only and nftables
  drops the packet

### Requirement: Hardening check
The project SHALL provide one command that checks the server over SSH and reports
each failure by name: service users, sandbox settings of the server, the review
Server and the tunnel, file modes, listening ports, nftables rules, SSH
settings, unattended upgrades, the tunnel attached to the relay, the server
ports closed on the LAN, and that the relay's ports 443 and <review-port> present the
same TLS keys as the server's loopback ports <review-port> and <review-server-port>. It SHALL also warn,
without failing, when a reboot is pending: `/run/reboot-required` exists, or a
kernel of the running kernel's flavour newer than the running one is installed.

#### Scenario: Root login re-enabled
- **WHEN** `PermitRootLogin` on the server is changed back to allow keys
- **THEN** the check fails and names the SSH root-login setting

#### Scenario: Kernel updated, not rebooted
- **WHEN** unattended upgrades installed kernel `6.18.61+rpt-rpi-v8` and the server
  still runs `6.18.60+rpt-rpi-v8`
- **THEN** the check warns `kernel-reboot` naming both versions and still passes

#### Scenario: Tunnel down
- **WHEN** `vyajena-tunnel.service` is stopped
- **THEN** the check fails and names the tunnel

### Requirement: Backup to the Mac
The project SHALL provide one command that copies a consistent snapshot of the
server's database and keys to a folder on the Mac with owner-only permissions, never
into the repository. It SHALL NOT copy image blobs: they are encrypted,
short-lived and re-sendable, and the owner accepts losing them.

#### Scenario: Backup while running
- **WHEN** the owner runs the backup while Alice is chatting
- **THEN** the Mac gets a snapshot that opens as a valid database, and the service
  keeps running

#### Scenario: Blobs are not backed up
- **WHEN** the owner runs the backup with image blobs present in the blob store
- **THEN** the snapshot holds the database and keys only, no blob bytes

### Requirement: Reachable through the relay
The server SHALL listen on `127.0.0.1:<server-port>` only, and its config SHALL name the
relay's DNS name and port 443 (`vyajarelay.purplechaamba.com:443`) as the public
address, so the invites it creates work for any device on any network. The server's
tunnel SHALL dial the relay's static IPv4 directly, so the server needs no DNS.

#### Scenario: Bob joins on mobile data
- **WHEN** the admin invites Bob and Bob pastes the invite on his phone on mobile
  data
- **THEN** the invite names `vyajarelay.purplechaamba.com:443`, Bob's phone connects through the
  relay with the pinned TLS key, and the admin's device adds him

### Requirement: Scripted reset
The project SHALL provide one command that resets the server to a fresh server: it
SHALL first take a backup with the backup command and stop if that fails, ask the
owner to confirm by typing the server id, then delete the database and keys
through the deploy script and start the server fresh. It SHALL save the new
bootstrap invite to an owner-only file outside the repository and print only the
new TLS pin and that file's path.

#### Scenario: Reset for the relay
- **WHEN** the owner runs the reset and types `server` to confirm
- **THEN** a backup lands in `~/vyajena-server-backups/`, the server starts with a new TLS
  pin and an empty database, and the new bootstrap invite is in an owner-only
  file, not on screen

#### Scenario: Backup fails
- **WHEN** the backup step fails during the reset
- **THEN** nothing on the server is deleted

### Requirement: Blob store deployment
The deploy scripts SHALL configure the server's filesystem blob backend with a
stable path, the same on every start, that the owner's auto-mount provides; SHALL
ensure that path exists owned by `vyajena` mode 0700; and SHALL NOT own the mount
itself or stop the server booting or the server serving text when the store is
absent. The hardening check SHALL report the path, its ownership and its mode,
and SHALL warn, without failing, when the store is absent. All of this SHALL go
through `deploy/server/` scripts only, never by hand. The server enforces this.

#### Scenario: Store present
- **WHEN** the owner runs the deploy with the drive auto-mounted at the stable
  path
- **THEN** the path exists owned by `vyajena` mode 0700, the config names it, and
  the server stores blobs there

#### Scenario: Store absent at boot
- **WHEN** the server boots with the drive not mounted at the stable path
- **THEN** it still boots and serves text, and the hardening check warns that the
  blob store is absent without failing

### Requirement: Deployment details stay out of the repository
The project SHALL NOT hold the real IP addresses, networks, router, MAC address,
SSH target, admin username or non-default ports of a deployment in the repository,
its docs or its specs. The deploy scripts SHALL read them from a settings file
outside the repository, `network.conf` in the owner's secrets folder, made from
`deploy/network.conf.example`, and SHALL stop with the missing key's name when the
file or a key is absent. Templates that name an address or port SHALL hold a
placeholder that deploy fills. Ports 443 and 22 are the only fixed public ports.
The scripts and tests enforce this.

#### Scenario: Settings file missing
- **WHEN** the owner runs `deploy/server/deploy.sh` before copying
  `deploy/network.conf.example` to `~/.vyajena-relay/network.conf`
- **THEN** the script stops before any SSH and says where to copy the example

#### Scenario: Templates fill from the settings
- **WHEN** deploy renders `nftables.conf`, `push.toml`, `vyajena-egress.service` and
  `harden.sh` with the example settings
- **THEN** no placeholder is left and the home network, router, address and ports
  of the settings appear in them

#### Scenario: No deployment value is committed
- **WHEN** the tests scan the tracked files
- **THEN** none holds the deployment's former LAN address, MAC address or SSH
  user, and nothing under `deploy/` names a port other than from the example file

