## MODIFIED Requirements

### Requirement: Heartbeats
While a `Connect` stream is idle the client SHALL send a heartbeat at least every
25 seconds, and the server SHALL reply to each. The server SHALL close a stream that
has received nothing for 90 seconds.

In addition, the server SHALL keep each `Connect` connection alive with HTTP/2
keepalive PINGs: it SHALL send a keepalive PING on an otherwise-idle connection at
least every `HTTP2_KEEPALIVE_INTERVAL` (30 seconds) and SHALL close a connection
whose keepalive PING is unanswered within `HTTP2_KEEPALIVE_TIMEOUT` (20 seconds),
then treat the device as offline. A conforming client's HTTP/2 layer answers these
PINGs automatically, with no application involvement. This detects a dead or
half-open connection within about `HTTP2_KEEPALIVE_INTERVAL + HTTP2_KEEPALIVE_TIMEOUT`
— so an offline device's wake push fires promptly rather than waiting out the
90-second heartbeat-idle close, which remains as a backstop. `HTTP2_KEEPALIVE_INTERVAL`
and `HTTP2_KEEPALIVE_TIMEOUT` are operator-tunable.

#### Scenario: Silent client
- **WHEN** the server receives no frame on a stream for 90 seconds
- **THEN** the server closes the stream and treats the device as offline

#### Scenario: Half-open connection detected by keepalive
- **WHEN** a device's `Connect` connection becomes half-open (the app is frozen or
  the network drops) so it answers no HTTP/2 keepalive PING
- **THEN** the server closes the connection within about
  `HTTP2_KEEPALIVE_INTERVAL + HTTP2_KEEPALIVE_TIMEOUT`, treats the device as offline,
  and a wake push fires for any pending notifiable message — without waiting the full
  90-second heartbeat-idle timeout

#### Scenario: Live but quiet client is not reaped
- **WHEN** a device holds an open `Connect` stream but sends no application frames
- **THEN** its HTTP/2 layer answers the server's keepalive PINGs, so the connection
  stays open and the device keeps receiving on its stream
