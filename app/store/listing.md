# Store listing (design D15)

The text the owner pastes into Play Console and App Store Connect. English
(en-US). Distribution is private for now (internal testing, R1), so the
listing is seen only by invited testers; it is written to stand for a public
listing later.

## Both stores

- **Name:** Vyajena
- **Package / bundle id:** `com.purplechaamba.vyajena`
- **Privacy policy URL:** https://purplechaamba.com/vyajena/privacy
- **Support / website URL:** https://purplechaamba.com/vyajena
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

## Age rating answers

- Violence, sexual content, profanity, drugs, gambling, horror: none in the app
  itself.
- User-generated content / users can communicate: **yes** (private group chat
  between invited members; no public content, no strangers).
- Location sharing: no. Purchases: no. Ads: no.
- Expected result: Play "Teen"-or-lower IARC rating; App Store 12+ or 17+
  depending on the unrestricted-communication answer. Accept what the forms
  give.

## Reviewer access (App Review / Play "App access")

Not possible yet: the server is reachable only on the owner's home network
until the relay change. This is why distribution is internal testing only
(proposal: what blocks a public release).

## Screenshots

`app/store/screenshots/` (from `integration_test/screenshots_test.dart` on the
fake core, never a real group): iPhone 6.9" (1320×2868) and an Android phone.
