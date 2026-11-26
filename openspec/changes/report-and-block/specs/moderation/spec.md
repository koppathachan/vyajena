## ADDED Requirements

### Requirement: Block a user
A user SHALL be able to block another user by their `UserId`, and to unblock them,
recording the id in a private *blocked-users set* that the core keeps in its local
SQLite store. Blocking SHALL be purely local to the device: it SHALL NOT send any
RPC, SHALL NOT write any server-side marker, and SHALL NOT travel to the user's
other devices or to the blocked user (MVP; a synced marker is deferred, design D4).
Blocking SHALL NOT change group membership, MLS state or crypto in any way: the
blocked user stays in every shared group and DM. The client enforces this.

#### Scenario: Block from a device
- **WHEN** Alice blocks `mallory@chat` on her phone
- **THEN** her core's `block_user` adds `mallory@chat` to the local blocked-users
  set, sends no RPC, and Mallory stays a member of every group she shares with Alice

#### Scenario: Unblock
- **WHEN** Alice unblocks `mallory@chat`
- **THEN** her core's `unblock_user` removes the id from the local set and Mallory's
  content is shown again (A blocked user is hidden)

#### Scenario: Blocking is device-local
- **WHEN** Alice blocks `mallory@chat` on her phone
- **THEN** her tablet's blocked-users set is unchanged, because the set is never
  synced (design D4)

### Requirement: A blocked user is hidden
The core SHALL hide a blocked user's content on the device that blocked them: it
SHALL filter the blocked user's messages out of the displayed history of every
group and the lobby, SHALL hide any DM whose peer is blocked from the chat list,
and SHALL raise no in-app notification for a blocked user's messages. Hiding SHALL
NOT drop or alter the messages: they still arrive, are still accepted and stored
(invariant: accepted means durable), and reappear when the user is unblocked. The
client enforces this; it reads the sender from the message envelope's authenticated
`UserId`, never from message text.

#### Scenario: Blocked sender in a group
- **WHEN** Mallory, whom Alice has blocked, sends a message to a group they share
- **THEN** Alice's app does not show the message and raises no notification, while
  Bob (who has not blocked Mallory) sees it normally

#### Scenario: Blocked DM peer
- **WHEN** Alice has an open DM with Mallory and blocks her
- **THEN** the DM disappears from Alice's chat list and its messages raise no
  notification

#### Scenario: Unblock reveals hidden content
- **WHEN** Alice unblocks Mallory
- **THEN** Mallory's previously hidden group messages and the hidden DM appear
  again from the stored history

#### Scenario: Wake push still fires (MVP limit)
- **WHEN** Mallory, whom Alice has blocked, sends a message while Alice's phone is
  offline
- **THEN** the server, which does not know Alice's local blocks, may still send the
  content-free wake push (message-delivery: Content-free wake pings), but on waking
  the app shows nothing for Mallory and raises no in-app notification (design D4)

### Requirement: Report a message
A user SHALL be able to report a specific message to the admin, and the core SHALL
deliver the report to the admin over a DM (direct-messages: Opening a DM), reusing
the admin's `UserId` that every member already knows. For a message in a group or
the lobby — where the admin is already a member and holds the message — the report
SHALL carry a reference only: the reported user's `UserId`, the group id, the
offending `msg_id`, and a reason. The report SHALL be delivered as a `Content.Text`
whose body the admin's app recognises as a report; no new wire message is added
(design D3). The client composes and sends it.

#### Scenario: Report a group message
- **WHEN** Alice reports Mallory's message in the "Work" group
- **THEN** Alice's core opens (or reuses) a DM with the admin and sends a report
  `Content.Text` naming `mallory@chat`, the "Work" group id, that message's `msg_id`
  and the reason, and the admin's app shows it as a report pointing at the original
  message it already holds

#### Scenario: Report needs no new RPC
- **WHEN** a report is sent
- **THEN** it travels on the existing DM send path and the existing `messaging`
  stream, with no new proto message and no server change

