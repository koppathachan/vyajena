## ADDED Requirements

### Requirement: Call signalling in a DM
A call SHALL be signalled with a `CallSignal` `Content` body in an existing DM
between the two users, carried over `Messaging.Connect` like any other application
message. `CallSignal` SHALL hold exactly one of `CallOffer`, `CallAnswer`,
`IceCandidate`, `CallHangup` or `CallReject`, each naming the call's 16-byte
`call_id`. This change SHALL NOT open a DM; a call is offered only where a DM
already exists. A client that predates `CallSignal` SHALL show the unknown-body
placeholder (message-content: Inner content format) and cannot be called. The
client enforces this.

#### Scenario: Alice calls Bob
- **WHEN** Alice starts a call in her DM with Bob
- **THEN** her core sends a `Content` with a `CallOffer` naming a fresh `call_id`,
  and Bob's DM shows an incoming-call message

#### Scenario: No DM, no call
- **WHEN** Alice has no DM with Carol
- **THEN** the app offers no call action for Carol

#### Scenario: Old client is uncallable
- **WHEN** a client that predates `CallSignal` receives a `CallOffer`
- **THEN** it shows the "needs a newer app version" placeholder and sends no answer

### Requirement: Call key binding
A caller's core SHALL export `mls_call_auth = export_secret("vyajena2-call-v1",
call_id, 32)` from the DM group and SHALL put `auth_tag = HMAC-SHA256(mls_call_auth,
dtls_fingerprint)` and its DTLS fingerprint in the `CallOffer`; the callee SHALL
likewise bind its fingerprint in `CallAnswer`. Each side SHALL verify the other's
`auth_tag` against the received fingerprint before media flows and SHALL drop the
call on a mismatch. The client enforces this.

#### Scenario: Fingerprint is authenticated
- **WHEN** Bob receives Alice's `CallOffer`
- **THEN** his core derives the same `mls_call_auth` from the DM and accepts the
  call only if `auth_tag` matches Alice's DTLS fingerprint

#### Scenario: Swapped DTLS key
- **WHEN** a TURN relay or network attacker replaces Alice's DTLS fingerprint on the
  media path
- **THEN** the fingerprint no longer matches `auth_tag` and the callee drops the call

### Requirement: Epoch-bound setup
A `CallOffer` SHALL carry the DM epoch (`dm_epoch`) at which `mls_call_auth` was
exported. The callee SHALL answer only at that epoch, processing pending commits to
reach it; if the DM has advanced past `dm_epoch` (strict `max_past_epochs = 0`, so
the epoch secret is gone) the callee SHALL reply `CallReject{epoch_stale}` and the
caller SHALL re-offer at the new epoch with a fresh `call_id`. The client enforces
this.

#### Scenario: Epoch advanced before answer
- **WHEN** a leaf rotation advances the DM epoch after Alice's offer and before Bob
  answers
- **THEN** Bob replies `CallReject{epoch_stale}` and Alice's core re-offers at the new
  epoch

### Requirement: Single active call
A device SHALL hold at most one active or ringing call. A device that receives a
`CallOffer` while it already has a call SHALL answer the new offer automatically with
`CallReject{busy}`. An unanswered offer SHALL expire after
`protocol::calls::OFFER_TTL` (~45 s) with `CallHangup{missed}`. The client enforces
this.

#### Scenario: Busy
- **WHEN** Bob is already in a call and Dan calls him
- **THEN** Bob's core auto-sends `CallReject{busy}` and Dan sees the call declined as
  busy

#### Scenario: Missed call
- **WHEN** Alice calls Bob and he does not answer within `OFFER_TTL`
- **THEN** both DMs show "Missed call"

### Requirement: Media never through the server
Call media SHALL be WebRTC (DTLS-SRTP, Opus mono) carried peer-to-peer or through a
TURN relay, and SHALL NEVER traverse the server; the server SHALL have no media path
and SHALL see only the encrypted `CallSignal` DM messages it routes. The client and
the server both enforce this.

#### Scenario: Server sees no media
- **WHEN** Alice and Bob are on a call
- **THEN** the server routes their `CallSignal` messages as ciphertext and carries no
  audio; the SRTP media flows P2P or via TURN only

### Requirement: TURN credentials
`Calls.GetTurnCredentials` SHALL return the STUN/TURN endpoints and an ephemeral
username and credential whose TTL is at most `protocol::calls::TURN_CRED_TTL` (≤ 1
hour), computed by the server from a shared secret it holds only in its configuration
(never in the repo). The call SHALL require a valid session token. The server
enforces this.

#### Scenario: Ephemeral credential
- **WHEN** Alice's core calls `Calls.GetTurnCredentials`
- **THEN** the server returns a username and HMAC credential valid for at most one
  hour, and no long-lived TURN password is stored on the device or in the repo

### Requirement: No persisted call state
Call keys, ICE state, sockets and the audio pipeline SHALL live only in RAM in the
core and SHALL be zeroized on hangup, decline, busy or the offer timeout. Nothing
about a call SHALL be written to the SQLite store; the DM SHALL keep only the call's
message tiles ("Call ended", "Missed call", "Call declined") as ordinary `Content`,
holding no media or key. The client enforces this.

#### Scenario: Nothing kept after a call
- **WHEN** a call ends
- **THEN** the core has zeroized the call keys and sockets, the store holds no call
  media or key, and the DM shows a "Call ended" tile
