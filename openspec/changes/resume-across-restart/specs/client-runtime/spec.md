## ADDED Requirements

### Requirement: Resume across restart
The core SHALL save its latest session token, with its expiry and renewal
point, in the key store each time it obtains or renews one, and SHALL load it
when the client starts, so a start within the token's life reuses it (the
session-resume path of `wire-transport`) instead of running the full
challenge/auth handshake. It SHALL reuse a loaded token only while it is before
its renewal point, and SHALL fall back to the full handshake when the token is
missing, unreadable, past its renewal point or rejected by the server with
`UNAUTHENTICATED`. The core SHALL keep the token only in the key store, never in
SQLite, a log or any other file, and SHALL delete it when the server answers
`Authenticate` or the stream with `PERMISSION_DENIED` and when it creates fresh
device keys. A key store failure SHALL NOT stop the client: it connects with the
full handshake.

#### Scenario: Reopen within the token's life
- **WHEN** Bob's app goes to the background, so the host calls `stop()`, and he
  reopens it two hours later, so the host calls `start()`, with his six-hour
  session token still before its renewal point
- **THEN** the core opens the new `Connect` stream with the saved token, the server
  issues no new session, and no `GetChallenge` or `Authenticate` is sent

#### Scenario: Reopen after the token is due for renewal
- **WHEN** Bob reopens the app after his saved token's renewal point
- **THEN** the core runs the full challenge/auth handshake and saves the fresh
  token

#### Scenario: Renewed token is the one saved
- **WHEN** Bob's core renews its session token while connected and the app is
  then stopped and started
- **THEN** the start resumes with the renewed token, not the first one

#### Scenario: Token kept only in the key store
- **WHEN** Bob's core has connected
- **THEN** the session token is in the key store and appears in no SQLite row,
  log line or file

#### Scenario: Token the server no longer holds
- **WHEN** Bob reopens the app with a saved token the server has since dropped, so the
  stream open is refused with `UNAUTHENTICATED`
- **THEN** the core runs the full handshake on the same attempt and connects

#### Scenario: Removed device forgets the token
- **WHEN** the admin removes Bob's phone and its core gets `PERMISSION_DENIED`
- **THEN** the saved token is deleted from the key store, and a later start does
  not try to resume

#### Scenario: Key store fails
- **WHEN** the key store cannot be read or written while Bob's core saves or loads
  the token
- **THEN** the core logs the failure without the token and connects with the full
  handshake
