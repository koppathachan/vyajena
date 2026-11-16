## MODIFIED Requirements

### Requirement: Join from an invite
The app SHALL let a new user join by pasting the invite text the admin shared,
SHALL show the username and server address the invite names before joining, and
SHALL call the core's `register` with it. A Pi that fails the invite's TLS pin
SHALL be shown as a server identity error, not as a network error. While the
device is in no group, the waiting screen SHALL show the newest refusal the
core reported (`CommitRefused`, e.g. a Welcome it could not use) with its
text and the time it came, and whether the core asked to be added again
(`ReaddRequested`: asked, or the daily limit reached). The client enforces
this.

#### Scenario: Bob joins
- **WHEN** Bob pastes the invite the admin made for `bob` and taps Join
- **THEN** he sees `bob@pi` on `127.0.0.1:8443` first, the core registers his
  phone with `Devices.Register`, and he lands in the chat

#### Scenario: Waiting to be added
- **WHEN** Bob has registered but the admin's device has not added him yet
- **THEN** his app says it is waiting for the admin's device, and shows the chat
  once the core emits `JoinedGroup`

#### Scenario: Welcome refused while waiting
- **WHEN** Bob's phone is in no group and the core emits `CommitRefused` with
  "a Welcome could not be used: …"
- **THEN** the waiting screen shows that text and the time it came, and the
  chat replaces it once the core emits `JoinedGroup`

#### Scenario: Asked to be added again
- **WHEN** Bob's core emits `ReaddRequested` with outcome asked
- **THEN** the waiting screen says "Your phone couldn't open the admin's
  invitation to the group. It has asked to be added again; this happens on its
  own. Reason: <error> (<time>)"; with outcome limit reached, it says the phone
  asked too often today and now waits for the admin

#### Scenario: Wrong pin
- **WHEN** the Pi at the invite's address presents a different TLS key
- **THEN** the app shows a server identity error and does not join

#### Scenario: Bad invite text
- **WHEN** Bob pastes text that is not an invite
- **THEN** the app says the invite is not valid and calls nothing
