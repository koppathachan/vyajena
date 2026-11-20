# group-membership Specification

## Purpose
TBD - created by archiving change core-wire-protocol. Update Purpose after archive.
## Requirements
### Requirement: Device-level membership
Every MLS group member SHALL be a device, whose credential is an MLS basic
credential containing its serialized device certificate. A user SHALL be a member
of the group when at least one of their devices is.

#### Scenario: Member verifies a new device
- **WHEN** a client processes an Add proposal
- **THEN** it verifies the device certificate in the new leaf against that user's
  identity key (pinned, or newly looked up as key-directory: Clients verify the
  identity chain allows), checks that the certificate's MLS signature key equals
  the leaf's signature key, and rejects the commit if either check fails

#### Scenario: Valid certificate on the wrong key
- **WHEN** a leaf carries a genuine certificate for Alice's phone but a different
  signature key
- **THEN** clients reject the commit

### Requirement: Group creation
The admin's device SHALL create the group by calling `Admin.CreateGroup` with the
group id and the group's public state (GroupInfo and ratchet tree) at epoch 0.
The group context SHALL list the server as an MLS external sender and SHALL name the
admin's user id. The MVP server SHALL hold at most one group.

#### Scenario: Group created
- **WHEN** an admin device creates the group with valid public state that lists
  the server as external sender
- **THEN** the server records the group at epoch 0 with that device as its only member

#### Scenario: Second group
- **WHEN** a device calls `Admin.CreateGroup` while a group already exists
- **THEN** the server rejects it with `ALREADY_EXISTS`

### Requirement: Commit authority
The server SHALL accept a commit from an admin device with any proposals. From any
other device it SHALL accept a commit only if every proposal is one of: an update
of the committer's own leaf, an Add of a device certified for the committer's own
user, a Remove of a device of the committer's own user, or a Remove the server issued
as external sender. Clients SHALL apply the same rule and reject commits that
break it.

#### Scenario: Member links their own device
- **WHEN** Alice's phone commits an Add of Alice's newly linked tablet
- **THEN** the server accepts the commit

#### Scenario: Member adds someone else
- **WHEN** Alice's phone commits an Add of a device certified for Bob
- **THEN** the server rejects the commit with `POLICY_VIOLATION`

#### Scenario: Member removes someone else
- **WHEN** Alice's phone commits a Remove of one of Bob's devices
- **THEN** the server rejects the commit with `POLICY_VIOLATION`

#### Scenario: Admin adds a user
- **WHEN** the admin's device sends one `COMMIT` envelope with Adds for all of
  Carol's devices and the Welcome for them
- **THEN** the server accepts the commit and its Welcome together, and only then
  delivers the Welcome to Carol's devices

### Requirement: server validates handshakes without group secrets
The server SHALL track the group's public state (ratchet tree and group context) from
the public state given at creation and every accepted commit, and SHALL verify
each `COMMIT`'s signature against the committing device's leaf, and that the
committing device is the authenticated sender, without holding any group secret.

#### Scenario: Commit signed by another leaf
- **WHEN** a device sends a commit whose sender leaf is not its own leaf
- **THEN** the server rejects it with `POLICY_VIOLATION`

### Requirement: Removing a user
The admin SHALL remove a user by committing Removes of all that user's devices and
calling `Admin.RemoveUser`, which revokes every device of that user. A revoked
device SHALL receive no further envelopes. `Admin.RemoveUser` SHALL also revoke
every unused invite for that username. After removal the username SHALL be free
for a new invite, so the admin can reset a user who lost their identity
device; the new signup brings a new identity key, which members see as a changed
identity key.

#### Scenario: User removed
- **WHEN** the admin removes Bob
- **THEN** Bob's devices fail authentication and are no longer group members

#### Scenario: Admin resets a user
- **WHEN** the admin removes Bob and then creates an invite for `bob@chat`
- **THEN** the server issues the invite, and Bob's new phone registers as `bob@chat`
  with a new identity key

#### Scenario: Old invite dies with the removal
- **WHEN** an unused invite for `bob@chat` exists and the admin removes Bob
- **THEN** registering with that old invite fails, and only an invite created
  after the removal works

### Requirement: Inactive devices are kept out
The server SHALL mark a member device inactive when it has not connected for 7 days
or when its queue is full, whichever comes first. For an inactive device the server
SHALL stop queueing application messages, SHALL delete its queued entries, and
SHALL issue an MLS external Remove proposal for it, reissuing it in each new
epoch until a commit includes it. The server SHALL accept a commit of it from any
member device, and admin devices SHALL commit pending inactivity proposals when
they next connect.

