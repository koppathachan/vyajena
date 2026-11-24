## ADDED Requirements

### Requirement: Out-of-sync re-sync affordance
The app SHALL show a clear in-chat banner stating the chat is out of sync, with a
**Re-sync** action that asks to be re-added, when the core signals that a chat is
out of sync — the device is wedged behind the group's epoch and cannot self-recover
(group-membership: Orphaned device self-recovery) — rather than leaving the
composer's send on an indefinite silent hourglass with no explanation. The app SHALL
NOT present this as the message having failed (its text is kept). The app enforces
this.

#### Scenario: Out-of-sync banner with a re-sync action
- **WHEN** the core emits the out-of-sync signal for the lobby because Bob's phone
  is wedged and could not be re-added
- **THEN** the lobby chat shows an "out of sync — re-sync" banner with a Re-sync
  button, and Bob's held message is shown as waiting to re-sync, not as a silent
  hourglass and not as failed

#### Scenario: Re-sync asks to be re-added
- **WHEN** Bob taps Re-sync
- **THEN** the app calls the core to request re-add, and once a device able to
  re-add him is online the banner clears and his held message is sent

### Requirement: Admin recovers a locked-out user
The admin's app SHALL expose, as a first-class and reachable flow, the recovery for
a user whose device is wiped or cannot be re-synced: remove the user
(`Admin.RemoveUser`), then create a fresh invite for the same username
(`Admin.CreateInvite`), which the user pastes to re-register. The app SHALL make
the remove and invite actions reachable to the admin (not only buried), SHALL state
that remove must precede re-invite (the server refuses an invite for a username
whose user still exists and is not removed), and SHALL note what is lost (the old
device's key and local history, and that the user's existing DMs were closed by the
removal). The app enforces reachability; the server enforces the remove-before-invite
rule.

#### Scenario: Admin re-invites a wiped user
- **WHEN** `thalasserykalaapam`'s phone was wiped and the admin opens the recovery
  flow
- **THEN** the admin finds Remove a user, removes `thalasserykalaapam`, then finds
  Invite a user and creates a fresh invite for the same username, and the app shows
  the invite text to share

#### Scenario: Invite before remove is refused with a clear reason
- **WHEN** the admin tries to create an invite for `thalasserykalaapam` before
  removing them
- **THEN** the server refuses it because the user still exists, and the app tells the
  admin to remove the user first
