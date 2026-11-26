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
report or moderation is added. **⚠ Now stale: report/block shipped (#58), so
block/report/moderation are YES — re-answer the questionnaire at the next
submission.**

## Reviewer access (App Review / Play "App access")

Reviewers never join the real group. They join a **separate review server** —
`review@review.purplechaamba.com`, reachable at
`vyajarelay.purplechaamba.com:53404`, with its own data, keys and pin — as an
ordinary **member**, not an admin. (Model locked 2026-11-26; this replaces the
earlier "the reviewer is the review server's admin / creates the group" flow,
which is now wrong and would mislead a reviewer.)

Set-up (owner, in the app — no code change, and no server reset per review):

- The owner redeems the review server's admin bootstrap invite, creates the
  lobby, and **keeps an admin device online for the whole review window**.
- For each store the owner mints a single-use, 7-day **member** invite from the
  admin device (Invite a user → `ios` / `android`): the iOS reviewer joins as
  `ios@review.purplechaamba.com`, the Android reviewer as
  `android@review.purplechaamba.com`. The owner pastes the invite into the store
  form, never into the repo. Minting the invite and any server reset stay the
  owner's action — no submission session runs a server script.

Reviewer flow (what the store form tells the reviewer to do):

1. Open the app and paste the member invite.
2. Tap **Check invite**, then **Join**.
3. Wait briefly to be admitted by the admin device.
4. Use the app as a normal member: send text, photos, videos, voice messages
   and stickers; open a direct message with another member from the members
   list; report or block a user; and delete your own account from Profile.

**CRITICAL — an admin device must stay online for the whole review**, or the
reviewer stalls at "Waiting for the admin's device…" and may reject. The
reviewer is a member only: they never become admin and never create a group.
Run the account-deletion check from the member account — the admin account
cannot self-delete (the control is disabled).

Store-form text — one canonical version for Play "Sign-in details" and App
Store "App access". Play caps this field at 500 characters including the
invite, so trim the feature list if a longer invite would overflow:

> No sign-up or password. Open the app, paste the invite below, tap Check
> invite then Join, and wait briefly to be admitted. Then use it as a normal
> member: send text, photos, videos and voice; open a DM from the members list;
> report or block a user; delete your own account. The invite works once, on
> one device.
>
> <invite>

## Screenshots

`app/store/screenshots/` (from `integration_test/screenshots_test.dart` on the
fake core, never a real group): iPhone 6.3" (1206×2622) and an Android phone.
The committed `app/store/screenshots/ios/*.png` are still the pre-restyle
shots; the refreshed 1206×2622 Arcade set used on App Store Connect is pending
commit (the screenshot-test follow-up).
