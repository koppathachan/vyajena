## ADDED Requirements

### Requirement: Device-level membership
Every MLS group member SHALL be a device, whose credential is an MLS basic
credential containing its serialized device certificate. A user SHALL be a member
of the group when at least one of their devices is.

#### Scenario: Member verifies a new device
- **WHEN** a client processes an Add proposal
- **THEN** it verifies the device certificate in the new leaf against that user's
  pinned identity key, checks that the certificate's MLS signature key equals the
  leaf's signature key, and rejects the commit if either check fails

#### Scenario: Valid certificate on the wrong key
- **WHEN** a leaf carries a genuine certificate for Alice's phone but a different
  signature key
- **THEN** clients reject the commit

### Requirement: Group creation
The admin's device SHALL create the group by calling `Admin.CreateGroup` with the
group id and the group's public state (GroupInfo and ratchet tree) at epoch 0.
The group context SHALL list the Pi as an MLS external sender and SHALL name the
admin's user id. The MVP Pi SHALL hold at most one group.

#### Scenario: Group created
- **WHEN** an admin device creates the group with valid public state that lists
  the Pi as external sender
- **THEN** the Pi records the group at epoch 0 with that device as its only member

#### Scenario: Second group
- **WHEN** a device calls `Admin.CreateGroup` while a group already exists
- **THEN** the Pi rejects it with `ALREADY_EXISTS`

### Requirement: Commit authority
The Pi SHALL accept a commit from an admin device with any proposals. From any
other device it SHALL accept a commit only if every proposal is one of: an update
of the committer's own leaf, an Add of a device certified for the committer's own
user, a Remove of a device of the committer's own user, or a Remove the Pi issued
as external sender. Clients SHALL apply the same rule and reject commits that
break it.

#### Scenario: Member links their own device
- **WHEN** Alice's phone commits an Add of Alice's newly linked tablet
- **THEN** the Pi accepts the commit

#### Scenario: Member adds someone else
- **WHEN** Alice's phone commits an Add of a device certified for Bob
- **THEN** the Pi rejects the commit with `POLICY_VIOLATION`

#### Scenario: Member removes someone else
- **WHEN** Alice's phone commits a Remove of one of Bob's devices
- **THEN** the Pi rejects the commit with `POLICY_VIOLATION`

#### Scenario: Admin adds a user
- **WHEN** the admin's device commits Adds for all of Carol's devices with a
  Welcome
- **THEN** the Pi accepts the commit and delivers the Welcome to Carol's devices

### Requirement: Pi validates handshakes without group secrets
The Pi SHALL track the group's public state (ratchet tree and group context) from
the public state given at creation and every accepted commit, and SHALL verify
each `COMMIT`'s signature against the committing device's leaf, and that the
committing device is the authenticated sender, without holding any group secret.

#### Scenario: Commit signed by another leaf
- **WHEN** a device sends a commit whose sender leaf is not its own leaf
- **THEN** the Pi rejects it with `POLICY_VIOLATION`

### Requirement: Removing a user
The admin SHALL remove a user by committing Removes of all that user's devices and
calling `Admin.RemoveUser`, which revokes every device of that user. A revoked
device SHALL receive no further envelopes.

#### Scenario: User removed
- **WHEN** the admin removes Bob
- **THEN** Bob's devices fail authentication and are no longer group members

### Requirement: Inactive devices are kept out
The Pi SHALL mark a member device inactive when it has not connected for 7 days
or when its queue is full, whichever comes first. For an inactive device the Pi
SHALL stop queueing application messages, SHALL delete its queued entries, and
SHALL issue an MLS external Remove proposal for it, reissuing it in each new
epoch until a commit includes it. The Pi SHALL accept a commit of it from any
member device, and admin devices SHALL commit pending inactivity proposals when
they next connect.

#### Scenario: Phone left in a drawer
- **WHEN** a member device has not connected for 7 days
- **THEN** the Pi stops queueing for it, deletes its queue, and sends an external
  Remove proposal for it to the group

#### Scenario: Busy group, quiet phone
- **WHEN** a device's queue reaches 1,000 entries after 3 days offline
- **THEN** the Pi marks it inactive the same way, without waiting for 7 days

### Requirement: Automatic re-add
When an inactive device of a non-revoked user reconnects, the Pi SHALL tell it
that it was removed for inactivity and since when, and SHALL send a rejoin notice
for it to the user's other connected devices and to the admin's connected
devices. The first of those devices to receive the notice SHALL, without user
action, verify the device's certificate against the user's pinned identity key
and commit its Add together with any pending Remove of its old leaf. The
returning device SHALL then continue from the Welcome; messages from while it
was inactive SHALL NOT be delivered to it.

#### Scenario: Back from holiday
- **WHEN** Bob's only phone reconnects after 10 days and the admin's phone is
  online
- **THEN** the admin's app re-adds Bob's phone with no one tapping anything, and
  Bob's app shows that messages from the last 10 days were not kept

#### Scenario: Two devices race to re-add
- **WHEN** Bob's tablet and the admin's phone both commit a re-add in the same
  epoch
- **THEN** the Pi accepts one, rejects the other with `STALE_EPOCH`, and the
  losing device sees the phone is already a member and does nothing more

#### Scenario: Nobody able to re-add is online
- **WHEN** a returning device has no other device of its user and no admin device
  online
- **THEN** it stays out, shows that it is waiting to be re-added, and is re-added
  when one of those devices next connects

### Requirement: Membership state on the Pi
The Pi SHALL store only the group id, the current epoch, the member devices with
their leaf positions, the public ratchet tree and group context, and the commit
log. It SHALL NOT store group names, settings or message content.

#### Scenario: Group renamed
- **WHEN** the admin renames the group
- **THEN** the new name travels only inside encrypted application messages and the
  Pi's stored state does not change
