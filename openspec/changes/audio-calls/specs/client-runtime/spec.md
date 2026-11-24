## ADDED Requirements

### Requirement: Call media and keys stay in the core
The core SHALL own every call socket and key: the WebRTC peer connection, the ICE
agent, DTLS and SRTP, the Opus mono pipeline, the MLS-exported call secret and the
`auth_tag`. Dart SHALL hold no call socket and no key and SHALL only draw the call
screen. Call media SHALL go peer-to-peer or through TURN and SHALL never traverse
the server. The core SHALL keep all call state in RAM and SHALL zeroize it on
hangup, decline, busy or the offer timeout, writing nothing about a call to SQLite.
The core enforces this.

#### Scenario: Keys never reach Dart
- **WHEN** a call is connected
- **THEN** the DTLS/SRTP keys and the MLS call secret are in the core, and Dart has
  only the on-screen state (muted, elapsed, connected)

#### Scenario: Zeroized on end
- **WHEN** a call ends or times out
- **THEN** the core zeroizes the call keys and sockets and the SQLite store holds no
  call key or media
