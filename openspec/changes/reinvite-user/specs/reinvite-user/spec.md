## ADDED Requirements

### Requirement: Re-key evicts the user's old group leaves and rejoins through the normal path
The server SHALL ensure that when a re-invite re-keys a present user (`device-identity:
Re-inviting an existing user`) no orphaned old leaf lingers in any group, and the
reinstalled device SHALL rejoin through the normal add path with no admin "remove" step
first. The server enforces eviction; the admin device enforces the re-add. The server
SHALL mark the re-keyed user's revoked old-device leaves for removal using the
same server-issued external-Remove mechanism it uses for revoked and inactive devices
(`group-membership: Inactive devices are kept out`), reissued each epoch until a member
commits it. The reinstalled device SHALL rejoin the lobby and named groups through
`group-membership: Automatic add of new users` unchanged: after it uploads its first
KeyPackages the server sends the admin a `JoinNotice`, and the admin device commits its
Add; the admin device SHOULD commit the Add together with the pending Remove of the old
leaf, with the Welcome in the same `COMMIT`, so no epoch holds both leaves.

#### Scenario: Old leaf is evicted and the new device joins
- **WHEN** the admin re-invites `bob@chat`, Bob's new phone registers and re-keys, and
  the admin device next commits in the lobby
- **THEN** the server has issued an external Remove for Bob's old leaf, the admin
  commits that Remove together with the Add of Bob's new phone with the Welcome in the
  same `COMMIT`, and the lobby ends with exactly Bob's new leaf and no orphan

#### Scenario: No prior remove step
- **WHEN** the admin re-invites a user who is still a present (non-removed) member
- **THEN** the admin does not call `Admin.RemoveUser` first; the re-key and the
  server-issued eviction of the old leaves are what clear the stale membership

#### Scenario: Member offline during the re-key catches up
- **WHEN** a lobby member was offline while Bob was re-invited and re-added, and later
  reconnects
- **THEN** it receives the commit(s) that removed Bob's old leaf and added his new one,
  and ends with only Bob's new leaf

### Requirement: Re-invite changes the user's identity key for peers
A re-invite SHALL present to every other member as a changed identity key for the same
user id, and SHALL be handled by the existing re-trust mechanism with nothing
auto-accepted; the client enforces this. Because the re-key keeps the user id and
changes only the identity key, each member's client SHALL follow `key-directory:
Clients verify the identity chain`: it pins the new key to keep decrypting, marks the
user **unverified / pending re-trust**, surfaces "⚠️ `<name>'s` safety number changed —
re-trust?", and SHALL NEVER silently auto-accept the changed key as trusted. As for any
identity-key change, only the **admin**-committed Add of the re-keyed device SHALL be
accepted with the new key; the same change in a non-admin commit SHALL be rejected. This
capability adds no new client behaviour; it binds the re-invite to that existing
requirement.

#### Scenario: Members see a re-trust prompt after a re-invite
- **WHEN** the admin re-invites `bob@chat`, Bob's new phone re-keys and the admin adds
  it to the lobby
- **THEN** every other member pins Bob's new identity key, marks Bob unverified, and
  shows "⚠️ Bob's safety number changed — re-trust?", and no member presents Bob as
  verified until its owner acknowledges

#### Scenario: Non-admin cannot slip in the re-keyed device
- **WHEN** a commit that is not from the admin tries to add Bob's re-keyed device whose
  certificate needs Bob's new identity key
- **THEN** members reject the commit