### Requirement: Report a DM message
A user SHALL be able to report a message from a DM, and because the admin is not a
member of that DM, the report SHALL quote the offending message content to the admin
rather than reference it. The core SHALL include the reported user's `UserId`, a
reason, and a quote of the offending message's text, folded and capped by the
existing reply-quote rules (message-content: Replies — whitespace folded to single
spaces, at most `MAX_REPLY_QUOTE_CHARS` Unicode scalar values, a trailing "…" when
cut). The quote SHALL come from the reporter's own decrypted copy; no key or
ciphertext of the DM leaves the reporter's device. The report SHALL be delivered to
the admin as a DM `Content.Text`, as in Report a message. The client enforces this.

#### Scenario: Report a DM message
- **WHEN** Alice reports a message Mallory sent her in their DM
- **THEN** Alice's core sends the admin a report `Content.Text` naming `mallory@chat`,
  the reason, and the capped quote of Mallory's message text, and nothing from the
  DM's keys or ciphertext is sent

#### Scenario: Long offending message is cut
- **WHEN** the reported DM message is longer than `MAX_REPLY_QUOTE_CHARS`
- **THEN** the quote in the report is cut to the limit with a trailing "…", exactly
  as a reply quote would be

### Requirement: Report a user
A user SHALL be able to report a whole user (not one message) to the admin, and the
core SHALL deliver it to the admin over a DM as a report `Content.Text` carrying the
reported user's `UserId` and a reason, with no message reference or quote. The client
composes and sends it.

#### Scenario: Report a user from their profile
- **WHEN** Alice reports `mallory@chat` from Mallory's profile
- **THEN** Alice's core sends the admin a report naming `mallory@chat` and the
  reason, and the admin's app shows it as a user report

### Requirement: A report reaches the admin
The report SHALL be delivered to the admin through the ordinary encrypted DM path,
so it is end-to-end encrypted and the server and relay see only ciphertext
(invariant: the relay and the server see ciphertext only). The admin's app SHALL
surface an incoming report distinctly from an ordinary DM message so the admin
notices it. Delivery SHALL follow the same durability rule as any message: a report
queued for the admin's offline device is delivered on its next connect (invariant:
accepted means durable). The client enforces presentation; the server routes the DM
as it routes any DM.

#### Scenario: Admin offline when reported
- **WHEN** Alice reports Mallory while the admin's phone is offline
- **THEN** the report waits in the admin's queue and is delivered as a DM on the
  admin's next connect, shown as a report

#### Scenario: Server sees only ciphertext
- **WHEN** a report DM passes through the relay and the server
- **THEN** neither can read the reported user, reason or quote, because the report
  rides inside the DM's MLS ciphertext

### Requirement: The admin acts on a report with the existing removal
Acting on a report SHALL reuse the existing admin capability and add no new
enforcement path: after reviewing a report, the admin MAY remove the offending user
by committing Removes of all that user's devices with `Admin.RemoveUser`
(group-membership: Removing a user). This change SHALL NOT add any new RPC, commit
authority or server rule for enforcement. Blocking and reporting SHALL remain
independent of removal: a user may block without reporting, and a report does not by
itself remove anyone. The admin's device enforces removal under the existing commit
authority.

#### Scenario: Admin removes a reported user
- **WHEN** the admin reviews Alice's report about Mallory and decides to act
- **THEN** the admin removes Mallory through the existing `Admin.RemoveUser` flow,
  and no new moderation RPC is involved

#### Scenario: A report does not auto-remove
- **WHEN** a report about Mallory reaches the admin
- **THEN** nothing is removed until the admin acts; the report only informs the
  admin

### Requirement: Published moderation policy
The change SHALL ship a short written moderation policy, published in the app and on
both store listings, stating that reports are reviewed and acted on — the offending
content removed and the user ejected where warranted — within 24 hours, and giving a
contact email for safety reports. The policy text SHALL be recorded in this change's
design.md so it ships with the change and can be copied to the listings. This is a
documentation requirement; no runtime component enforces the 24-hour commitment.

#### Scenario: Policy is reachable in the app
- **WHEN** Alice opens the Profile tab's "Safety & moderation" entry
- **THEN** she sees the policy: how to report, that the admin reviews and acts within
  24 hours, and the safety contact email

#### Scenario: Policy is on the store listings
- **WHEN** the App Store and Play listings are prepared for the public release
- **THEN** the same policy text and contact email are included, satisfying the
  stores' "act on reports" requirement
