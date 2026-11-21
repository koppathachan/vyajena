## MODIFIED Requirements

### Requirement: Group settings travel encrypted
A group name SHALL be sent as a `GroupSettings` body, and only messages from an
admin device SHALL change it. Clients SHALL ignore `GroupSettings` from a
non-admin device. For the system lobby, clients SHALL ignore `GroupSettings.name`
entirely and SHALL always show the fixed name "lobby"; the admin's device SHALL
send no `GroupSettings` for the lobby (multi-group: The system lobby). For a named
group, the name SHALL travel as a `GroupSettings` from an admin device, the same
way the main group's name did, and after merging a commit that adds devices each
admin device SHALL re-send the current `GroupSettings` of that group if a name is
set, so devices added later learn it.

#### Scenario: Non-admin rename
- **WHEN** a client receives `GroupSettings` from a non-admin device
- **THEN** it ignores the change

#### Scenario: Lobby name is fixed
- **WHEN** a client receives a `GroupSettings` with a name for the system lobby
- **THEN** it keeps showing the lobby as "lobby" and shows no rename notice

#### Scenario: Named group renamed
- **WHEN** the admin sends `GroupSettings` "Weekend" to a named group
- **THEN** its members show "Weekend" as the group's title

#### Scenario: Device added to a named group learns its name
- **WHEN** the admin named a group "Weekend" and later Bob's tablet is added to it
  by a `COMMIT`
- **THEN** the admin's phone re-sends that group's `GroupSettings` "Weekend" and
  the tablet shows "Weekend" as the title
