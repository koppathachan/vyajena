## MODIFIED Requirements

### Requirement: Push token registration
A device SHALL register or replace its push token with `Devices.UpdatePushToken`,
stating the platform (FCM or APNs). A device SHALL be able to clear its token.
The server SHALL refuse with `INVALID_ARGUMENT` a token longer than 4096 bytes, one
with whitespace or control characters, and an APNs token that is not 64 to 200
hexadecimal characters. The server SHALL clear a device's token when the device is
revoked (`Devices.RevokeDevice`, `Admin.RemoveUser`). The server enforces this.

#### Scenario: Token rotation
- **WHEN** a device sends a new FCM token
- **THEN** the server replaces the old token and uses the new one for wake pings

#### Scenario: Bad APNs token
- **WHEN** Bob's iPhone sends an APNs token `../../3/device/x`
- **THEN** the server rejects it with `INVALID_ARGUMENT` and keeps the old token

#### Scenario: Revoked phone
- **WHEN** Bob revokes his lost phone from his tablet
- **THEN** the server clears the phone's push token and never pings it again
