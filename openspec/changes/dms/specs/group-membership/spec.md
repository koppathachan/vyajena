## MODIFIED Requirements

### Requirement: Group creation
The admin's device SHALL create the main group by calling `Admin.CreateGroup`
with the group id and the group's public state (GroupInfo and ratchet tree) at
epoch 0. The group context SHALL list the Pi as an MLS external sender and SHALL
name the admin's user id. The Pi SHALL hold at most one main group; every other
group it holds SHALL be a DM created with `Dms.CreateDm` (direct-messages:
Opening a DM).

#### Scenario: Group created
- **WHEN** an admin device creates the group with valid public state that lists
  the Pi as external sender
- **THEN** the Pi records the group at epoch 0 with that device as its only member

#### Scenario: Second group
- **WHEN** a device calls `Admin.CreateGroup` while a main group already exists
- **THEN** the Pi rejects it with `ALREADY_EXISTS`

#### Scenario: DMs do not count as a main group
- **WHEN** Alice and Bob have a DM and the admin's device calls
  `Admin.CreateGroup` on a Pi with no main group
- **THEN** the Pi records the main group

### Requirement: Commit authority
In the main group, the Pi SHALL accept a commit from an admin device with any
proposals. From any other device it SHALL accept a commit only if every proposal
is one of: an update of the committer's own leaf, an Add of a device certified
for the committer's own user, a Remove of a device of the committer's own user,
or a Remove the Pi issued as external sender. In a DM, the Pi SHALL accept a
commit from a member device only if every proposal is one of: an update of the
committer's own leaf, an Add of a device certified for either of the DM's two
users, a Remove of a device of the committer's own user, or a Remove the Pi
issued as external sender; no device has more power in a DM. Clients SHALL apply
the same rules and reject commits that break them.

#### Scenario: Member links their own device
- **WHEN** Alice's phone commits an Add of Alice's newly linked tablet
- **THEN** the Pi accepts the commit

#### Scenario: Member adds someone else
- **WHEN** Alice's phone commits an Add of a device certified for Bob in the
  main group
- **THEN** the Pi rejects the commit with `POLICY_VIOLATION`

#### Scenario: Member removes someone else
- **WHEN** Alice's phone commits a Remove of one of Bob's devices
- **THEN** the Pi rejects the commit with `POLICY_VIOLATION`

#### Scenario: Admin adds a user
- **WHEN** the admin's device sends one `COMMIT` envelope with Adds for all of
  Carol's devices and the Welcome for them
- **THEN** the Pi accepts the commit and its Welcome together, and only then
  delivers the Welcome to Carol's devices

#### Scenario: Alice adds Bob's phone to their DM
- **WHEN** Alice's phone commits an Add of Bob's phone in the Alice–Bob DM
- **THEN** the Pi accepts the commit

#### Scenario: Alice removes Bob from their DM
- **WHEN** Alice's phone commits a Remove of Bob's phone in the Alice–Bob DM
- **THEN** the Pi rejects the commit with `POLICY_VIOLATION`

### Requirement: Membership state on the Pi
For each group the Pi SHALL store only the group id, its kind (main or DM), for
a DM the two usernames and when it was closed, the current epoch, the member
devices with their leaf positions, the public ratchet tree and group context,
and the commit log. For each device it SHALL store the protocol version of its
last `Hello`. It SHALL NOT store group names, settings or message content.

#### Scenario: Group renamed
- **WHEN** the admin renames the group
- **THEN** the new name travels only inside encrypted application messages and the
  Pi's stored state does not change

#### Scenario: What the Pi knows of a DM
- **WHEN** Alice and Bob exchange messages in their DM
- **THEN** the Pi's stored state for it is the pair, the epoch, the members, the
  public tree and the commit log, and no message text
