## MODIFIED Requirements

### Requirement: Contact tap
Tapping any row on the Contacts tab SHALL do something: another member's row opens
that member's profile (app-shell: Member profile), from which a Message action opens
or starts the DM with them, asking the core afresh whether DMs are offered (the
answer can arrive after the tab first loaded) and, when DMs are not offered, showing
a snack bar that says so; the user's own row opens the Profile tab.

#### Scenario: Another row opens the member's profile
- **WHEN** Alice taps Bob
- **THEN** Bob's profile opens, showing his name, avatar and device count with a
  Message action

#### Scenario: Message when DMs are learnt after the tab loaded
- **WHEN** the Contacts tab loaded before the device connected, and Alice opens
  Bob's profile and taps Message once the server says it offers DMs
- **THEN** the DM with Bob opens

#### Scenario: Message when DMs are not offered
- **WHEN** Alice opens Bob's profile and taps Message while DMs are not offered
- **THEN** a snack bar says so and no DM opens

#### Scenario: Own row
- **WHEN** Alice taps her own row
- **THEN** the Profile tab opens

### Requirement: Open a DM
The app SHALL offer "Message" on another member's profile (app-shell: Member
profile). It SHALL open the existing open DM with that member, or ask the core to
open one (direct-messages: Opening a DM) and show it at once with "Waiting for
Bobby's devices" until the peer's first device is added. When the server lists only
protocol version 1 (DMs are not offered), Message SHALL instead show a notice that
direct messages are not available yet, so the action never does nothing.

#### Scenario: First message to Bob
- **WHEN** Alice taps "Message" on Bob's profile and has no DM with him
- **THEN** the app asks the core to open the DM and shows the empty DM, where
  Alice can type at once; her message is sent once Bob's phone is added

#### Scenario: Old server
- **WHEN** the server lists only protocol version 1 and Alice taps "Message" on
  Bob's profile
- **THEN** a notice says direct messages are not available yet and no DM opens
