## ADDED Requirements

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

## MODIFIED Requirements

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
