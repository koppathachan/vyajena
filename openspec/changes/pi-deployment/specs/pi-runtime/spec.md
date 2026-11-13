## ADDED Requirements

### Requirement: Graceful stop
The server SHALL stop cleanly on `SIGTERM` as well as on Ctrl-C: it stops
accepting connections, finishes the transaction in progress, closes the database,
and exits with status 0.

#### Scenario: systemd stops the service
- **WHEN** systemd stops the server while Alice's message is being accepted
- **THEN** either Alice gets `Accepted` and the message is stored, or she gets no
  answer and resends it after the restart; the server exits with status 0
