## ADDED Requirements

### Requirement: Re-key evicts the user's old group leaves and rejoins through the normal path
The admin device SHALL ensure that when a re-invite re-keys a present user
(`device-identity: Re-inviting an existing user`) no orphaned old leaf lingers in any
group, and the reinstalled device SHALL rejoin through the normal add path with no admin
"remove" step first; the admin device enforces both the eviction and the re-add. The
reinstalled device SHALL rejoin the lobby and named groups through `group-membership:
Automatic add of new users` unchanged: after it uploads its first KeyPackages the server
sends the admin a `JoinNotice`, and the admin device commits its Add. In that same
`COMMIT`, the admin device SHALL also Remove every stale leaf of the re-keyed user — the
leaves of the devices the server revoked on the re-key, which the admin recognises
because their certificates no longer verify against the user's new pinned identity key —
with the Welcome in the same `COMMIT`, so no epoch holds both the old and the new leaf.
The admin device SHALL commit these Removes without a separate `Admin.RemoveUser` and
without the admin hand-selecting leaves. No server-issued external Remove is required for
this path.

#### Scenario: Old leaf is evicted and the new device joins
- **WHEN** the admin re-invites `bob@chat`, Bob's reinstalled phone registers and
  re-keys, and the admin device processes the `JoinNotice` for it
- **THEN** the admin device commits, in one `COMMIT`, the Add of Bob's new phone and the
  Remove of Bob's old revoked leaf, with the Welcome in the same `COMMIT`, and the lobby
  ends with exactly Bob's new leaf and no orphan

#### Scenario: No prior remove step
- **WHEN** the admin re-invites a user who is still a present (non-removed) member
- **THEN** the admin does not call `Admin.RemoveUser` first, and does not hand-pick
  leaves; the re-key plus the admin's combined Remove+Add on re-add are what clear the
  stale membership

#### Scenario: Member offline during the re-key catches up
- **WHEN** a lobby member was offline while Bob was re-invited and re-added, and later
  reconnects
- **THEN** it receives the commit that removed Bob's old leaf and added his new one, and
  ends with only Bob's new leaf

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
