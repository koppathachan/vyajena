# Store listing (design D15)

The text the owner pastes into Play Console and App Store Connect. English
(en-US). Distribution is private for now (internal testing, R1), so the
listing is seen only by invited testers; it is written to stand for a public
listing later.

## Both stores

- **Name:** Vyajena
- **Package / bundle id:** `com.purplechaamba.vyajena`
- **Privacy policy URL:** https://www.purplechaamba.com/vyajena/privacy/
- **Support / website URL:** https://www.purplechaamba.com/vyajena/
- **Category:** Communication (Play) · Social Networking (App Store)
- **Price:** free, no in-app purchases, no ads

## Short description (Play, ≤ 80 characters)

Private, end-to-end encrypted text chat for one invite-only group.

## Subtitle (App Store, ≤ 30 characters)

Private group chat, encrypted

## Keywords (App Store, ≤ 100 characters)

encrypted,private,group,chat,family,messenger,secure,MLS,invite

## Full description

Vyajena is a quiet, private chat for one group: a family, a few friends, a
small team. Its admin runs the server, invites each member by name, and
nobody else gets in.

- End-to-end encrypted with MLS (RFC 9420): only the group's members can read
  messages; the server only passes them on.
- Keys stay in your phone's key store and never leave the phone.
- Text only. No ads, no analytics, no tracking, no contacts upload.
- Link a second phone to your account.
- Light and dark mode, following your phone.

You need an invite from your group's admin to use Vyajena.

## What's new (first release)

First release of the new Vyajena.

## Age rating answers (IARC, saved in Play 2026-11-15)

Category: Social or communication → **Communication** (people already known to
the user, small group). Precise location shared: no. Digital purchases: no.
Users can block: no. Users can report: no. Chat moderation: no (end-to-end
encrypted; only the admin removes people). Interactions limited to invited
people: yes. Ratings contact: privacy@purplechaamba.com.

Result: ESRB Everyone · PEGI Parental guidance · USK All ages · ClassInd All
ages · IARC (rest of world) 12+, all with "Users interact". Re-answer if block,
report or moderation is added.

## Reviewer access (App Review / Play "App access")

Possible since the relay (2026-11-16): invites name
`vyajarelay.purplechaamba.com:443`, reachable from anywhere. An invite is
one-time and expires after 7 days (`INVITE_TTL`), and a server holds one
group.

**Owner, 2026-11-16: reviewers never join the real group** ("we can't let
reviewers access to internal chats"). They join a separate review server
(change `review-server`): `vyajarelay.purplechaamba.com:8443`, its own data,
keys and pin, ids `<name>@review.purplechaamba.com`. Its admin is
`review@review.purplechaamba.com` on the iOS simulator "Vyajena Review".

Per review: on that simulator, menu → Invite a user → `playreview` → Copy;
the owner pastes the invite into the store form (one-time, 7 days, never in
the repo). After the review: menu → Remove a user (or
`deploy/pi/deploy.sh --reset-review`).

Play "App access" text (instructions only; the owner pastes the invite):

> Vyajena is a private chat for one invite-only group; there is no sign-up or
> password. 1. Open the app; it shows "Join the group". 2. Paste the invite
> below into the Invite field and tap "Check invite". 3. Tap "Join". You are
> now in a review group with its admin ("review") and can send and read
> messages. Admin screens (inviting and removing people) appear only for the
> group's admin. The invite works once and expires 7 days after it was
> made; if it has expired, write to privacy@purplechaamba.com for a new one.

## Screenshots

`app/store/screenshots/` (from `integration_test/screenshots_test.dart` on the
fake core, never a real group): iPhone 6.9" (1320×2868) and an Android phone.
