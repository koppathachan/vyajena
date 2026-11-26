## ADDED Requirements

### Requirement: Update nudge
The app SHALL compare its running version against `ServerInfo.latest_app_version`
and `ServerInfo.min_app_version` (from the `GetServerInfo` the core performs
before `Connect`) and act on the result: below the minimum it SHALL show a
blocking "Please update" screen and not proceed to `Connect` on that server;
otherwise below the latest it SHALL show a soft, dismissible "Update available"
banner; otherwise it SHALL show nothing. The blocking state takes precedence over
the banner. The app compares versions as `major.minor.patch` integers (build
metadata after `+` ignored); an empty or non-numeric advertised value is ignored,
so the app never blocks or nags on an unparseable advisory. The app enforces this
on the client.

#### Scenario: Below minimum blocks
- **WHEN** the app's running version is `1.3.0` and `ServerInfo.min_app_version`
  is `1.4.0`
- **THEN** the app shows a blocking "Please update" screen with the store link and
  does not open `Connect` to that server

#### Scenario: Below latest nudges
- **WHEN** the running version is `1.5.0`, `min_app_version` is `1.4.0` and
  `latest_app_version` is `1.6.0`
- **THEN** the app connects as normal and shows a dismissible "Update available"
  banner linking to the store

#### Scenario: Up to date is silent
- **WHEN** the running version equals or exceeds `latest_app_version`
- **THEN** the app shows no banner and no blocking screen

#### Scenario: Unparseable advisory is ignored
- **WHEN** `min_app_version` is empty or `"newest"`
- **THEN** the app treats that advisory as absent and never blocks on it

### Requirement: Update link is a client constant
The app SHALL open only its own compiled-in store URL (App Store on iOS, Play
Store on Android) from the update nudge, and SHALL never open a URL taken from
`ServerInfo` or any other server field. The app enforces this on the client, so a
compromised or hostile server cannot redirect users to a phishing update page; the
server advertises version numbers only.

#### Scenario: Store link comes from the app, not the server
- **WHEN** the user taps "Update" on the banner or the blocking screen
- **THEN** the app opens its own platform store URL, and no URL from `ServerInfo`
  is read or opened
