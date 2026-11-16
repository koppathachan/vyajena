## MODIFIED Requirements

### Requirement: Sandboxed service
The server SHALL run as the `vyajena` system user, which has no login shell,
under systemd with `NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome`,
`PrivateTmp`, no capabilities, and write access only to `/var/lib/vyajena` and
its own private `/tmp`; it SHALL reach no address but loopback. Its
database and private keys SHALL have mode 0600. The tunnel SHALL run as
`vyajena-tunnel.service` under a dynamic user with the same sandbox, no writable
path, and no address but loopback and the relay. The egress proxy SHALL run as
`vyajena-egress.service` under a dynamic user with the same sandbox, no
writable path, listening on `127.0.0.1:8445` only, and reaching no address
but loopback, the home router's DNS and public addresses (no private,
link-local or carrier-grade NAT range). systemd SHALL restart any of them if
it crashes. The Pi enforces this.

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

#### Scenario: Server dials Google directly
- **WHEN** `vyajena.service` tries to connect to `fcm.googleapis.com` without
  the proxy
- **THEN** systemd blocks the connection; pings reach FCM only through
  `vyajena-egress.service`

### Requirement: Host hardening
The Pi SHALL allow SSH only with keys and SHALL refuse root logins. nftables SHALL
drop all inbound traffic except replies and SSH from the home network's subnet;
the servers' ports SHALL NOT be reachable from any network. The Pi SHALL install
security updates unattended and SHALL listen on nothing but SSH, the two
servers (the real one and the review server) and the egress proxy on loopback.

#### Scenario: Root login
- **WHEN** someone tries `ssh root@192.168.1.8` with a valid key
- **THEN** the Pi refuses the login

#### Scenario: Server port from the home network
- **WHEN** a phone on the home Wi-Fi connects to `192.168.1.8:8443`
- **THEN** nothing answers: the server listens on loopback only and nftables
  drops the packet

### Requirement: Hardening check
The project SHALL provide one command that checks the Pi over SSH and reports
each failure by name: service users, sandbox settings of the server, the review
server, the tunnel and the egress proxy, file modes (including the push
credentials), listening ports, nftables rules, SSH settings, unattended
upgrades, the tunnel attached to the relay, the server ports closed on the LAN,
that the proxy refuses a host outside its list, and that the relay's ports 443
and 8443 present the same TLS keys as the Pi's loopback ports 8443 and 8444.
It SHALL also warn, without failing, when a reboot is pending:
`/run/reboot-required` exists, or a kernel of the running kernel's flavour newer
than the running one is installed; and when a push platform has no credentials
on the Pi.

#### Scenario: Root login re-enabled
- **WHEN** `PermitRootLogin` on the Pi is changed back to allow keys
- **THEN** the check fails and names the SSH root-login setting

#### Scenario: Kernel updated, not rebooted
- **WHEN** unattended upgrades installed kernel `6.18.61+rpt-rpi-v8` and the Pi
  still runs `6.18.60+rpt-rpi-v8`
- **THEN** the check warns `kernel-reboot` naming both versions and still passes

#### Scenario: Tunnel down
- **WHEN** `vyajena-tunnel.service` is stopped
- **THEN** the check fails and names the tunnel

#### Scenario: Service account key world-readable
- **WHEN** `/var/lib/vyajena/push/fcm-service-account.json` has mode 0644
- **THEN** the check fails and names the file

## ADDED Requirements

### Requirement: Push credentials
The push credentials (the FCM service account key and the APNs `.p8` key) SHALL
live on the Mac only in the owner-only folder `~/.vyajena-push/` and on the Pi
only in `/var/lib/vyajena/push/` (mode 0700, files 0600, owned by `vyajena`);
they SHALL never be in the repository, a log or the review server's folder.
The install command SHALL copy them on every deploy without printing them,
refuse a Mac folder or file readable by group or others, and leave the review
server without push. The deploy script enforces this.

#### Scenario: Owner adds the APNs key later
- **WHEN** the owner drops `AuthKey_ABC123.p8` into `~/.vyajena-push/` and runs
  `deploy/pi/deploy.sh`
- **THEN** the Pi gets `/var/lib/vyajena/push/AuthKey_ABC123.p8` with mode 0600
  and starts sending APNs pings; the review server still only logs pings

#### Scenario: Folder open to others
- **WHEN** `~/.vyajena-push/` has mode 0755
- **THEN** the deploy stops before copying anything and names the folder
