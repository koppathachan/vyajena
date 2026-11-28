## MODIFIED Requirements

### Requirement: Notification detail level
The app SHALL show one notification detail for every device: the per-conversation
count and type (Type-aware notification wording) — for example "3 new in Work" or
"2 new DMs" — and the fixed admin-event phrases, never a sender and never any message
text. There SHALL be no per-device "Notification detail" level and no Basic/More
selector; the Profile screen MAY show a short static description of what notifications
contain, with no control to change it. The app SHALL show the same notification
content on iOS and Android. The app enforces this.

#### Scenario: Count and type, always
- **WHEN** Bob installs the app and three messages arrive in the group "Work"
- **THEN** his phone shows "3 new in Work", naming no sender — there is no generic
  "New message" level to fall back to

#### Scenario: No detail selector in settings
- **WHEN** Bob opens Profile and looks at "Notification detail"
- **THEN** he sees a description of what notifications show and no Basic/More control
  to change it

#### Scenario: Same on both platforms
- **WHEN** Bob has an iPhone and an Android phone and the same two DMs and one "Work"
  message are unread
- **THEN** both show the same count + type content (e.g. "2 new DMs, 1 new in Work"),
  neither naming a sender
