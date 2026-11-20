## ADDED Requirements

### Requirement: Key storage
The core SHALL keep the user identity key, the device auth key and the MLS
signature key only in a key store: the OS keychain on desktop (macOS Keychain,
Linux Secret Service, Windows Credential Manager), Android Keystore or iOS
Keychain on phones. Only when the user explicitly opts in on a machine without
a keychain SHALL it use a key file with mode 0600. It SHALL never write these
keys to SQLite, logs or any other file. OpenMLS group state (epoch secrets, leaf
encryption keys, the secret tree) SHALL be stored in SQLite only encrypted with a
storage key that itself lives in the key store; messages stay unencrypted.

#### Scenario: Copied database
- **WHEN** someone copies Alice's SQLite file but not her key store
- **THEN** they can read her stored messages but cannot read her MLS group state,
  so they cannot decrypt new group traffic

#### Scenario: Laptop with a keychain
- **WHEN** Alice registers from her laptop's terminal client
- **THEN** her identity, auth and MLS signature keys are in the macOS Keychain and
  appear in no file or database row

#### Scenario: Headless machine without opt-in
- **WHEN** the core starts on a machine with no keychain and no file opt-in
- **THEN** it refuses to create keys and tells the user how to opt in

### Requirement: Local message store
The core SHALL store messages in a local SQLite database. It SHALL store the
plaintext of a message the user sends before sending it, because the sender
cannot decrypt its own ciphertext, and SHALL store each received message before
acknowledging it to the server.

#### Scenario: Own message survives a crash
- **WHEN** Alice sends "hi" and her laptop crashes before `Accepted` arrives
- **THEN** after restart "hi" is in her history marked as sending, and the core
  resends it with the same `msg_id`

#### Scenario: Crash before ack
- **WHEN** Bob's core crashes after storing a received message but before acking
- **THEN** the server redelivers it and Bob sees it once

### Requirement: Connection lifecycle
The core SHALL connect to the relay address from the invite, accept the server only
by its pinned TLS key, authenticate with its device auth key, renew its session
token before it expires, keep one `Connect` stream open with heartbeats, and
reconnect with backoff when the connection drops.

#### Scenario: Wrong pin
- **WHEN** the server presents a TLS key that does not match the invite's pin
- **THEN** the core does not connect and reports a server identity error to the UI

#### Scenario: Network drop
- **WHEN** Bob's laptop loses the network for a minute
- **THEN** the core reconnects on its own and catches up in order

### Requirement: Send status
The core SHALL report each sent message as sending until the server accepts it, then
as accepted. On `STALE_EPOCH` or a reconnect it SHALL process pending commits,
re-encrypt and resend with the same `msg_id`; on any other rejection it SHALL
report the message as failed with the reason.

#### Scenario: Message sent across a commit
- **WHEN** Alice's message is rejected with `STALE_EPOCH` because the admin just
  added Carol
- **THEN** the core applies the commit, resends the message in the new epoch, and
  Alice sees it go from sending to accepted

#### Scenario: Message typed while removed for inactivity
- **WHEN** Bob sends "back" while his laptop is removed for inactivity
- **THEN** it stays sending, and after the re-add the core sends it in the new
  epoch with the same `msg_id` and Bob sees it go to accepted

### Requirement: Events in order
The core SHALL deliver events to the UI in `server_seq` order through one event
stream: received messages, profile and group-setting changes, gap notices,
removal and rejoin notices, and identity-key-changed warnings.

#### Scenario: Back from holiday
- **WHEN** Bob's laptop returns after 10 days and is re-added
- **THEN** the UI gets a removed-for-inactivity event with the time messages stop,
  then the messages from after the re-add, in order
