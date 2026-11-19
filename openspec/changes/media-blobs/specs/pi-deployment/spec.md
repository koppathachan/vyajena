## MODIFIED Requirements

### Requirement: Sandboxed service
The server SHALL run as the `vyajena` system user, which has no login shell,
under systemd with `NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome`,
`PrivateTmp`, no capabilities, and write access only to `/var/lib/vyajena`, the
filesystem blob-store path when one is configured, and its own private `/tmp`; it
SHALL reach no address but loopback. Its database and private keys SHALL have
mode 0600, and the blob-store path SHALL be owned by `vyajena` with mode 0700.
The tunnel SHALL run as `vyajena-tunnel.service` under a dynamic user with the
same sandbox, no writable path, and no address but loopback and the relay.
systemd SHALL restart either if it crashes. The Pi enforces this.

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

### Requirement: Backup to the Mac
The project SHALL provide one command that copies a consistent snapshot of the
Pi's database and keys to a folder on the Mac with owner-only permissions, never
into the repository. It SHALL NOT copy image blobs: they are encrypted,
short-lived and re-sendable, and the owner accepts losing them.

#### Scenario: Backup while running
- **WHEN** the owner runs the backup while Alice is chatting
- **THEN** the Mac gets a snapshot that opens as a valid database, and the service
  keeps running

#### Scenario: Blobs are not backed up
- **WHEN** the owner runs the backup with image blobs present in the blob store
- **THEN** the snapshot holds the database and keys only, no blob bytes

## ADDED Requirements

### Requirement: Blob store deployment
The deploy scripts SHALL configure the server's filesystem blob backend with a
stable path, the same on every start, that the owner's auto-mount provides; SHALL
ensure that path exists owned by `vyajena` mode 0700; and SHALL NOT own the mount
itself or stop the Pi booting or the server serving text when the store is
absent. The hardening check SHALL report the path, its ownership and its mode,
and SHALL warn, without failing, when the store is absent. All of this SHALL go
through `deploy/pi/` scripts only, never by hand. The Pi enforces this.

#### Scenario: Store present
- **WHEN** the owner runs the deploy with the drive auto-mounted at the stable
  path
- **THEN** the path exists owned by `vyajena` mode 0700, the config names it, and
  the server stores blobs there

#### Scenario: Store absent at boot
- **WHEN** the Pi boots with the drive not mounted at the stable path
- **THEN** it still boots and serves text, and the hardening check warns that the
  blob store is absent without failing
