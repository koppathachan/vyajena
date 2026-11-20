## MODIFIED Requirements

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
