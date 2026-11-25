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

### Requirement: Invite expiry is surfaced
The app SHALL surface an invite's expiry wherever an invite is shown or used: when
the admin creates or displays an invite it SHALL state when the invite expires (an
invite is valid for `invite_ttl`, 7 days by default), and when a user pastes an
invite that the server rejects as expired the app SHALL say the invite has expired
and that a fresh one is needed, rather than showing a generic failure. This keeps the
recovery flow usable — a wiped user who does not paste the invite in time is told
plainly, and the admin can create another. The app enforces the surfacing; the server
enforces the TTL and reports expiry.

#### Scenario: A created invite shows when it expires
- **WHEN** the admin creates a fresh invite for `thalasserykalaapam`
- **THEN** the app shows the invite text together with when it expires (for example
  "expires in 7 days"), not the invite text alone

#### Scenario: Pasting an expired invite is explained
- **WHEN** `thalasserykalaapam` pastes an invite after it has expired and the server
  rejects it
- **THEN** the app says the invite has expired and a new one is needed, rather than a
  generic "could not join" error

### Requirement: Add a member to an existing named group
The admin's app SHALL provide a way to add an existing user to an existing named
group, not only when the group is created, using the existing add-to-group flow
(the lobby and named groups: admin devices commit the Add). Without it, a user who
is removed and re-invited (or any user added after a named group was created) cannot
be restored to that named group, because today the Members screen is view-only and
named-group membership can only be chosen at creation. The lobby and DMs re-add
automatically and are unaffected; this requirement covers named groups only. The app
enforces reachability; the server and clients enforce commit authority as today.

#### Scenario: Re-add a recovered user to a named group
- **WHEN** `thalasserykalaapam` has re-registered and is back in the lobby, and was
  previously in the named group "Weekend"
- **THEN** the admin opens "Weekend" and uses an add-a-member action to add
  `thalasserykalaapam` to it, rather than having to recreate the group

#### Scenario: Add a later user to an existing named group
- **WHEN** the admin created "Weekend" before Dave joined and now wants Dave in it
- **THEN** the admin can add Dave to "Weekend" from the group, with no need to
  recreate it
