## MODIFIED Requirements

### Requirement: Send status
The core SHALL report each sent message as sending until the server accepts it, then
as accepted. On `STALE_EPOCH` or a reconnect it SHALL process pending commits,
re-encrypt and resend with the same `msg_id`; on any other rejection it SHALL report
the message as failed with the reason. A message held behind the group's epoch
SHALL NOT remain silently sending once the device is known to be stuck out of epoch
sync (group-membership: Orphaned device self-recovery): the core SHALL then surface
the message's chat as out of sync (app-ui: Out-of-sync re-sync affordance), keeping
the message and its `msg_id` so it is sent once the device re-syncs — a distinct
recoverable state, not reported as failed (its text is not lost) and not left an
indefinite silent hourglass.

#### Scenario: Message sent across a commit
- **WHEN** Alice's message is rejected with `STALE_EPOCH` because the admin just
  added Carol
- **THEN** the core applies the commit, resends the message in the new epoch, and
  Alice sees it go from sending to accepted

#### Scenario: Message typed while removed for inactivity
- **WHEN** Bob sends "back" while his laptop is removed for inactivity
- **THEN** it stays sending, and after the re-add the core sends it in the new
  epoch with the same `msg_id` and Bob sees it go to accepted

#### Scenario: Held message never silently stuck forever
- **WHEN** Bob's phone is wedged out of epoch sync and cannot be re-added (no
  re-adder online, or the re-add daily limit reached), with "hi" still held
- **THEN** the core surfaces the chat as out of sync so the app shows a re-sync
  action, "hi" is kept with its `msg_id` for sending after re-sync, and it is not
  left an indefinite silent sending nor reported as failed
