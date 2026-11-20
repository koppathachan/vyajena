## MODIFIED Requirements

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
`PrivateTmp`, no capabilities, and write access only to `/var/lib/vyajena` and
its own private `/tmp`; it SHALL reach no address but loopback. Its
database and private keys SHALL have mode 0600. The tunnel SHALL run as
`vyajena-tunnel.service` under a dynamic user with the same sandbox, no writable
path, and no address but loopback and the relay. systemd SHALL restart either
if it crashes. The server enforces this.

#### Scenario: Crash
- **WHEN** the server process is killed
- **THEN** systemd starts it again within seconds, with the same pin

#### Scenario: No writes outside its folder
- **WHEN** the server tries to write anywhere other than `/var/lib/vyajena` or
  its private `/tmp`
- **THEN** the write fails

#### Scenario: Tunnel dials elsewhere
- **WHEN** the tunnel process tries to connect to any address other than
  loopback and the relay
- **THEN** systemd blocks the connection

### Requirement: Host hardening
The server SHALL allow SSH only with keys and SHALL refuse root logins. nftables SHALL
drop all inbound traffic except replies and SSH from the home network's subnet;
the server's port SHALL NOT be reachable from any network. The server SHALL install
security updates unattended and SHALL listen on nothing but SSH and the server on
loopback.

#### Scenario: Root login
- **WHEN** someone tries `ssh root@<server-address>` with a valid key
- **THEN** the server refuses the login

#### Scenario: Server port from the home network
- **WHEN** a phone on the home Wi-Fi connects to `<server-address>:<server-port>`
- **THEN** nothing answers: the server listens on loopback only and nftables
  drops the packet

### Requirement: Hardening check
The project SHALL provide one command that checks the server over SSH and reports
each failure by name: service user, sandbox settings of the server and the
tunnel, file modes, listening ports, nftables rules, SSH settings, unattended
upgrades, the tunnel attached to the relay, the server port closed on the LAN,
and that the relay's port 443 presents the same TLS key as the server's loopback
port. It SHALL also warn, without failing, when a reboot is pending:
`/run/reboot-required` exists, or a kernel of the running kernel's flavour newer
than the running one is installed.

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

## ADDED Requirements

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

## REMOVED Requirements

### Requirement: Reachable on the home network
**Reason**: the home-LAN phase ends; the relay replaces it (Reachable through the
relay).
**Migration**: none. The server is reset; every device re-joins with an invite
naming the relay.
