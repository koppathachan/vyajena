## MODIFIED Requirements

### Requirement: Content-free wake pings
The Pi SHALL send a wake ping through FCM or APNs when it adds an entry to the
queue of a device that has no open `Connect` stream and has a push token.
The ping SHALL contain no message content, sender, group id or message count:
it carries only one fixed notification text, "New message", the same for every
ping, shown as a visible alert on iOS and as a notification on Android, and
pings to the same device replace each other instead of stacking. The Pi SHALL
send at most one ping per device per 60 seconds. The Pi enforces this.

#### Scenario: Offline device
- **WHEN** a message is queued for a device with no open stream
- **THEN** the Pi sends one wake ping with no content to that device's push token

#### Scenario: Connected device
- **WHEN** a message is queued for a device with an open stream
- **THEN** the Pi delivers it on the stream and sends no wake ping

#### Scenario: Bob's closed iPhone
- **WHEN** Alice sends "dinner at 8?" while Bob's iPhone app is closed and its
  APNs token is registered
- **THEN** APNs receives one alert whose only text is "New message", with no
  sender, group id, count or any part of "dinner at 8?"
