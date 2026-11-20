## MODIFIED Requirements

### Requirement: Relay firewall
The relay's VPC firewall SHALL allow inbound TCP 443, the review port (<review-port>)
and the tunnel port from anywhere, TCP 22 only from Google IAP's range
`<iap-range>`, and nothing else. VPC flow logs and firewall-rule logging
SHALL be off, so client addresses are not kept. GCP enforces the firewall; the
relay check verifies it.

#### Scenario: Port scan
- **WHEN** someone probes the relay's address on TCP 22, 80 and 9443
- **THEN** no connection opens, while 443, <review-port> and the tunnel port accept

### Requirement: Relay hardening check
The project SHALL provide one command that checks the relay through `gcloud` and
SSH and reports each failure by name: VM size, region and disk, service account,
static address, the DNS name's records, firewall rules (including the review
port), flow logs, SSH settings, rathole's sandbox and version, listeners,
unattended upgrades and the volatile journal. It SHALL warn, without failing,
when a reboot is pending or the server's tunnel is not attached.

#### Scenario: Extra firewall rule
- **WHEN** someone adds a rule to the relay's VPC that opens TCP 80
- **THEN** the check fails and names the firewall rule
