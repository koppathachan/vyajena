## 1. Trim the conversation overflow menu (`app-shell`: Conversation screen)

- [x] 1.1 Remove `MenuEntry` (members, profile, linkDevice, invite, invites,
      removeUser) from `chat/chat_screen.dart`; `ChatMenu` keeps only the mute
      toggle and the lobby's greyed notes.
- [x] 1.2 Widget test: on the lobby and a DM, the overflow menu offers mute/unmute
      and none of Members, Profile, Link a new device, Invite, Invites or Remove a
      user (`// spec: app-shell/conversation-screen`).

## 2. Contact tap opens the member profile (`app-ui`: Contact tap)

- [x] 2.1 Extract `MemberUser`, `byUser`, `devicesText` to `members/member.dart`;
      re-point `contacts_tab`, `chat/chat_list_screen`, `admin/remove_user_screen`,
      `admin/create_named_group_screen`.
- [x] 2.2 Add `members/member_profile_screen.dart`: avatar, name, device count, a
      Message button (opens/starts the DM, else the "not available yet" notice), and
      Report / Block / Unblock (reuse `app/moderation.dart` and the core calls).
- [x] 2.3 `contacts_tab.dart` `_tapMember`: another member → push the member
      profile; own row → Profile tab (unchanged).
- [x] 2.4 Widget tests: a tap opens the profile; Message opens the DM when offered;
      Message shows the notice when not; own row opens the Profile tab
      (`// spec: app-ui/contact-tap`, `// spec: app-shell/member-profile`).

## 3. Retire the standalone members list (`app-shell`)

- [x] 3.1 Delete `members/members_screen.dart` (`MembersScreen`); remove
      `Routes.members` and its builder/import from `app/routes.dart`.
- [x] 3.2 Remove the Profile tab's "Members and devices" entry from
      `profile/profile_screen.dart`.
- [x] 3.3 Migrate every test that drove the old chat overflow menu or the members
      list to the new entry points: `members/members_test`, `members/moderation_test`
      and `chat/dms_test` → Contacts tab / member profile; `admin/admin_test` →
      Contacts-tab admin actions; `devices/link_test` and `profile/profile_test` →
      Profile tab; `chat/notices_test` → Contacts tab.
- [x] 3.4 MODIFY `app-ui` Open a DM: "Message" lives on the member profile and, on
      an old (protocol v1) server, says DMs are not available yet instead of hiding.

## 4. Verify

- [x] 4.1 `cd app && flutter analyze` clean.
- [x] 4.2 `cd app && flutter test` green.
- [x] 4.3 `openspec validate shell-nav-consistency --strict` passes.
