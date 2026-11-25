## 1. Proto / contract

- [ ] 1.1 None — confirm no `.proto`, `protocol/`, server or core change is needed;
  the peer `Profile.about` and the chat summary already reach the app.

## 2. App (UI)

- [ ] 2.1 In the chats-list row builder, select the subtitle by row type per the D2
  table: DM (open) → peer `Profile.about`; empty `about` → last-message preview;
  no messages → neutral placeholder.
- [ ] 2.2 Keep lobby and named-group rows on the last-message preview + media glyph
  (unchanged).
- [ ] 2.3 Keep closed-DM rows on their existing greyed/swipe-to-remove treatment.
- [ ] 2.4 Ellipsise `about` to one line, matching preview truncation; keep timestamp,
  unread count, avatar and the `manual-presence` dot unchanged.

## 3. Tests

- [ ] 3.1 App widget tests: an open DM with a peer who set `about` shows the `about`
  as subtitle; a peer with no `about` falls back to the preview; a group row still
  shows the preview; a closed DM row is unchanged.
- [ ] 3.2 The `app-shell` requirement added here has a test named
  `// spec: app-shell/<title-slug>`.

## 4. Docs

- [ ] 4.1 Note the DM-subtitle rule where the chats-list behaviour is documented;
  flag the open D1 (group about-row) for the owner.
