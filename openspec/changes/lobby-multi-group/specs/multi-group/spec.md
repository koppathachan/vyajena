## ADDED Requirements

### Requirement: The system lobby
The server SHALL hold exactly one group of kind `system`, the lobby: the group
the admin created at bootstrap with `Admin.CreateGroup`, now recorded as
`system`. Every non-revoked member of the server SHALL be a member of it, kept
there by group-membership: Automatic add of new users. Clients SHALL show its name
as the fixed string "lobby", SHALL ignore any `GroupSettings.name` for it, and
SHALL offer no rename or close for it. The server SHALL reject `Admin.CloseGroup`
on it with `FAILED_PRECONDITION`. The server and the client enforce this.

#### Scenario: Lobby cannot be renamed
- **WHEN** an admin device sends a `GroupSettings` with a name for the system group
- **THEN** every client still shows the group as "lobby" and shows no rename notice

#### Scenario: Lobby cannot be closed
- **WHEN** the admin calls `Admin.CloseGroup` with the system group's `group_id`
- **THEN** the server rejects it with `FAILED_PRECONDITION` and the group is unchanged

#### Scenario: Old name dropped on upgrade
- **WHEN** the group was named "Home chat" before the upgrade
- **THEN** after the upgrade clients show it as "lobby" and never show a rename
  notice for the change

### Requirement: Named group creation
The admin's device SHALL create a named group by calling `Admin.CreateNamedGroup`
with the `group_id` and the group's epoch-0 public state (GroupInfo and ratchet
tree). The group context SHALL list the server as an MLS external sender, SHALL name
the admin in the `0xF0A1` admin extension, and SHALL carry the named-group
extension `0xF0A3`. The server SHALL record the group with kind `named` and that
device as its only member, and SHALL reject the call from a non-admin device with
`PERMISSION_DENIED`. The server enforces this.

#### Scenario: Admin creates a named group
- **WHEN** the admin's device calls `Admin.CreateNamedGroup` with valid epoch-0
  state that lists the server as external sender and carries `0xF0A1` and `0xF0A3`
- **THEN** the server records a `named` group at epoch 0 with that device as its
  only member

#### Scenario: Non-admin cannot create one
- **WHEN** Bob's device calls `Admin.CreateNamedGroup`
- **THEN** the server rejects it with `PERMISSION_DENIED` and records no group

#### Scenario: Named group needs its extension
- **WHEN** an admin device calls `Admin.CreateNamedGroup` with a context that
  carries `0xF0A1` but not `0xF0A3`
- **THEN** the server rejects it with `INVALID_ARGUMENT` and records no group

### Requirement: Named group membership
A named group SHALL hold any subset of the server's members that the admin chooses.
The admin's device SHALL add a member by committing an Add of that member's
devices and remove a member by committing a Remove of them, under
group-membership: Commit authority for a named group. A newly invited member
SHALL NOT be auto-added to any named group; only the system lobby auto-adds them.
A member removed from a named group SHALL remain in the lobby and in their own DMs.
The server and every client enforce the commit rule.

#### Scenario: Admin adds a member
- **WHEN** the admin commits an Add of Bob's phone to the "Weekend" named group
- **THEN** the server accepts the commit and Bob's phone receives the Welcome

#### Scenario: A member cannot add another member
- **WHEN** Bob's phone commits an Add of Carol's phone in the named group
- **THEN** the server rejects it with `POLICY_VIOLATION`

#### Scenario: New server member joins lobby only
- **WHEN** the admin invites Dan and the admin's device auto-adds him
- **THEN** Dan is added to the system lobby and to no named group

#### Scenario: Removed from a group, still in lobby
- **WHEN** the admin commits a Remove of Bob's devices from the "Weekend" group
- **THEN** Bob is no longer in that group and is still a member of the lobby

### Requirement: Closing a named group
The admin SHALL close a named group by calling `Admin.CloseGroup` with its
`group_id`. The server SHALL set its `closed_at`, delete its queued entries and
public state, reject later envelopes for it with `REJECTION_REASON_GROUP_CLOSED`,
and send each of its member devices a `GroupClosed{group_id, closed_at}` on every
connect until that device sends `Ack(group_id, server_seq = 0)` for it. Clients
SHALL mark the chat closed and keep its history read-only. The server enforces
this.

#### Scenario: Admin closes a group
- **WHEN** the admin calls `Admin.CloseGroup` on the "Weekend" group
- **THEN** the server marks it closed and each member device gets a `GroupClosed`
  on its next connect

#### Scenario: Message to a closed group
- **WHEN** Bob's phone sends an envelope to a closed named group
- **THEN** the server rejects it with `REJECTION_REASON_GROUP_CLOSED` and stores
  nothing

#### Scenario: Close notice is durable
- **WHEN** Bob's phone receives `GroupClosed` and acks it with `server_seq` 0
- **THEN** the server stops sending `GroupClosed` for that group to that device

### Requirement: Named groups reach only v3 apps
A named group SHALL reach only devices that run protocol version 3. A version 3
core SHALL list the named-group extension `0xF0A3` in every KeyPackage and leaf.
The server SHALL return only `0xF0A3`-listing KeyPackages for a named group
(key-directory: KeyPackage fetch), SHALL reject a named-group `COMMIT` that adds a
leaf without `0xF0A3` with `REJECTION_REASON_POLICY_VIOLATION`, and SHALL never
queue a named group's envelopes, Welcomes included, for a version 1 or version 2
device. A version 1 or 2 core SHALL refuse a named group's Welcome without
corrupting the lobby or its DMs. The server and the client enforce this.

#### Scenario: Old device is never added
- **WHEN** the admin fetches KeyPackages for the "Weekend" group and Carol still
  runs a version 2 app
- **THEN** the server returns no KeyPackage for Carol's devices and she is never
  added

#### Scenario: Named-group Add of an uncapable leaf
- **WHEN** a commit adds a leaf whose capabilities do not list `0xF0A3` to a named
  group
- **THEN** the server rejects it with `REJECTION_REASON_POLICY_VIOLATION`

#### Scenario: A v2 core refuses a named group's Welcome
- **WHEN** a version 2 core is somehow sent a named group's Welcome
- **THEN** it refuses the Welcome and keeps its lobby and DMs intact
