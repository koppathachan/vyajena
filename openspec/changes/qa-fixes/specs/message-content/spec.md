## MODIFIED Requirements

### Requirement: Group settings travel encrypted
The group name and settings SHALL be sent as a `GroupSettings` body, and only
messages from admin devices SHALL change them. Clients SHALL ignore
`GroupSettings` from non-admin devices. After merging a commit that adds devices,
each admin device SHALL re-send the current `GroupSettings` if a name is set, so
devices added later learn it.

#### Scenario: Non-admin rename
- **WHEN** a client receives `GroupSettings` from a non-admin device
- **THEN** it ignores the change

#### Scenario: Tablet joins after the rename
- **WHEN** the admin renamed the group to "Home chat" and later Bob's tablet is
  added by a `COMMIT`
- **THEN** the admin's phone re-sends `GroupSettings` "Home chat" and the tablet
  shows "Home chat" as the title

### Requirement: User-owned profiles
A user SHALL be able to set their display name and a short "about" text by
sending a `Profile` body. Clients SHALL apply a `Profile` only to the user whose
device sent it, SHALL show the username alongside the display name, and SHALL
fall back to the username when no display name is set. After merging a commit that
adds devices, every device SHALL re-send its own user's `Profile` if one is set.
When a user's identity key changes, clients SHALL drop that user's stored profile.

#### Scenario: Alice sets a display name
- **WHEN** Alice's phone sends a `Profile` with display name "Ali"
- **THEN** other members show "Ali (alice)" for her messages

#### Scenario: Profile cannot impersonate
- **WHEN** Bob sends a `Profile` with display name "akhil"
- **THEN** clients still show Bob's username next to it, so it reads "akhil (bob)"

#### Scenario: New device learns the names
- **WHEN** Bob set display name "Bobby" and later Bob's tablet is added
- **THEN** Bob's phone re-sends its `Profile`, and the tablet and every other
  member show "Bobby (bob)"

#### Scenario: Reset user starts without a name
- **WHEN** Bob had display name "Bobby", is removed, and registers again with a
  new identity key
- **THEN** members show the key-changed warning and his messages as "bob" until
  the new Bob sets a display name
