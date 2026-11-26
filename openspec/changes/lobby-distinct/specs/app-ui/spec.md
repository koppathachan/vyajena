## ADDED Requirements

### Requirement: Lobby row styling
The chat list SHALL render the lobby row (`ChatKindInfo_Lobby`) distinctly from
all other rows: a `#` hash tile on an accent-tinted square in place of the
plain avatar, the lobby name in italics, a small "system" tag pill beside the
name, the row pinned to the top of the list, and a subtle accent-tinted row
background. This treatment SHALL apply only to the lobby, never to an
admin-created named group (`ChatKindInfo_Group`) or a DM (`ChatKindInfo_Dm`),
whose rows keep their plain avatar and upright name. The app enforces this.

#### Scenario: Lobby looks like the system space
- **WHEN** Alice opens the chat list with the lobby, a "Weekend" named group and a
  DM with Bob
- **THEN** the lobby row (`ChatKindInfo_Lobby`) is first, with a `#` hash tile on
  an accent-tinted square, its name in italics, a "system" pill beside it and a
  subtle accent row tint

#### Scenario: Only the lobby is styled
- **WHEN** the same list is shown
- **THEN** the "Weekend" group row and Bob's DM row show no hash tile, no italics,
  no "system" pill and no accent tint

#### Scenario: Lobby stays pinned first
- **WHEN** a DM or named group has the newest message
- **THEN** the lobby row is still first in the list
