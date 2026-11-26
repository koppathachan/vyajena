## ADDED Requirements

### Requirement: Simplified onboarding in open mode
The app SHALL, when the server advertises `ServerInfo.onboarding.mode = MODE_OPEN`
(server-runtime: Server advertises its onboarding mode), show a simplified
first-run screen that lets a new user pick a username and — only when
`join_code_required` is true — enter a join code, then calls the core's
`register_open`. When no admin is active the screen SHALL also offer a one-tap
"join as the admin (`admin_username`)" path that registers the admin username, after
which the lobby appears automatically (auto-lobby). For any other mode
(`MODE_INVITE` or an older server's `MODE_UNSPECIFIED`) the app SHALL show today's
paste-an-invite screen (app-ui: Join from an invite) unchanged.

#### Scenario: Admin joins the demo server directly
- **WHEN** the app points at an open-mode demo server with no active admin and the
  user taps "join as the admin (akhil)"
- **THEN** the core registers `akhil`, `Devices.GetSelf` reports admin, and the app
  lands in the lobby with no invite and no "create group" tap

#### Scenario: A reviewer joins with a username and join code
- **WHEN** the server advertises `mode = MODE_OPEN` and `join_code_required = true`, and
  a reviewer enters username `reviewer` and the join code
- **THEN** the app calls `register_open`, the reviewer registers, and the app shows
  it is waiting for the admin's device to add it (app-ui: Join from an invite,
  "Waiting to be added")

#### Scenario: Invite mode is unchanged
- **WHEN** the server advertises `mode = MODE_INVITE`
- **THEN** the app shows the paste-an-invite screen and no username/join-code screen
