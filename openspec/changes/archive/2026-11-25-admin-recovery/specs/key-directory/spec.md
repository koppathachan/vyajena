## MODIFIED Requirements

### Requirement: Clients verify the identity chain
A client SHALL verify every fetched KeyPackage's device certificate against the
returned user identity key, and SHALL pin each user's identity key on first sight,
learning it from `Keys.GetIdentityKeys` or `Keys.FetchKeyPackages`. If a later
fetch returns a different identity key for a pinned user, the client SHALL refuse
to add that user's devices and SHALL warn the user. A member processing a commit
from the admin that adds a device whose certificate does not verify against the
pinned key SHALL look up the user's identity key; if the server returns a new key that
verifies the certificate, the member SHALL accept the commit and pin the new key. The
same change in a commit from any other member SHALL be rejected. A device joining from
a Welcome whose tree holds a leaf that does not verify against that user's pinned key
SHALL look up the user's identity key the same way, whoever signed the Welcome; if the
server's key verifies the leaf, the device SHALL join and pin the new key.

Whenever a user's pinned identity key changes, the client SHALL mark that user's
identity **unverified / pending re-trust** and SHALL surface an explicit
safety-number-changed prompt naming the user ("⚠️ <user>'s safety number changed —
re-trust?"). The client SHALL NOT present that identity as trusted/verified until the
device owner acknowledges the change, and SHALL NEVER silently auto-accept the changed
key as trusted. Pinning the new key (so the group keeps decrypting) and trusting the
identity as verified are distinct: the client pins to keep working but withholds the
verified state until the owner acknowledges. The client enforces this.

#### Scenario: Identity key changed
- **WHEN** a fetch returns an identity key for `bob@chat` that differs from the one
  pinned on this client
- **THEN** the client does not add Bob's devices and surfaces a warning

#### Scenario: Admin re-adds a reset user
- **WHEN** the admin resets Bob and commits an Add of Bob's new phone, which is
  certified by Bob's new identity key
- **THEN** every other member looks up Bob's key, accepts the commit, pins the new
  key, marks Bob unverified with a re-trust prompt, and no member stalls

#### Scenario: Non-admin adds a device with a changed key
- **WHEN** a commit not from the admin adds a device of Bob whose certificate
  needs a new identity key
- **THEN** members reject the commit

#### Scenario: Back from holiday after a reset
- **WHEN** Alice's laptop, with Bob's old key pinned, is re-added after Bob was
  reset and receives a Welcome whose tree holds Bob's new phone
- **THEN** it looks up Bob's key, joins the group, pins the new key and shows a
  re-trust prompt for Bob

#### Scenario: Recovered admin's key must be acknowledged
- **WHEN** the admin is recovered via an operator reset (admin-recovery) and
  re-registers with a new identity key, and a member's client learns the new key
- **THEN** the member's client pins it, marks the admin unverified, shows "⚠️
  <admin>'s safety number changed — re-trust?", and does not present the admin as
  verified until the owner acknowledges, never auto-accepting silently