#### Scenario: Phone left in a drawer
- **WHEN** a member device has not connected for 7 days
- **THEN** the server stops queueing for it, deletes its queue, and sends an external
  Remove proposal for it to the group

#### Scenario: Busy group, quiet phone
- **WHEN** a device's queue reaches 1,000 entries after 3 days offline
- **THEN** the server marks it inactive the same way, without waiting for 7 days

### Requirement: Automatic re-add
When an inactive device of a non-revoked user reconnects, the server SHALL tell it
that it was removed for inactivity and since when, and SHALL send a rejoin notice
for it to the user's other connected devices and to the admin's connected
devices. The first of those devices to receive the notice SHALL, without user
action, verify the device's certificate against the user's pinned identity key,
fetch a KeyPackage for that device only, and commit its Add together with any
pending Remove of its old leaf, with the Welcome in the same `COMMIT` envelope.
The returning device SHALL then continue from the Welcome; messages from while it
was inactive SHALL NOT be delivered to it.

#### Scenario: Back from holiday
- **WHEN** Bob's only phone reconnects after 10 days and the admin's phone is
  online
- **THEN** the admin's app re-adds Bob's phone with no one tapping anything, and
  Bob's app shows that messages from the last 10 days were not kept

#### Scenario: Two devices race to re-add
- **WHEN** Bob's tablet and the admin's phone both commit a re-add in the same
  epoch
- **THEN** the server accepts one, rejects the other with `STALE_EPOCH` together
  with its Welcome, Bob's phone receives only the winner's Welcome, and the
  losing device sees the phone is already a member and does nothing more

#### Scenario: Nobody able to re-add is online
- **WHEN** a returning device has no other device of its user and no admin device
  online
- **THEN** it stays out, shows that it is waiting to be re-added, and is re-added
  when one of those devices next connects

### Requirement: Membership state on the server
The server SHALL store only the group id, the current epoch, the member devices with
their leaf positions, the public ratchet tree and group context, and the commit
log. It SHALL NOT store group names, settings or message content.

#### Scenario: Group renamed
- **WHEN** the admin renames the group
- **THEN** the new name travels only inside encrypted application messages and the
  server's stored state does not change

### Requirement: Own leaf rotation
Each device SHALL commit an update of its own leaf when its leaf is older than 7
days, checking when it connects and at least once a day, with random jitter so
devices do not rotate at the same moment. A rotation that loses an epoch race
SHALL be retried after the device catches up.

#### Scenario: Quiet group
- **WHEN** no commit has touched Alice's leaf for 7 days and her laptop is online
- **THEN** her core commits an update of its own leaf, and the group moves to a
  new epoch

#### Scenario: Two devices rotate at once
- **WHEN** Alice's and Bob's devices both commit a rotation for the same epoch
- **THEN** the server accepts one, the other gets `STALE_EPOCH`, catches up and
  retries its rotation in the new epoch

### Requirement: Automatic add of new users
The server SHALL send a `JoinNotice` to the admin's connected devices for each
non-revoked device that has never been a group member and whose user has no
member device: once that device has uploaded its first KeyPackages after
registering, and again each time an admin device connects, until the device is a
member. The first admin device to receive it
SHALL, without user action, fetch a KeyPackage for that device only, verify its
certificate (pinning the user's identity key on first sight), and commit its Add
with the Welcome in the same `COMMIT` envelope. A changed identity key SHALL
follow key-directory: Clients verify the identity chain: the admin's device
warns and adds the device once the admin accepts the key. An admin device that
loses the race gets `STALE_EPOCH`, sees the device is already a member and does
nothing more. The server decides who gets the notice; the admin's device enforces
the identity checks.

#### Scenario: Bob joins while the admin is online
- **WHEN** Bob registers his phone from the admin's invite and the admin's phone
  is connected
- **THEN** the server sends the admin's phone a `JoinNotice` for Bob's phone, the
  admin's phone adds it with no one tapping anything, and Bob's phone receives
  the Welcome

#### Scenario: Admin offline when Bob joins
- **WHEN** Bob registers while no admin device is connected
- **THEN** Bob's app shows that it is waiting to be added, and the admin's phone
  adds Bob's phone as soon as it next connects

#### Scenario: Reset user joins again
- **WHEN** the admin resets Bob and Bob registers again with a new identity key
- **THEN** the admin's phone shows that Bob's key changed and adds his new phone
  once the admin accepts the key

#### Scenario: Two admin devices race
- **WHEN** the admin's phone and tablet both commit the Add of Bob's phone in the
  same epoch
- **THEN** the server accepts one, rejects the other with `STALE_EPOCH` together with
  its Welcome, and the losing device does nothing more

