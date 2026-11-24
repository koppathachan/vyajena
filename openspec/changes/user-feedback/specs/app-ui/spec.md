## ADDED Requirements

### Requirement: Send feedback from the Profile tab
The Profile tab SHALL offer a Send feedback entry that uses the server's
advertised feedback channel (user-feedback: Feedback channel is operator-
configured). The entry SHALL appear only when the chosen server advertises an
email or URL channel, and SHALL be hidden or disabled when the server advertises
none. On use it SHALL open a compose screen that shows the destination and the
exact payload before anything is sent (user-feedback: The user sees exactly what
is sent and to whom), then hand off to the OS mail composer for an email channel
or open the URL for a URL channel. The app enforces this.

#### Scenario: Send feedback with an email channel
- **WHEN** the server advertises a feedback email and the user taps Send feedback,
  writes a note, and confirms
- **THEN** the app opens the OS mail composer addressed to the operator's email
  with the note prefilled, and the user taps Send there

#### Scenario: Send feedback with a URL channel
- **WHEN** the server advertises a feedback URL and the user taps Send feedback
- **THEN** the app shows the destination and opens that URL for the user to file
  their report

#### Scenario: No channel, no entry
- **WHEN** the chosen server advertises no feedback channel
- **THEN** the Profile tab shows no Send feedback entry (or shows it disabled with
  a note that this server has no feedback channel)
