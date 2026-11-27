## ADDED Requirements

### Requirement: Deployment details stay out of the repository
The project SHALL NOT hold the real IP addresses, networks, router, MAC address,
SSH target, admin username or non-default ports of a deployment in the repository,
its docs or its specs. The deploy scripts SHALL read them from a settings file
outside the repository, `network.conf` in the owner's secrets folder, made from
`deploy/network.conf.example`, and SHALL stop with the missing key's name when the
file or a key is absent. Templates that name an address or port SHALL hold a
placeholder that deploy fills. Ports 443 and 22 are the only fixed public ports.
The scripts and tests enforce this.

#### Scenario: Settings file missing
- **WHEN** the owner runs `deploy/server/deploy.sh` before copying
  `deploy/network.conf.example` to `~/.vyajena-relay/network.conf`
- **THEN** the script stops before any SSH and says where to copy the example

#### Scenario: Templates fill from the settings
- **WHEN** deploy renders `nftables.conf`, `push.toml`, `vyajena-egress.service` and
  `harden.sh` with the example settings
- **THEN** no placeholder is left and the home network, router, address and ports
  of the settings appear in them

#### Scenario: No deployment value is committed
- **WHEN** the tests scan the tracked files
- **THEN** none holds the deployment's former LAN address, MAC address or SSH
  user, and nothing under `deploy/` names a port other than from the example file
