## ADDED Requirements

### Requirement: Recovery phrase wraps a full-entropy identity key
The core SHALL store the user identity key as an encrypted blob wrapped by a symmetric
key derived from a user-chosen recovery phrase with Argon2id, and SHALL NEVER derive
the identity key from the phrase itself. Recovery unwraps the stored blob to the
original full-entropy random key; the phrase is a recovery factor over a random key,
not the key's seed. The core enforces this.

#### Scenario: Phrase unwraps, never seeds
- **WHEN** the admin sets a recovery phrase and later recovers on a fresh install by
  entering it
- **THEN** the core derives a wrapping key with Argon2id, unwraps the stored blob, and
  obtains the same full-entropy identity key, which matches the key peers already
  pinned

#### Scenario: Phrase is never the key material
- **WHEN** the identity key is generated or recovered
- **THEN** the key is full-entropy random and the phrase only wraps it, so the key's
  strength does not depend on the phrase's guessability

### Requirement: Recovery phrase strength and attempt limits
The core SHALL require a chosen recovery phrase of at least 24 words and SHALL reject a
phrase whose estimated entropy is too low, because word count alone is not security. It
SHALL rate-limit recovery attempts with backoff and lock out after repeated failures,
because unwrapping the blob is offline-guessable. The core enforces this.

#### Scenario: Weak phrase rejected
- **WHEN** a user enters a 24-word phrase that is 24 repetitions of one word
- **THEN** the core rejects it as too weak despite meeting the word count

#### Scenario: Repeated recovery failures locked out
- **WHEN** wrong phrases are entered repeatedly on a recovering device
- **THEN** the core backs off and locks further attempts for a cooldown

### Requirement: RecoverDevice admits a device on the identity-key signature
The server SHALL provide a `RecoverDevice` RPC that admits a device certificate signed
directly by the user's pinned identity key, without an existing device of the user
approving it (it is `LinkDevice` minus the approve step; the identity-key signature is
the authority). Because the recovered identity key is unchanged, peers SHALL NOT be
prompted to re-trust. The server enforces that the certificate verifies against the
pinned identity key.

#### Scenario: Recover with no existing device
- **WHEN** a fresh install unwraps the identity key from the phrase and calls
  `RecoverDevice` with a certificate signed by that key
- **THEN** the server admits the device without any existing-device approval, and no
  peer sees a safety-number change

#### Scenario: Forged recovery certificate
- **WHEN** a `RecoverDevice` certificate does not verify against the user's pinned
  identity key
- **THEN** the server rejects it with `PERMISSION_DENIED`

#### Scenario: Identity recovered, history is not
- **WHEN** a device is recovered via `RecoverDevice`
- **THEN** its new leaf must be re-added to the lobby and groups by a fresh
  Add/Welcome, and prior on-device encrypted history is not recovered

### Requirement: Quorum re-trust by signed member vouches
Each confirming member's vouch for a re-keyed admin (admin-recovery) SHALL be an
attestation cryptographically signed by that member's identity key over the admin's new
key, and a receiving device SHALL verify each vouch against the voucher's pinned
identity key so a hostile server cannot forge or inflate the tally.
The quorum threshold SHALL be a floor or fraction with sane small-group behaviour and
SHALL treat offline members as not-yet-vouched rather than blocking. The client
enforces this.

#### Scenario: Signed vouches cannot be forged by the server
- **WHEN** the server reports that 3 of 5 members vouched for the admin's new key
- **THEN** the receiving client verifies each vouch signature against that member's
  pinned key and discards any vouch it cannot verify

#### Scenario: Small group threshold
- **WHEN** the quorum rule is applied in a 2-person group
- **THEN** the required count is clamped so it cannot exceed the eligible members and
  the admin is excluded from their own tally

### Requirement: Quorum tally is advisory and consented
A client SHALL present a vouch tally as advisory, showing both the count and WHO
vouched ("3 of 5 vouched — accept?"), and SHALL require the device owner's explicit
acknowledgement before trusting the key. It SHALL NEVER silently push a key onto other
members or auto-accept it on the tally's strength. The client enforces this.

#### Scenario: Owner still taps
- **WHEN** a quorum of signed vouches is reached for the admin's new key
- **THEN** the member's device shows the count and the voucher names and still waits
  for the owner to accept before treating the admin as trusted

#### Scenario: No auto-propagation
- **WHEN** one member accepts the admin's new key
- **THEN** no other member's device is auto-trusted; each decides for itself

### Requirement: Verified badges are signed attestations
The admin SHALL have an intrinsic verified badge established by the bootstrap/invite
chain, with no attestation minted for it, and members SHALL have no badge by default. A
member MAY request a badge, and the admin SHALL grant it only by signing an attestation
binding that member's user id to their identity key; a badge is a valid, unrevoked
admin signature over that binding, and all trust surfaced as a badge is a signed
attestation, never the server's assertion. The client enforces verification of the
signature.

#### Scenario: Admin badge is intrinsic
- **WHEN** a member's client displays the admin's identity
- **THEN** it shows the admin as verified from the bootstrap/invite chain without any
  minted attestation

#### Scenario: Member badge requires an admin signature
- **WHEN** a member claims a badge
- **THEN** the client shows it as verified only if there is a valid, unrevoked admin
  signature binding that member's id to their current identity key
