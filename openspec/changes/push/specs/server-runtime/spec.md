## MODIFIED Requirements

### Requirement: Push provider interface
The server SHALL send wake pings through a push provider interface. Started
with `--push <push.toml>`, it SHALL send FCM pings with the FCM HTTP v1 API
when the push credentials folder holds a Google service account key, and APNs
pings with the APNs HTTP/2 API and a token-based (`.p8`) key it signs itself
when the folder holds one; for a platform without credentials, or without
`--push`, it SHALL use a logging provider that records only the recipient
device id and platform, never content, sender or group.

#### Scenario: Local wake ping
- **WHEN** a message is queued for Bob's offline phone on a server without a push
  provider configured
- **THEN** the server logs one wake ping for Bob's phone's device id and sends
  nothing

#### Scenario: Real FCM ping
- **WHEN** a message is queued for Bob's offline Android phone on a server
  started with `--push` and an FCM service account
- **THEN** the server gets an OAuth access token for the service account and
  posts one `messages:send` request for Bob's FCM token

#### Scenario: Only APNs configured
- **WHEN** the credentials folder holds an APNs key but no service account and
  a message is queued for Bob's offline Android phone
- **THEN** the server logs the ping for Bob's phone and sends nothing

## ADDED Requirements

### Requirement: Push configuration
`--push` SHALL name a TOML file with `credentials_dir` (the folder with the
service account key `fcm-service-account.json` and the APNs key
`AuthKey_<key id>.p8`), `apns_team_id`, `apns_topic` and an optional `proxy`
(`http://host:port`, the egress proxy every push request goes through). The
Server SHALL refuse to start, naming the problem, if that file has a missing,
invalid or unknown setting, if `credentials_dir` or a key file in it is
readable by group or others, if a key does not parse, or if it holds more than
one `AuthKey_*.p8`. The server enforces this.

#### Scenario: Key readable by others
- **WHEN** the owner starts the server with `--push` and
  `fcm-service-account.json` has mode 0644
- **THEN** the server exits with an error naming the file and opens no port

### Requirement: Push errors
The server SHALL clear a device's push token when the platform says it is no
longer valid (FCM `UNREGISTERED`, `SENDER_ID_MISMATCH` or an invalid
`message.token`; APNs 410, `DeviceTokenNotForTopic`, or `BadDeviceToken` from
both the production and the sandbox endpoint), but only if the device still
has that same token. On 429 or 5xx it SHALL wait (the `Retry-After` time if
given, else doubling from 1 second) and retry at most 3 times, then drop the
ping; while it waits it SHALL send nothing else to that platform. An expired
access token or JWT SHALL be replaced once and the ping retried. Push logs
SHALL name only the device id, the platform and the outcome, never the token,
a message id, or the response body. The server enforces this.

#### Scenario: Uninstalled app
- **WHEN** FCM answers a ping for Bob's phone with 404 `UNREGISTERED`
- **THEN** the server clears Bob's phone's token and sends it no more pings

#### Scenario: New token raced the error
- **WHEN** Bob's phone registers a new token while FCM is answering
  `UNREGISTERED` for its old one
- **THEN** the server keeps the new token

#### Scenario: FCM overloaded
- **WHEN** FCM answers 503 twice and then 200
- **THEN** the server sends the same ping three times, waiting 1 s and then 2 s,
  and logs one sent ping

### Requirement: Egress proxy
The server binary SHALL provide `egress-proxy --listen <addr>`, an HTTP
`CONNECT` proxy that SHALL tunnel only to `fcm.googleapis.com`,
`oauth2.googleapis.com`, `api.push.apple.com` and
`api.sandbox.push.apple.com` on port 443, SHALL refuse any other host, port or
method with 403 or 405, and SHALL refuse to tunnel to a name that resolves to a
loopback, private, link-local or unspecified address. It sees only TLS bytes.
The proxy enforces this.

#### Scenario: Compromised server dials out
- **WHEN** a process on the server asks the proxy for `CONNECT attacker.example:443`
- **THEN** the proxy answers 403 and opens no connection

#### Scenario: Rebinding to the LAN
- **WHEN** a request for `CONNECT fcm.googleapis.com:443` resolves to
  `<router>`
- **THEN** the proxy answers 403 and opens no connection
