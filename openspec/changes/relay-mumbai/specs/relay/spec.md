## MODIFIED Requirements

### Requirement: Relay host
The relay SHALL run on its own Google Cloud e2-micro VM in region `asia-south1`
(Mumbai), the same country as the server and its users, of project `vyajena`, with a
standard persistent boot disk of at most 30 GB, a reserved static external IPv4
address in that region, in its own VPC. Invites SHALL name it by the DNS name
`vyajarelay.purplechaamba.com`, whose only record is an A record for that address.
Nothing else SHALL run on that VM or in that VPC. The relay check enforces this.

#### Scenario: Invites name the relay
- **WHEN** the admin creates an invite for Bob with `Admin.CreateInvite`
- **THEN** the invite payload names `vyajarelay.purplechaamba.com:443`, never an IP
  or a home address, and Bob's core resolves the name and checks only the server's
  pin

#### Scenario: Relay moves region
- **WHEN** the owner moves the relay from `us-east1` to `asia-south1` and points
  the A record at the new address
- **THEN** Bob's existing invite, his stored pin and his device keys keep working,
  and his core connects to the new relay with no re-join

#### Scenario: Relay in the wrong region
- **WHEN** the relay VM runs in `us-east1`
- **THEN** the relay check fails and names the VM region
