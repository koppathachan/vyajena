## ADDED Requirements

### Requirement: Relay host
The relay SHALL run on its own Google Cloud e2-micro VM in a free-tier US region
(`us-central1`, `us-east1` or `us-west1`) of project `vyajena`, with a
standard persistent boot disk of at most 30 GB, a reserved static external IPv4
address, in its own VPC. Invites SHALL name it by the DNS name
`relay.purplechaamba.com`, whose only record is an A record for that address.
Nothing else SHALL run on that VM or in that VPC. The relay check enforces this.

#### Scenario: Invites name the relay
- **WHEN** the admin creates an invite for Bob with `Admin.CreateInvite`
- **THEN** the invite payload names `relay.purplechaamba.com:443`, never an IP
  or a home address, and Bob's core resolves the name and checks only the Pi's
  pin

### Requirement: TLS ends on the Pi
The relay SHALL forward every TCP connection on port 443 byte for byte through
the tunnel to the Pi's loopback port, and SHALL NOT terminate TLS or hold any TLS
key. The client enforces this: it accepts only the Pi's pinned TLS key, so a
relay that answers TLS itself is refused.

#### Scenario: Alice sends through the relay
- **WHEN** Alice's core connects to the relay address from her invite and sends
  "hi Bob" with `Messaging.Connect`
- **THEN** the TLS handshake completes with the Pi's pinned key, and no byte the
  relay forwards contains "hi Bob"

#### Scenario: Relay answers with another key
- **WHEN** the relay forwards Alice's connection to a server whose TLS key is
  not the pinned one
- **THEN** Alice's core refuses the connection and reports a server identity
  error

### Requirement: Tunnel authentication
The Pi SHALL dial out to the relay's tunnel port; nothing SHALL listen at home.
The tunnel SHALL use rathole's Noise transport with the relay's static key pinned
on the Pi, and a service token. The relay enforces the token: it SHALL expose
port 443 only through a tunnel whose client proved the token. The Pi enforces the
pin: it SHALL attach only to a relay that proves the pinned Noise key.

#### Scenario: Someone else attaches
- **WHEN** a host that is not the Pi connects to the tunnel port with a wrong
  token
- **THEN** the relay closes the connection and keeps forwarding 443 to the Pi only

#### Scenario: Tunnel restarts
- **WHEN** the relay VM reboots
- **THEN** the Pi's tunnel service reattaches by itself within a minute

### Requirement: Relay firewall
The relay's VPC firewall SHALL allow inbound TCP 443 and the tunnel port from
anywhere, TCP 22 only from Google IAP's range `35.235.240.0/20`, and nothing else.
VPC flow logs and firewall-rule logging SHALL be off, so client addresses are not
kept. GCP enforces the firewall; the relay check verifies it.

#### Scenario: Port scan
- **WHEN** someone probes the relay's address on TCP 22, 80 and 8443
- **THEN** no connection opens, while 443 and the tunnel port accept

### Requirement: Relay host hardening
The relay SHALL allow SSH only through Google IAP, only with the relay key from
the owner's Mac, and never as root or with a password. It SHALL install security
updates unattended and SHALL never reboot automatically. Only SSH and rathole
SHALL listen on a public interface. rathole SHALL run as a sandboxed systemd
service under a dynamic user, holding only the capability to bind port 443.

#### Scenario: Public SSH
- **WHEN** someone runs `ssh vyajena@<relay-ip>` from the internet with the
  relay key
- **THEN** the connection times out; the same key works through
  `gcloud compute start-iap-tunnel`

### Requirement: Relay keeps no state
The relay VM SHALL hold no data that outlives a restart beyond its software and
its config: no service account or API scopes, rathole with no writable path,
logs kept in memory only, and no client address logged at the configured log
level. The relay can be rebuilt from the repo and `~/.vyajena-relay/` alone.

#### Scenario: Relay rebuilt
- **WHEN** the owner deletes the relay VM and runs the create and deploy commands
  again with the same static address and secrets
- **THEN** devices reconnect with no change on the Pi or the devices

### Requirement: Relay secrets
The rathole token, the relay's Noise key pair and the relay's SSH key SHALL be
generated on the owner's Mac into `~/.vyajena-relay/` (folder 0700, files 0600)
and SHALL never be written into the repository or printed. Deploy scripts SHALL
read them from there and send them to the hosts over SSH only.

#### Scenario: Secrets made once
- **WHEN** the owner runs the secrets command twice
- **THEN** the first run creates the token, the Noise key pair and the SSH key
  with owner-only modes, the second run changes nothing, and neither prints a
  secret

### Requirement: Provision and deploy the relay
The project SHALL provide one command each to create the relay's GCP resources
(idempotently, with a dry run that prints the exact `gcloud` commands), to install
or upgrade rathole and its config on the relay with a rollback to the previous
binary, and to build rathole for the relay. rathole SHALL be a pinned version
built from crates.io sources with its own lock file.

#### Scenario: Dry run
- **WHEN** the owner runs the create command with `--dry-run`
- **THEN** it prints every `gcloud` command it would run and changes nothing

### Requirement: Relay hardening check
The project SHALL provide one command that checks the relay through `gcloud` and
SSH and reports each failure by name: VM size, region and disk, service account,
static address, the DNS name's records, firewall rules, flow logs, SSH settings,
rathole's sandbox and
version, listeners, unattended upgrades and the volatile journal. It SHALL warn,
without failing, when a reboot is pending or the Pi's tunnel is not attached.

#### Scenario: Extra firewall rule
- **WHEN** someone adds a rule to the relay's VPC that opens TCP 80
- **THEN** the check fails and names the firewall rule
