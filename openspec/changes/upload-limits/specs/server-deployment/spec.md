## ADDED Requirements

### Requirement: Service resource limits
The unit that runs the real server SHALL cap its memory and tasks
(`MemoryHigh=384M`, `MemoryMax=512M`, `TasksMax=256`), so a runaway server is
throttled or restarted by systemd before it takes the host down. systemd enforces
the caps; the hardening check SHALL verify them and fail when one is missing.

#### Scenario: Limits present
- **WHEN** the owner runs the server's hardening check after a deploy
- **THEN** it reports the memory and task limits of `vyajena.service` as in place

#### Scenario: Limit removed
- **WHEN** someone deletes `MemoryMax` from the installed unit
- **THEN** the hardening check fails and names the limit
