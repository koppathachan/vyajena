## ADDED Requirements

### Requirement: Resume a staged own commit across a restart
The core SHALL persist, next to each own membership commit it stages and sends, a
typed record of the job that produced the commit (the job's kind and the fields the
core needs to redo it: the target group, the user, the device, and any chained
follow-up job). The record SHALL round-trip every job that can stage such a commit,
including the follow-up job chained after a group removal.

When the core starts and finds a staged own commit still pending (held locally,
not yet merged), it SHALL resume **that** job — the same group and the same chained
follow-up work — and no other, so the pending commit the server will see is finished
exactly as the crashed run intended.

When the stored record cannot be parsed into a known job, the core SHALL discard the
pending commit — clearing both the staged commit and the stored record locally and
logging it — rather than resume a different job in its place. The core SHALL NOT
reinterpret an unrecognised record as a leaf rotation or any other membership change.

#### Scenario: Rotation resumes as a rotation
- **WHEN** Bob's core stages its own leaf rotation, writes it to the pending-commit
  row, and the process restarts before the server accepts it
- **THEN** on restart the core resends that same rotation and no other commit

#### Scenario: Group removal keeps its follow-up
- **WHEN** the admin removes Carol, whose devices are in a named group, so the core
  stages the named-group removal with the follow-up `Admin.RemoveUser` chained after
  it, and the process restarts after that removal commit is staged but before the
  chain finishes
- **THEN** on restart the core finishes the named-group removal and then runs the
  chained `Admin.RemoveUser`, so Carol is removed from the group and fully removed as
  a user, not left removed from the group only

#### Scenario: Unrecognised stored commit is cleared, not reinterpreted
- **WHEN** the core starts with a pending own commit whose stored record it cannot
  parse into a known job
- **THEN** the core clears the pending commit and the stored record and logs it, and
  does not resume a leaf rotation or any other commit in its place
