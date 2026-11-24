## ADDED Requirements

### Requirement: Re-ping on stream loss with a backlog
The server SHALL request a content-free wake push for a device when its `Connect`
stream closes or is reaped for idle timeout while that device still has
undelivered queued entries and a push token, subject to the existing
one-per-device-per-60-second limit (Content-free wake pings). This closes the
window in which messages are queued against a stream that is open but dead: those
entries get a push as soon as the server treats the device as offline, instead of
no push at all. The push payload is unchanged — the one fixed "New message", no
content, sender or count. The server enforces this.

#### Scenario: Messages queued during the zombie window
- **WHEN** Alice's phone backgrounds, messages are queued for it while its stream
  is still open, and the server then reaps that stream for idle timeout with the
  backlog still undelivered
- **THEN** the server requests one content-free wake push for the phone, where
  before the reap left the backlog with no push

#### Scenario: Still rate-limited
- **WHEN** the stream is reaped within 60 seconds of a store-time wake push already
  sent to that device
- **THEN** the server sends no second push in that window; the single push stands

#### Scenario: No backlog, no push
- **WHEN** a device's stream closes and it has no undelivered queued entries
- **THEN** the server requests no wake push for it

### Requirement: Sweep catch-up wake ping
On each periodic sweep the server SHALL request a content-free wake push for every
device that has no open `Connect` stream, has undelivered queued entries, and has a
push token, rate-limited by the one-per-device-per-60-second rule, and SHALL repeat
this on later sweeps until the device reconnects and acknowledges its backlog (the
queue drains) or a bounded number of catch-up attempts for that backlog is reached.
A wake push dropped by the rate limit, collapsed with an earlier one, or lost in
transit is therefore retried rather than lost after one attempt. A catch-up push is
still a single content-free "New message" and is still subject to any notification
suppression (mute-notifications: A muted conversation gets no wake push). The server
enforces this.

#### Scenario: A lost ping is retried
- **WHEN** the one store-time wake push for Alice's offline phone is lost, and her
  phone is still offline with the message undelivered
- **THEN** a later sweep requests another wake push for the phone, within the
  rate limit, so the ping is not lost after a single attempt

#### Scenario: Catch-up stops once the backlog drains
- **WHEN** Alice's phone reconnects, catches up, and acknowledges all its queued
  entries
- **THEN** the sweep requests no further catch-up push for it and its attempt count
  resets

#### Scenario: Bounded attempts for a dead device
- **WHEN** a device never reconnects and its backlog stays undelivered
- **THEN** the sweep stops requesting catch-up pushes after the bounded number of
  attempts, and a platform "unregistered" response clears the token entirely
  (push: Push errors)

#### Scenario: The attempt bound survives a server restart
- **WHEN** a device has already used its bounded catch-up attempts while offline
  and the server restarts with the backlog still undelivered
- **THEN** the sweep does not resume catch-up pushes for it — the attempt count is
  durable, so a restart hands no fresh budget — until the device drains its backlog
  or reconnects, which resets the count

#### Scenario: Connected device is never swept-pinged
- **WHEN** a device has an open `Connect` stream
- **THEN** the sweep requests no wake push for it, whatever its backlog, because it
  is receiving on the stream

### Requirement: No undelivered message is left unpinged
A stored, undelivered queued entry for an offline device with a push token SHALL
keep producing content-free wake pushes — within the one-per-60-second limit and
the catch-up attempt bound — until the entry is delivered or it expires
(Expiry and gap notice), so no message sits pinged-once-and-forgotten or, in the
zombie window, never pinged. This invariant is satisfied together by Content-free
wake pings, Re-ping on stream loss with a backlog, and Sweep catch-up wake ping.
The server enforces this.

#### Scenario: Nothing goes silent
- **WHEN** a message is stored for Alice's offline phone and the first push does
  not result in a reconnect
- **THEN** the phone keeps being nudged by catch-up pushes (rate-limited, bounded)
  until it reconnects and receives the message, or the message expires — never
  left silently undelivered pending a manual reopen
