## Why

The `app-shell` restyle moved navigation to three tabs — Chats, Contacts and a
Profile tab reached by the user's own avatar. Two seams were left where the old,
pre-shell navigation still shows through and the owner noticed the duplication
(owner review, 2026-11-27):

1. **The conversation screen's overflow menu still lists the tabs.** On the lobby
   and in DMs the overflow menu (`ChatMenu`) offers Members, Profile, Link a new
   device, Invite, Invites and Remove a user — every one of which is now a tab
   (Contacts, Profile) or already offered on the Contacts tab. The menu duplicates
   the shell.
2. **Tapping a contact jumps straight into a DM.** The Contacts tab opens the DM
   the instant a member is tapped, so there is no way to look at a person — their
   name, avatar, devices — or to block or report them without first entering a
   conversation.

This is a **presentation and navigation change only** — no protocol, crypto,
server or wire change. It refines the existing `app-shell` and `app-ui`
behaviours.

## What Changes

- **Trim the conversation overflow menu** (`app-shell`: Conversation screen). The
  lobby/DM overflow menu keeps only conversation-scoped actions — mute/unmute, and
  for the lobby the greyed "name is fixed / cannot be closed" notes. Members,
  Profile and Link a new device are dropped (the Contacts and Profile tabs cover
  them); the admin Invite, Invites and Remove-a-user entries are dropped (the
  Contacts tab already offers them). A named group keeps its own group-management
  menu (`GroupMenu`) unchanged. "Link a new device" already lives on the Profile
  tab, so nothing is lost.
- **Contact tap opens the member's profile** (`app-ui`: Contact tap; `app-shell`:
  Contacts tab, Member profile). Tapping another member opens a **member profile**
  view — avatar, name, device count — with a clear **Message** button that opens or
  starts the DM (asking the core afresh whether DMs are offered, and showing the
  existing "Direct messages are not available yet" notice when they are not), and
  **Report** and **Block/Unblock** entry points (reusing `report-and-block`). The
  user's own row still opens the Profile tab.
- **Retire the standalone members list** (`MembersScreen`, `Routes.members`). The
  Contacts tab plus the new member profile supersede it: its per-member Message and
  report/block move into the member profile, and the Profile tab's "Members and
  devices" entry (its only remaining caller) is removed. The shared helpers it held
  (`MemberUser`, `byUser`, `devicesText`) move to `members/member.dart`.

## Open decisions

- None new. The three-tab shell and the "a contact tap always does something"
  guarantee are owner-resolved (2026-11-23, `arcade-restyle`: Contact tap). This
  change keeps that guarantee: a tap now always opens the profile.
- **For owner review:** removing the Profile tab's "Members and devices" entry.
  It pointed at the retired `MembersScreen`; the Contacts tab (members + per-member
  profile with device count and moderation) now covers it. Flagged here because it
  touches the Profile tab the owner approved.

## Proto files touched

- None. No `.proto`, `protocol/`, server or core change. Dart UI only, over the
  existing bridge API (no new bridge field).

## Capabilities

### Modified Capabilities

- `app-shell`: the Conversation screen's overflow menu is trimmed to
  conversation-scoped actions; the Contacts tab's tap opens a member profile; a new
  **Member profile** view.
- `app-ui`: the Contact tap opens the member's profile, with Message from there;
  Open a DM's "Message" moves to that profile and, on an old server, says DMs are
  not available yet instead of hiding.

## Impact

- **App (`app/`):** `chat/chat_screen.dart` (`ChatMenu`/`MenuEntry` trimmed),
  `members/contacts_tab.dart` (tap → profile), new
  `members/member_profile_screen.dart`, `members/member.dart` (extracted helpers),
  `members/members_screen.dart` removed, `app/routes.dart` (`Routes.members`
  removed), `profile/profile_screen.dart` ("Members and devices" entry removed),
  and the admin/chat-list importers of the helpers re-pointed. Widget tests updated
  and added.
- **No bridge/core/server change.**
- **Archive ordering:** this change MODIFIES requirements that currently live only
  in the still-active `arcade-restyle` (Contact tap), `dms` (Open a DM) and
  `app-shell` (Conversation screen, Contacts tab) deltas. Archive those three before
  this one so each MODIFY applies to a present base (see `design.md`).
