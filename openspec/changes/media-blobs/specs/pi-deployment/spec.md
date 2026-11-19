## MODIFIED Requirements

### Requirement: Sandboxed service
The server SHALL run as the `vyajena` system user, which has no login shell,
under systemd with `NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome`,
`PrivateTmp`, no capabilities, and write access only to `/var/lib/vyajena`, the
configured `blob_dir` on the external mount, and its own private `/tmp`; it SHALL
reach no address but loopback. When `blob_dir` is set the unit SHALL depend on
its mount (`RequiresMountsFor`) so the drive is mounted before the server writes
a blob. Its database and private keys SHALL have mode 0600, and `blob_dir` SHALL
be owned by `vyajena` with mode 0700. The tunnel SHALL run as
`vyajena-tunnel.service` under a dynamic user with the same sandbox, no writable
path, and no address but loopback and the relay. systemd SHALL restart either if
it crashes. The Pi enforces this.

#### Scenario: Crash
- **WHEN** the server process is killed
- **THEN** systemd starts it again within seconds, with the same pin

#### Scenario: No writes outside its folders
- **WHEN** the server tries to write anywhere other than `/var/lib/vyajena`,
  `blob_dir` or its private `/tmp`
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
- **WHEN** the owner runs the backup with image blobs present under `blob_dir`
- **THEN** the snapshot holds the database and keys only, no blob bytes

## ADDED Requirements

### Requirement: Blob storage mount
The deploy scripts SHALL mount the external thumb drive (ext4) by its filesystem
UUID with `nofail`, create `blob_dir` on it owned by `vyajena` mode 0700, and
configure the server with that `blob_dir`. A missing drive SHALL NOT stop the Pi
from booting or the server from serving text. The hardening check SHALL report
the mount, its ownership and its mode, and SHALL warn, without failing, when the
drive is absent. All of this SHALL be done through `deploy/pi/` scripts only,
never by hand. The Pi enforces this.

#### Scenario: Drive mounted by UUID
- **WHEN** the owner runs the deploy with the thumb drive attached
- **THEN** the drive is mounted by UUID with `nofail`, `blob_dir` exists owned by
  `vyajena` mode 0700, and the server stores blobs there

#### Scenario: Drive absent at boot
- **WHEN** the Pi boots with the thumb drive removed
- **THEN** it still boots and serves text, and the hardening check warns that the
  blob mount is absent without failing
