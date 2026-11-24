## ADDED Requirements

### Requirement: Feedback reaches this server's own operator
Send feedback SHALL route the user to the operator of the server this install is
on, never to the app publisher and never to any address compiled into the build.
The destination SHALL come at runtime from the chosen server's
`ServerInfo.feedback` (Feedback channel is operator-configured), so that a user on
operator X's server reaches operator X. This preserves the self-hostable model
(self-hostable: Published build is operator-agnostic): there is no central inbox,
and each operator receives their own users' feedback. The app enforces this.

#### Scenario: Two operators, one build
- **WHEN** a user on operator A's server and a user on operator B's server each
  send feedback from the same published build
- **THEN** A's user reaches A's configured channel and B's user reaches B's, with
  no shared or baked-in destination

#### Scenario: Reaches the operator, not the publisher
- **WHEN** a user sends feedback
- **THEN** it goes to the server operator's configured channel, and no feedback is
  sent to the app publisher or any default address

### Requirement: Feedback channel is operator-configured
The server SHALL advertise its feedback channel in `ServerInfo.feedback` as a
method (none, email, or URL), a target (an email address or an `https` URL) and an
optional label, taken from operator configuration, and SHALL store no feedback
content and provide no feedback RPC (invariant: the server holds no durable
message content). An operator that configures nothing SHALL advertise `NONE`, and
the app SHALL then offer no feedback channel. The server enforces this.

#### Scenario: Email channel
- **WHEN** the operator configures a feedback email and a client calls
  `GetServerInfo`
- **THEN** `ServerInfo.feedback` names method `EMAIL`, that address, and the label

#### Scenario: No channel configured
- **WHEN** the operator configures no feedback channel
- **THEN** `ServerInfo.feedback` is `NONE` (or unset) and the server stores nothing
  and exposes no submit RPC

#### Scenario: Nothing stored on the server
- **WHEN** a user sends feedback through the advertised channel
- **THEN** the feedback does not pass through the server and the server keeps no
  copy of it

### Requirement: The user sees exactly what is sent and to whom
Before any feedback leaves the device the app SHALL show the destination (the
operator's configured email or URL) and the exact text to be sent, and SHALL send
only the text the user typed plus diagnostics the user can see and has opted to
include (app version, `server_id`, platform). The app SHALL NOT attach message
content, contacts, keys, or any other personal data silently, and SHALL embed no
feedback destination in the build (self-hostable: operator-agnostic; the only
destination used is the one the chosen server advertised). A malformed or
non-`mailto`/`https` target SHALL be treated as no channel. The app enforces this.

#### Scenario: Preview before sending
- **WHEN** the user writes feedback and proceeds
- **THEN** the app shows the destination and the exact payload, and nothing is sent
  until the user confirms in the mail composer or the opened page

#### Scenario: No silent PII
- **WHEN** feedback is sent
- **THEN** it contains only the user's typed text and the diagnostics the user
  chose to include, and no message content, contacts, keys, or hidden identifiers

#### Scenario: Nothing hardcoded
- **WHEN** a guard test scans the built client and app for a feedback email address
  or feedback URL constant
- **THEN** it finds none; the destination is only ever the chosen server's
  advertised channel

#### Scenario: Hostile target is ignored
- **WHEN** `ServerInfo.feedback` carries a target that is neither a valid email nor
  an `https` URL
- **THEN** the app treats the server as having no feedback channel and sends
  nothing
