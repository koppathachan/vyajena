## ADDED Requirements

### Requirement: Call in a DM
The DM screen SHALL offer a call action, and SHALL show call signalling as message
tiles: an incoming `CallOffer` as "Incoming call" with Answer and Decline, an
outgoing offer as "Calling…" with Cancel, and the outcome as "Call ended", "Missed
call" or "Call declined". There SHALL be no OS ring and no native call UI; an
incoming call relies on the ordinary content-free "New message" notification. The
call action SHALL appear only in a DM and only when both the app and the core speak
`CallSignal`. The app enforces this.

#### Scenario: Incoming call tile
- **WHEN** Bob receives a `CallOffer` from Alice in their DM
- **THEN** the DM shows an "Incoming call" tile with Answer and Decline, and Bob gets
  the ordinary "New message" notification, with no ring

#### Scenario: Decline
- **WHEN** Bob taps Decline
- **THEN** the core sends `CallReject{declined}` and both DMs show "Call declined"

#### Scenario: Missed call
- **WHEN** Alice calls Bob and he does not answer in time
- **THEN** both DMs show "Missed call"

### Requirement: In-call screen
When a call is connected the app SHALL show an in-call screen with mute, hang up,
elapsed time and the connection state, drawing only what the core reports; it SHALL
hold no socket and no key (app-ui: UI goes through the core). Hanging up SHALL ask
the core to end the call. The app SHALL request microphone permission before the
first call. The app enforces this.

#### Scenario: Talk and hang up
- **WHEN** Alice and Bob are connected and Alice taps hang up
- **THEN** the core ends the call, media stops, and both DMs show "Call ended"

#### Scenario: Mic permission
- **WHEN** Bob answers his first call
- **THEN** the app requests microphone permission and the call connects once granted
