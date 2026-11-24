## MODIFIED Requirements

### Requirement: Relay host
The relay SHALL run on its own Google Cloud e2-micro VM in a free-tier US region
(`us-central1`, `us-east1` or `us-west1`) of project `vyajena`, with a
standard persistent boot disk of at most 30 GB, a reserved static external IPv4
address, in its own VPC. Invites SHALL name it by the DNS name
`vyajarelay.purplechaamba.com`, whose only record is an A record for that address.
Only rathole and coturn SHALL run on that VM (coturn provides TURN for audio-calls);
nothing else SHALL run on it or in that VPC. The relay check enforces this.

#### Scenario: Invites name the relay
- **WHEN** the admin creates an invite for Bob with `Admin.CreateInvite`
- **THEN** the invite payload names `vyajarelay.purplechaamba.com:443`, never an IP
  or a home address, and Bob's core resolves the name and checks only the server's
  pin

#### Scenario: Only rathole and coturn run
- **WHEN** the relay check lists the VM's listening services
- **THEN** it finds only rathole and coturn on a public interface, and SSH through IAP

### Requirement: Relay firewall
The relay's VPC firewall SHALL allow inbound TCP 443 and the tunnel port from
anywhere, the coturn TURN UDP listening port `<turn-udp-port>` and the media relay
UDP range `<turn-media-range>` from anywhere, TCP 22 only from Google IAP's range
`<iap-range>`, and nothing else. The real port numbers SHALL live only in
`~/.vyajena-relay/network.conf`, never in the repo, docs or specs. VPC flow logs and
firewall-rule logging SHALL be off, so client addresses are not kept. GCP enforces the
firewall; the relay check verifies it.

#### Scenario: Port scan
- **WHEN** someone probes the relay's address on TCP 22, 80 and <review-port>
- **THEN** no connection opens, while 443, the tunnel port and the coturn TURN ports
  accept

#### Scenario: No real ports in the repo
- **WHEN** the repo, docs and specs are searched for the TURN port numbers
- **THEN** they appear only as placeholders, with the real values in
  `~/.vyajena-relay/network.conf`
