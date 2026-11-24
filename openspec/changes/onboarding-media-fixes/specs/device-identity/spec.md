## ADDED Requirements

### Requirement: At most one pending invite per username
`Admin.CreateInvite` SHALL NOT create a second unused invite for a username that
already has an unused, unrevoked, unexpired invite. When one already exists, the
server SHALL return that existing invite and its payload instead of minting a
duplicate, so an admin who asks twice gets the same invite, not two. The server
enforces this. A bootstrap invite, a used invite, a revoked invite, and an expired
invite do not count as a pending invite for this purpose.

#### Scenario: Admin asks twice for the same username
- **WHEN** an admin device calls `Admin.CreateInvite` for `bob`, then calls it
  again for `bob` while the first invite is still unused
- **THEN** the second call returns the same invite and payload as the first, and
  the server holds one unused invite for `bob`, not two

#### Scenario: A fresh invite after the first is used
- **WHEN** `bob`'s first invite has been used (he registered) and the admin calls
  `Admin.CreateInvite` for `bob` again
- **THEN** the server behaves as before (it may refuse because the user now exists,
  or mint a new invite if the user was removed); the pending-invite guard does not
  apply because the first invite is no longer pending
