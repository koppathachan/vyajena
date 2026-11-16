## ADDED Requirements

### Requirement: Push notifications
The app SHALL register this device's push token (FCM on Android, APNs on iOS)
with the core's `set_push_token` each time the core connects and again
whenever the platform rotates the token, while notifications are allowed; when
they are not, it SHALL clear the token instead. It SHALL ask for notification
permission once per install, the first time the core connects after the
device joined or was linked, never before. Tapping the "New message"
notification SHALL open the app, which connects and catches up; opening the
app SHALL remove its shown notifications. When the device is removed the app
SHALL delete its platform token. The app never shows message text, a sender
or a count in a notification. The client enforces this.

#### Scenario: Bob joins
- **WHEN** Bob joins with an invite and his phone connects for the first time
- **THEN** the app asks once for notification permission and registers the
  token through the core

#### Scenario: Token rotation on Android
- **WHEN** Firebase gives Bob's phone a new FCM token while the app is open
- **THEN** the app sends the new token through the core's `set_push_token`

#### Scenario: Permission denied
- **WHEN** Bob denies notification permission
- **THEN** the app works as before, clears its token through the core, and
  does not ask again

#### Scenario: Removed device
- **WHEN** Bob's phone gets `Removed` from the core
- **THEN** the app deletes its platform push token
