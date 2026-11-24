## ADDED Requirements

### Requirement: Contact about as the DM subtitle
The Chats tab SHALL show the other person's `about` text (`Profile.about`,
message-content: User-owned profiles) as the subtitle of an **open DM** row, in
place of the last-message preview. When the peer has set no `about`, the row SHALL
fall back to the last-message preview, and when the DM has no messages yet it SHALL
show a neutral placeholder. Lobby and named-group rows SHALL keep the last-message
preview with its media glyph, and a closed-DM row SHALL keep its greyed
swipe-to-remove treatment. The subtitle is ellipsised to one line; the avatar, the
presence dot, the timestamp and the unread count are unchanged. The app enforces
this.

#### Scenario: DM row shows the peer's about
- **WHEN** Alice opens the Chats tab and her DM peer Bob has set his `about` to
  "coffee and code"
- **THEN** Bob's DM row shows "coffee and code" as its subtitle, not his last message

#### Scenario: DM peer with no about
- **WHEN** Alice's DM peer Carol has set no `about`
- **THEN** Carol's DM row falls back to the last-message preview

#### Scenario: Group row keeps the preview
- **WHEN** Alice opens the Chats tab with the lobby and a "Weekend" named group
- **THEN** both group rows still show the last-message preview with its media glyph
