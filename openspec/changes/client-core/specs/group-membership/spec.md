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
- **THEN** the Pi accepts one, the other gets `STALE_EPOCH`, catches up and
  retries its rotation in the new epoch
