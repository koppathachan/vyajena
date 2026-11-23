## 0. Before starting

- [ ] 0.1 `dms` and `lobby-multi-group` merged (the chats the shell lists); media
      changes merged for the inline players (the shell can land on what exists and
      reveal the rest as they ship)
- [ ] 0.2 Pure app change — no proto, protocol, server or bridge protocol change

## 1. Navigation shell

- [ ] 1.1 A bottom `NavigationBar` with Chats / Contacts / Profile; the Profile
      destination icon is the user's round avatar; tab state and scroll preserved;
      widget tests `app-shell/bottom-navigation`

## 2. Chats tab

- [ ] 2.1 Unified list: lobby pinned, named groups, DMs; row = avatar, name,
      last-message preview with a media glyph, timestamp, unread + `@` badge; closed
      DMs greyed at the bottom with the swipe-to-remove affordance
      (delete-messages); widget tests `app-shell/chats-list`

## 3. Conversation screen

- [ ] 3.1 One screen for lobby / named group / DM: bubbles, sender name in groups
      only, links, inline image/audio/video players, mentions/replies; widget tests
      `app-shell/conversation-screen`
- [ ] 3.2 Composer: attachment sheet (photo / video / audio / file) + mic button;
      app bar (back, avatar, title + subtitle, overflow); admin actions only on an
      admin device; widget tests `app-shell/conversation-screen`

## 4. Contacts tab

- [ ] 4.1 Members list with avatars, the `admin` badge and the "you" marker;
      Invite; admin create-group and replace-user entry points; tap a member to
      open/start a DM; widget tests `app-shell/contacts-tab`

## 5. Profile tab

- [ ] 5.1 The user's profile (display name, about), server info and settings behind
      the avatar tab; widget tests `app-shell/profile-tab`

## 6. Check

- [ ] 6.1 `cd app && flutter analyze`, `flutter test`,
      `openspec validate app-shell --strict`
