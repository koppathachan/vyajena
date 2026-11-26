## MODIFIED Requirements

### Requirement: Automatic re-add
When an inactive device of a non-revoked user reconnects, the server SHALL tell it
that it was removed for inactivity and since when, and SHALL send a rejoin notice
for it to the user's other connected devices and to the admin's connected
devices. This "removed for inactivity" notice SHALL be durable: a device that was
offline at the moment it was removed SHALL still be told, on its next connect, that
it was removed and since when — the notice is not lost just because the device was
not connected when the removal happened. The first of the re-adding devices to
receive the rejoin notice SHALL, without user action, verify the device's
certificate against the user's pinned identity key, fetch a KeyPackage for that
device only, and commit its Add together with any pending Remove of its old leaf,
with the Welcome in the same `COMMIT` envelope. The returning device SHALL then
continue from the Welcome; messages from while it was inactive SHALL NOT be
delivered to it. The server enforces the durable notice; the re-adding device
enforces the certificate check.

#### Scenario: Back from holiday
- **WHEN** Bob's only phone reconnects after 10 days and the admin's phone is
  online
- **THEN** the admin's app re-adds Bob's phone with no one tapping anything, and
  Bob's app shows that messages from the last 10 days were not kept

#### Scenario: Two devices race to re-add
- **WHEN** Bob's tablet and the admin's phone both commit a re-add in the same
  epoch
- **THEN** the server accepts one, rejects the other with `STALE_EPOCH` together
  with its Welcome, Bob's phone receives only the winner's Welcome, and the
  losing device sees the phone is already a member and does nothing more

#### Scenario: Nobody able to re-add is online
- **WHEN** a returning device has no other device of its user and no admin device
  online
- **THEN** it stays out, shows that it is waiting to be re-added, and is re-added
  when one of those devices next connects

#### Scenario: Removed while offline, told on next connect
- **WHEN** Bob's phone was offline when the server removed it for inactivity, and it
  later connects
- **THEN** the server delivers the "removed for inactivity" notice (with the
  since-when) on that connect, so Bob's app shows it instead of leaving him with no
  indication that he is out of the group

## ADDED Requirements

### Requirement: Orphaned device self-recovery
The core SHALL recover a device that still holds a group locally but has fallen
behind that group's current epoch with no way to advance, rather than let it wait
indefinitely. The wedge is: the device has caught up on its `Connect` stream (acked
the server's latest `server_seq`) yet an own message it is sending is still rejected
`STALE_EPOCH` because the epoch-advancing commit will never reach it (it is no longer
a current member). On detecting it, the core SHALL request re-add
(`Keys.RequestReadd`), bounded by the per-device daily limit, so a device wedged out
of epoch sync recovers without the user reinstalling. The core SHALL make this
request even though it still holds group state (that is, it is not gated on holding
no group). If re-add cannot complete — no device able to re-add is online, or the
daily limit is reached — the core SHALL signal the chat as out of sync (app-ui:
Out-of-sync re-sync affordance) rather than leave the send a silent indefinite
hourglass. The client enforces this.

#### Scenario: Wedged lobby device asks for itself
- **WHEN** Bob's phone was removed for inactivity, still holds the lobby locally,
  reconnects, catches up on its stream, and its send of "hi" is rejected
  `STALE_EPOCH` with nothing left to apply
- **THEN** the core requests re-add instead of silently holding "hi", and when a
  device able to re-add him is online he is re-added and "hi" goes to accepted with
  the same `msg_id`

#### Scenario: Restart does not dead-end a stale group
- **WHEN** Bob force-quits and reopens the app while wedged (still holding the stale
  lobby)
- **THEN** the core requests re-add on reconnect, where before it stayed silent
  because it still held a group

#### Scenario: Cannot recover, so it surfaces re-sync
- **WHEN** no device able to re-add Bob is online, or his re-add requests reach the
  daily limit
- **THEN** the core signals the chat as out of sync so the app can show a re-sync
  action, and Bob's send is not left a silent indefinite hourglass

### Requirement: Admin is told when a member goes inactive
The server SHALL notify the admin's connected devices when it marks a member's
device inactive and removes it for inactivity, naming the user and since when, so
the admin learns a member fell out of the group rather than finding out only when
that member complains. This is the admin-facing counterpart to the device's own
durable removal notice (Automatic re-add); the existing rejoin notice already tells
the admin when such a device returns and needs re-adding. The admin's app SHALL
surface the inactivity removal (and the pending re-add on return). The server
enforces the notification; the app surfaces it.

#### Scenario: Admin learns a user went inactive
- **WHEN** the server marks `thalasserykalaapam`'s only device inactive after 7 days
  and removes it from the lobby, and the admin's device is connected
- **THEN** the admin's app shows that `thalasserykalaapam` was removed automatically
  (inactive or account deleted; the app cannot tell the two apart, as both are the
  server's external Remove), where today the admin got no signal at all

#### Scenario: Admin learns they returned and were re-added
- **WHEN** `thalasserykalaapam` later reconnects and the admin's device re-adds them
- **THEN** the admin's app shows that they returned and are back in the group
