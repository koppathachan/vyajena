## ADDED Requirements

### Requirement: Stop the core when the phone locks
The app SHALL stop the core and give back any iOS background time as soon as the
system reports that its protected data has become unavailable (the device locked)
while the app is keeping the core running in the background to finish a send
(app-ui: Finish sends in the background), and SHALL NOT begin that background wait
while protected data is already unavailable. On iOS the app observes
`UIApplication.protectedDataWillBecomeUnavailableNotification` and reads
`UIApplication.isProtectedDataAvailable`; other platforms report protected data as
always available and never lock. This keeps the core from reading its
`NSFileProtectionComplete` database while the device is locked, which faults
(SIGBUS). The app enforces this.

#### Scenario: Lock during a background send
- **WHEN** Bob sends a message that still shows ⏳, switches away so the core keeps
  running to finish it, and then locks the phone
- **THEN** the app stops the core and ends the iOS background time at once, and the
  message waits for the next time the app opens (unlocked)

#### Scenario: Already locked when backgrounded
- **WHEN** the app is backgrounded with a message still unsent while the phone is
  already locked
- **THEN** the app stops the core at once and does not start the finish-sends wait

#### Scenario: Not locked finishes the send as before
- **WHEN** Bob sends a ⏳ message and switches away without locking the phone, and
  the core is connected
- **THEN** the core keeps running, finishes the send, and the app stops it as the
  finish-sends-in-background behaviour already specifies
