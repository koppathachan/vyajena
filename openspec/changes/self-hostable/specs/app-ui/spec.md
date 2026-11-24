## ADDED Requirements

### Requirement: Choose a server (first run)
On a fresh install with no account, the app SHALL, before the invite step, let the
user enter the server's public relay DNS address (a host, with the port defaulting
to 443), SHALL record it as the install's single server, and SHALL show it. This
step is local: the app SHALL NOT open a connection to the entered address here —
the invite pasted next is what proves the server's key (device-identity: Invites).
The flow is choose server → paste invite → register. The app enforces the ordering;
no server is contacted until `register`.

#### Scenario: Bob enters his friend's server
- **WHEN** Bob opens the app for the first time and types `chat.example.com`
- **THEN** the app stores `chat.example.com:443` as this install's server, shows
  it, and moves him to the invite step without contacting any server

#### Scenario: Server with an explicit port
- **WHEN** Bob types `chat.example.com:8443`
- **THEN** the app stores that host and port as the install's server

#### Scenario: Link a device starts from the chosen server
- **WHEN** Bob already has an account elsewhere and taps "link this device"
- **THEN** he still first confirms which server this install is for, then follows
  the device-linking flow (app-ui: Link a device)

### Requirement: Published build is operator-neutral in the UI
The app's onboarding copy SHALL name no single implied server or group: it SHALL
refer to "the server you were invited to" rather than assume one owner-run group.
The app's store identity (bundle id, display name) is the publisher's and is out of
scope for this requirement; only server identity is runtime-chosen.

#### Scenario: Onboarding text carries no server host
- **WHEN** the onboarding and choose-server screens are rendered
- **THEN** no server host, `server_id`, or group name is baked into their strings;
  the server shown is the one the user chose or the invite named

## MODIFIED Requirements

### Requirement: Join from an invite
After the server is chosen (app-ui: Choose a server), the app SHALL let a user join
by pasting or scanning the invite text the admin shared, SHALL show the username
and server address the invite names before joining, and SHALL call the core's
`register` with it. The app SHALL cross-check the invite's relay address and
`server_id` against the chosen server and SHALL warn, before joining, when they
differ. A server that fails the invite's TLS pin SHALL be shown as a server
identity error, not as a network error. The core, not the app, enforces the pin
(wire-transport: gRPC over HTTP/2 and pinned TLS on TCP 443).

#### Scenario: Bob joins
- **WHEN** Bob has chosen his server, pastes the invite the admin made for `bob`,
  and taps Join
- **THEN** he sees `bob@chat` on the invite's address first, the core registers his
  phone with `Devices.Register`, and he lands in the chat

#### Scenario: Waiting to be added
- **WHEN** Bob has registered but the admin's device has not added him yet
- **THEN** his app says it is waiting for the admin's device, and shows the chat
  once the core emits `JoinedGroup`

#### Scenario: Invite for a different server
- **WHEN** Bob chose `chat.example.com` but pastes an invite whose `relay_address`
  or `server_id` names a different server
- **THEN** the app warns that the invite is for a different server and does not
  register until Bob re-chooses the server or re-pastes a matching invite

#### Scenario: Wrong pin
- **WHEN** the server at the invite's address presents a different TLS key
- **THEN** the app shows a server identity error and does not join

#### Scenario: Bad invite text
- **WHEN** Bob pastes text that is not an invite
- **THEN** the app says the invite is not valid and calls nothing
