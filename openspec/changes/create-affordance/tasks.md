## 1. Home is the chat list

- [ ] 1.1 In `app/lib/chat/group_gate.dart`, route to `ChatListScreen` whenever
  this device is in the group (`epoch() != null`), dropping the `_hasDms` branch,
  its `chats()` probe and the `_hasDms` field (spec: app-ui/chat-list).
- [ ] 1.2 In `app/lib/chat/chat_screen.dart`, remove the now-dead `forum` "Chats"
  app-bar button (the `isMain && hasDms && !canPop()` block and its `dmUnread`
  badge) and the unused `chat_list_screen.dart` import.

## 2. The `+` FAB

- [ ] 2.1 In `app/lib/chat/chat_list_screen.dart`, load `dmsAvailable()` alongside
  `chats()` in `_load`, and keep it in state.
- [ ] 2.2 Add a `FloatingActionButton` whose actions follow availability: New DM
  when `dmsAvailable`, New group when `admin`; one action runs directly, both open
  a stock bottom-sheet menu, neither hides the button (spec: app-ui/create-affordance).
- [ ] 2.3 New DM: open a bottom-sheet member picker from `members()` (deduped by
  user, excluding this device's own user), then `openDm(user)` and show the DM with
  the existing `showDm` helper; refresh the list on return (spec: app-ui/create-affordance).
- [ ] 2.4 New group: call `createGroup()` and refresh the list (spec: app-ui/create-affordance).

## 3. Tests (`app/test/`, tagged `// spec: app-ui/<slug>`)

- [ ] 3.1 Chat list always the home: the gate shows the chat list once in the
  group, with and without a DM (spec: app-ui/chat-list).
- [ ] 3.2 Create affordance: the FAB's New DM picks a member and calls `openDm`
  then shows the DM; New group is admin-only and calls `createGroup`; no FAB when
  neither action is available (spec: app-ui/create-affordance).

## 4. Verify

- [ ] 4.1 `cd app && flutter analyze` is clean.
- [ ] 4.2 `cd app && flutter test` is green (existing members-list "Message" path
  still passes).
- [ ] 4.3 `openspec validate create-affordance --strict` passes; tick the tasks.
