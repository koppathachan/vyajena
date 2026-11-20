## ADDED Requirements

### Requirement: Build for the server
The project SHALL build the server for `aarch64` Linux on the owner's Mac with
one command, inside a Linux arm64 container, producing one release binary that
runs on the server's Debian 13. The server SHALL need no compiler or Rust toolchain.

#### Scenario: Build on the Mac
- **WHEN** the owner runs the build command on the Mac
- **THEN** it produces an `aarch64` Linux release binary of the server, and the
  Server has no `cargo` or `rustc`

### Requirement: Install and upgrade
The project SHALL install or upgrade the server on the server over SSH with one
command: it copies the binary, the config and the systemd unit, restarts the
service, and checks that it is running. The database and keys in
`/var/lib/vyajena` SHALL survive every upgrade, and the previous binary SHALL be
kept so the owner can roll back.

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
its own private `/tmp`. Its
database and private keys SHALL have mode 0600. systemd SHALL restart it if it
crashes. The server enforces this.

#### Scenario: Crash
- **WHEN** the server process is killed
- **THEN** systemd starts it again within seconds, with the same pin

#### Scenario: No writes outside its folder
- **WHEN** the server tries to write anywhere other than `/var/lib/vyajena` or
  its private `/tmp`
- **THEN** the write fails

### Requirement: Host hardening
The server SHALL allow SSH only with keys and SHALL refuse root logins. nftables SHALL
drop all inbound traffic except SSH and, during the home-LAN phase, the server's
port from the home network's subnet. The server SHALL install security updates
unattended and SHALL listen on nothing but SSH and the server.

#### Scenario: Root login
- **WHEN** someone tries `ssh root@<server-address>` with a valid key
- **THEN** the server refuses the login

#### Scenario: Traffic from outside the home network
- **WHEN** a packet for port <server-port> arrives from an address outside the home subnet
- **THEN** nftables drops it

### Requirement: Reachable on the home network
During the home-LAN phase the server SHALL listen on port <server-port> on the server's LAN
address, and its config SHALL name that address as the public address, so the
invites it creates work for any device on the home Wi-Fi, including the iOS
Simulator and the Android emulator on the Mac without `adb reverse`.

#### Scenario: Bob joins from his phone at home
- **WHEN** the admin invites Bob and Bob pastes the invite on his phone on the
  home Wi-Fi
- **THEN** the invite names `<server-address>:<server-port>`, Bob's phone connects with the
  pinned TLS key, and the admin's device adds him

### Requirement: Hardening check
The project SHALL provide one command that checks the server over SSH and reports
each failure by name: service user, sandbox settings, file modes, listening
ports, nftables rules, SSH settings and unattended upgrades.

#### Scenario: Root login re-enabled
- **WHEN** `PermitRootLogin` on the server is changed back to allow keys
- **THEN** the check fails and names the SSH root-login setting

### Requirement: Backup to the Mac
The project SHALL provide one command that copies a consistent snapshot of the
server's database and keys to a folder on the Mac with owner-only permissions, never
into the repository.

#### Scenario: Backup while running
- **WHEN** the owner runs the backup while Alice is chatting
- **THEN** the Mac gets a snapshot that opens as a valid database, and the service
  keeps running
