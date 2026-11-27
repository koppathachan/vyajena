# direct-messages Specification

## Purpose
TBD - created by archiving change dms. Update Purpose after archive.
## Requirements
### Requirement: Opening a DM
A member device SHALL open a DM with another user by creating an MLS group whose
only member is itself, whose group context lists the server as external sender and
carries the DM extension (`0xF0A2`) naming its own user and the peer, and by
calling `Dms.CreateDm` with the peer and the epoch-0 public state. The server SHALL
accept it only when the caller's device is a member of the main group and
is DM-capable (Old apps never join a DM), the peer is a non-revoked user other than the
caller's user, the extension names exactly those two users, and no open DM
exists for that pair; otherwise it SHALL reject it (`INVALID_ARGUMENT`,
`NOT_FOUND` or `FAILED_PRECONDITION`). When an open DM exists for the pair, the
Server SHALL record nothing and answer `created = false` with that DM's group id.
Any member MAY open a DM with any member; no approval is needed.

#### Scenario: Alice opens a DM with Bob
- **WHEN** Alice's phone calls `Dms.CreateDm` with peer `bob@chat` and valid
  epoch-0 state naming `alice@chat` and `bob@chat`
- **THEN** the server records the DM for the pair at epoch 0 with Alice's phone as
  its only member

#### Scenario: Both open at once
- **WHEN** Bob's phone calls `Dms.CreateDm` for the same pair after Alice's call
  was accepted
- **THEN** the server answers `created = false` with the open DM's group id, Bob's
  core drops its own epoch-0 group, and Bob's phone joins Alice's DM from its
  Welcome

#### Scenario: DM with yourself
- **WHEN** Alice's phone calls `Dms.CreateDm` with peer `alice@chat`
- **THEN** the server rejects it with `INVALID_ARGUMENT`

#### Scenario: Extension names someone else
- **WHEN** Alice's phone calls `Dms.CreateDm` with peer `bob@chat` and an
  extension naming `alice@chat` and `carol@chat`
- **THEN** the server rejects it with `INVALID_ARGUMENT` and records nothing

### Requirement: DM members
A DM SHALL only ever hold devices of its two users. The server SHALL reject a DM
`COMMIT` that adds a device of any other user, or a device whose leaf
capabilities do not list the DM extension `0xF0A2`, with
`REJECTION_REASON_POLICY_VIOLATION`. Every client
SHALL refuse a DM Welcome or commit with a leaf of a third user, and SHALL refuse
a DM Welcome whose extension does not name its own user.

#### Scenario: Alice tries to add Carol
- **WHEN** Alice's phone commits an Add of Carol's phone in the Alice–Bob DM
- **THEN** the server rejects it with `POLICY_VIOLATION`

#### Scenario: Forged Welcome
- **WHEN** a device of Carol receives a Welcome to a group whose DM extension
  names `alice@chat` and `bob@chat`
- **THEN** Carol's core refuses it and joins nothing

### Requirement: Old apps never join a DM
A version 2 core SHALL list the DM extension `0xF0A2` in the capabilities of
every KeyPackage and leaf it makes, and SHALL upload a fresh set of KeyPackages,
last-resort included, the first time it runs. The server SHALL record for each
KeyPackage whether its capabilities list `0xF0A2`, and SHALL count a device as
DM-capable when its last-resort KeyPackage does. When `FetchKeyPackages` names a
DM in `group_id`, the server SHALL return only KeyPackages that list `0xF0A2`, so a
version 1 device is never added to a DM and nothing of a DM is ever queued for
it.

#### Scenario: Bob's tablet still runs 1.0.0
- **WHEN** Alice's phone opens a DM with Bob, whose phone runs a version 2 app
  and whose tablet still runs 1.0.0
- **THEN** only Bob's phone is added, Bob's tablet keeps its main group and
  sees nothing of the DM

#### Scenario: Bob updates his tablet
- **WHEN** Bob updates his tablet's app and its core uploads KeyPackages that
  list `0xF0A2`
- **THEN** the server sends `JoinNotice` for it in the DM to the DM's connected
  member devices, and the first of them adds it

