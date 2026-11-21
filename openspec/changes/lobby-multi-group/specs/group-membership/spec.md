## MODIFIED Requirements

### Requirement: Group creation
The admin's device SHALL create the system lobby by calling `Admin.CreateGroup`
with the group id and the group's public state (GroupInfo and ratchet tree) at
epoch 0, and SHALL create a named group by calling `Admin.CreateNamedGroup`
(multi-group: Named group creation). Each group context SHALL list the server as an
MLS external sender and SHALL name the admin's user id in the `0xF0A1` admin
extension; a named group's context SHALL also carry `0xF0A3`. The server SHALL hold
at most one group of kind `system`; every other group it holds SHALL be a DM
created with `Dms.CreateDm` (direct-messages: Opening a DM) or a named group
created with `Admin.CreateNamedGroup`.

#### Scenario: Lobby created
- **WHEN** an admin device creates the group with valid public state that lists
  the server as external sender
- **THEN** the server records the system group at epoch 0 with that device as its
  only member

#### Scenario: Second system group
- **WHEN** a device calls `Admin.CreateGroup` while a system group already exists
- **THEN** the server rejects it with `ALREADY_EXISTS`

#### Scenario: DMs and named groups do not count as the system group
- **WHEN** Alice and Bob have a DM, a "Weekend" named group exists, and the
  admin's device calls `Admin.CreateGroup` again
- **THEN** the server rejects it with `ALREADY_EXISTS` and still holds exactly one
  system group next to them

### Requirement: Commit authority
In the system lobby, the server SHALL accept a commit from an admin device with
any proposals. From any other device it SHALL accept a commit only if every
proposal is one of: an update of the committer's own leaf, an Add of a device
certified for the committer's own user, a Remove of a device of the committer's
own user, or a Remove the server issued as external sender. In a named group the
server SHALL apply the same admin rule, using that group's admin (named in its
`0xF0A1` extension). In a DM, the server SHALL accept a commit from a member
device only if every proposal is one of: an update of the committer's own leaf,
an Add of a device certified for either of the DM's two users, a Remove of a
device of the committer's own user, or a Remove the server issued as external
sender; no device has more power in a DM. Clients SHALL apply the same rules and
reject commits that break them.

#### Scenario: Member links their own device
- **WHEN** Alice's phone commits an Add of Alice's newly linked tablet
- **THEN** the server accepts the commit

#### Scenario: Member adds someone else
- **WHEN** Alice's phone commits an Add of a device certified for Bob in the
  lobby
- **THEN** the server rejects the commit with `POLICY_VIOLATION`

#### Scenario: Member removes someone else
- **WHEN** Alice's phone commits a Remove of one of Bob's devices
- **THEN** the server rejects the commit with `POLICY_VIOLATION`

#### Scenario: Admin adds a user
- **WHEN** the admin's device sends one `COMMIT` envelope with Adds for all of
  Carol's devices and the Welcome for them
- **THEN** the server accepts the commit and its Welcome together, and only then
  delivers the Welcome to Carol's devices

#### Scenario: Admin moderates a named group
- **WHEN** the admin's device commits an Add of Bob's phone, and later a Remove of
  it, in the "Weekend" named group
- **THEN** the server accepts both commits

#### Scenario: Member cannot moderate a named group
- **WHEN** Bob's phone commits an Add of Carol's phone in the "Weekend" named group
- **THEN** the server rejects the commit with `POLICY_VIOLATION`

#### Scenario: Alice adds Bob's phone to their DM
- **WHEN** Alice's phone commits an Add of Bob's phone in the Alice–Bob DM
- **THEN** the server accepts the commit

#### Scenario: Alice removes Bob from their DM
- **WHEN** Alice's phone commits a Remove of Bob's phone in the Alice–Bob DM
- **THEN** the server rejects the commit with `POLICY_VIOLATION`

### Requirement: Membership state on the server
For each group the server SHALL store only the group id, its kind (`system`,
`named` or `dm`), for a DM the two usernames and when it was closed, for a named
group when it was closed, the current epoch, the member devices with their leaf
positions, the public ratchet tree and group context, and the commit log. For
each KeyPackage it SHALL store whether its capabilities list the DM extension
`0xF0A2` and whether they list the named-group extension `0xF0A3`. It SHALL NOT
store group names, settings or message content.

#### Scenario: Group renamed
- **WHEN** the admin renames a named group
- **THEN** the new name travels only inside encrypted application messages and the
  server's stored state does not change

#### Scenario: What the server knows of a named group
- **WHEN** the members of the "Weekend" named group exchange messages
- **THEN** the server's stored state for it is its kind, the epoch, the members,
  the public tree and the commit log, and no message text or name

### Requirement: Automatic add of new users
The server SHALL send a `JoinNotice` for the system lobby to the admin's connected
devices for each non-revoked device that has never been a lobby member and whose
user has no lobby member device: once that device has uploaded its first
KeyPackages after registering, and again each time an admin device connects, until
the device is a lobby member. The first admin device to receive it SHALL, without
user action, fetch a KeyPackage for that device only, verify its certificate
(pinning the user's identity key on first sight), and commit its Add with the
Welcome in the same `COMMIT` envelope. A changed identity key SHALL follow
key-directory: Clients verify the identity chain: the admin's device warns and
adds the device once the admin accepts the key. An admin device that loses the
race gets `STALE_EPOCH`, sees the device is already a member and does nothing
more. New users SHALL be auto-added to the system lobby only, never to a named
group. The server decides who gets the notice; the admin's device enforces the
identity checks.

#### Scenario: Bob joins while the admin is online
- **WHEN** Bob registers his phone from the admin's invite and the admin's phone
  is connected
- **THEN** the server sends the admin's phone a `JoinNotice` for Bob's phone, the
  admin's phone adds it to the lobby with no one tapping anything, and Bob's phone
  receives the Welcome

#### Scenario: Admin offline when Bob joins
- **WHEN** Bob registers while no admin device is connected
- **THEN** Bob's app shows that it is waiting to be added, and the admin's phone
  adds Bob's phone to the lobby as soon as it next connects

#### Scenario: New user is not in any named group
- **WHEN** Bob is auto-added after registering
- **THEN** Bob is a member of the lobby and of no named group until the admin adds
  him to one

#### Scenario: Two admin devices race
- **WHEN** the admin's phone and tablet both commit the Add of Bob's phone in the
  same epoch
- **THEN** the server accepts one, rejects the other with `STALE_EPOCH` together with
  its Welcome, and the losing device does nothing more

### Requirement: Removing a user
The admin SHALL remove a user by committing Removes of all that user's devices in
every group that user is in — the lobby, every named group and every DM — and
calling `Admin.RemoveUser`, which revokes every device of that user. A revoked
device SHALL receive no further envelopes. `Admin.RemoveUser` SHALL also revoke
every unused invite for that username and close every DM of that user
(direct-messages: DM closed on user removal). After removal the username SHALL be
free for a new invite, so the admin can reset a user who lost their identity
device; the new signup brings a new identity key, which members see as a changed
identity key.

#### Scenario: User removed
- **WHEN** the admin removes Bob
- **THEN** Bob's devices fail authentication and are no longer members of the
  lobby, of any named group or of any DM

#### Scenario: Admin resets a user
- **WHEN** the admin removes Bob and then creates an invite for `bob@chat`
- **THEN** the server issues the invite, and Bob's new phone registers as `bob@chat`
  with a new identity key

#### Scenario: Old invite dies with the removal
- **WHEN** an unused invite for `bob@chat` exists and the admin removes Bob
- **THEN** registering with that old invite fails, and only an invite created
  after the removal works
