## MODIFIED Requirements

### Requirement: Ordered delivery and catch-up
The server SHALL deliver to each device its commit log entries, welcomes and queued
application entries merged in `server_seq` order. A reconnecting device SHALL
receive everything after its cursor in that order before any live traffic. On the
committing device's own stream, the server SHALL deliver every entry for that device
with a `server_seq` below an accepted commit's `server_seq` before it sends the
`Accepted` result for that commit.

#### Scenario: Catch-up across an epoch change
- **WHEN** a device was offline while message M1 (epoch 4), commit C (epoch 4 to 5)
  and message M2 (epoch 5) were accepted
- **THEN** on reconnect it receives M1, then C, then M2

#### Scenario: Message just before own commit
- **WHEN** Bob's message M (epoch 5) is accepted at `server_seq` 10 and Alice's
  commit C (epoch 5 to 6) at `server_seq` 11
- **THEN** Alice's stream delivers M before `Accepted` for C, so she decrypts M in
  epoch 5 before she merges C