### Requirement: DM members kept complete
For each DM, the server SHALL send `JoinNotice(group_id)` to the DM's connected
member devices for every non-revoked, active, DM-capable device of either user
that is a member of the main group but not of the DM: after it uploads a DM-capable last-resort KeyPackage, and
each time a member device of that DM connects, until it is a member. The first device to receive it SHALL, without user action, verify the
device's certificate, fetch a KeyPackage for that device only with the DM's
`group_id`, and commit its Add with the Welcome in the same `COMMIT` envelope. A
device that loses the race gets `STALE_EPOCH` and does nothing more.

#### Scenario: Bob links a new phone
- **WHEN** Bob links a new phone with a version 2 app while Alice's phone is
  connected
- **THEN** the new phone is added to the main group as today and to the
  Alice–Bob DM by the first member device of that DM to get the notice

#### Scenario: Peer added at creation
- **WHEN** Alice's phone has created the DM and Bob's phone is offline
- **THEN** Alice's phone adds Bob's phone from a fresh KeyPackage, and Bob's
  phone receives the Welcome when it next connects

### Requirement: Inactivity across groups
When the server marks a device inactive, it SHALL issue an external Remove for it
in every group the device is a member of, delete its queue in every group, and
on its return send `RemovedNotice` and `RejoinNotice` per group: in a DM the
`RejoinNotice` goes to the DM's connected member devices, which re-add it under
the DM commit authority rule. The queue limit of 1,000 entries SHALL count all
groups together.

#### Scenario: Bob back from holiday
- **WHEN** Bob's only phone returns after 10 days and Alice's phone is online
- **THEN** the admin's phone re-adds it to the main group, Alice's phone re-adds
  it to the Alice–Bob DM, and Bob sees the gap in both chats

### Requirement: DM closed on user removal
When `Admin.RemoveUser` removes a user, the server SHALL close every open DM of
that user: record `closed_at`, delete its queued entries and public state,
reject later envelopes for it with `REJECTION_REASON_GROUP_CLOSED`, and send
`DmClosed` to each device of the other user that was a member, on every
connect until that device acknowledges it with `Ack(group_id, 0)`. The client
SHALL mark the chat closed and keep its history read-only. The admin SHALL have
no other power over DMs it is not in.

#### Scenario: Bob is removed
- **WHEN** the admin removes Bob while Alice's phone is offline
- **THEN** on Alice's next connect her phone receives `DmClosed` for the
  Alice–Bob DM, keeps its messages, and can no longer send there

#### Scenario: Bob reset and back
- **WHEN** the admin resets Bob and Bob's new phone joins the main group
- **THEN** Alice can open a new DM with Bob, with a new group id, and the old
  DM stays closed

#### Scenario: Admin is not in Alice and Bob's DM
- **WHEN** Alice and Bob have a DM
- **THEN** no admin device is a member and the server offers the admin no call that
  reads, lists or closes it

### Requirement: DM content stays encrypted
DM text, mentions, replies and profiles SHALL travel only inside MLS
application messages of the DM group. The server SHALL learn only the pair, the
DM's creation time and envelope metadata it already sees for the main group,
and the push ping SHALL stay content-free with the text "New message".

#### Scenario: Push for a DM
- **WHEN** Alice sends Bob "hi" in their DM while Bob's phone is offline
- **THEN** Bob's phone gets one ping carrying only its device id and token, with
  the text "New message"

### Requirement: Several groups in the core
The core SHALL keep each group it is a member of separately, route each
envelope, `GapNotice`, `RemovedNotice` and `JoinNotice` by its `group_id`, keep
sending status, epoch retries, acks and leaf rotation per group, and only
replace a group's state on a Welcome for that same group id.

#### Scenario: A DM Welcome keeps the group
- **WHEN** Bob's phone, a member of the main group, joins the Alice–Bob DM from
  a Welcome
- **THEN** Bob's phone is a member of both, and its main-group history and
  state are unchanged

#### Scenario: Stale epoch in one chat only
- **WHEN** Alice's DM message is rejected with `STALE_EPOCH` while the main
  group's epoch is current
- **THEN** her core catches up and resends the DM message only, and her
  main-group messages are not touched

