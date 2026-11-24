## ADDED Requirements

### Requirement: Prompt reconnection after a dropped stream
The app SHALL re-open the `Connect` stream and catch up (message-delivery: Ordered
delivery and catch-up) promptly after its stream drops, on returning to the
foreground, and on regaining network, using a bounded backoff rather than waiting
for a user action. A delivered wake push SHALL likewise cause the app to connect
and catch up. This ensures a wake push, or simply coming back online, reliably
drains any queued backlog. The client enforces this.

#### Scenario: Reconnect on resume
- **WHEN** the app returns to the foreground after being backgrounded and its
  stream had dropped
- **THEN** it re-opens `Connect` and catches up on any queued messages without the
  user doing anything

#### Scenario: Reconnect on network regain
- **WHEN** the device loses and then regains connectivity
- **THEN** the app re-opens `Connect` with a bounded backoff and catches up

#### Scenario: Wake push leads to catch-up
- **WHEN** the OS delivers the content-free wake push and the user opens it
- **THEN** the app connects and catches up on the backlog that triggered the push

#### Scenario: Backoff, not a tight loop
- **WHEN** the network is flapping and connect attempts keep failing
- **THEN** the app retries with a bounded backoff instead of reconnecting in a tight
  loop
