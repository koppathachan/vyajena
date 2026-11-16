# Vyajena privacy policy

<!--
  Source of the page published at https://www.purplechaamba.com/vyajena/privacy/
  (repo chaamba/purple-www, vyajena/privacy/index.html; GitHub Pages).
  It says what app/store/privacy.md says; change all three together.
-->

*Last updated: 2026-11-16*

Vyajena is an end-to-end encrypted, text-only chat for one private group. It is
made and run by Akhil Sasidharan (purplechaamba.com), who also runs the group's server.
There are no ads, no analytics and no tracking.

## What leaves your phone

- **Your messages, the group's name, display names and "about" texts** leave
  your phone only end-to-end encrypted. The server stores the encrypted copy
  until every member's device has received it, for at most 7 days, and cannot
  read it. Only the group's members can.
- **Your username**, chosen by the group's admin when you were invited, is
  stored on the group's server while you are in the group.
- **Public keys and device certificates** for you and each of your devices are
  stored on the server while you or that device are in the group, so other
  members can check who they are talking to. Your private keys never leave your
  phone; they stay in the phone's key store (Android Keystore, iOS Keychain).
- **Your IP address** reaches the server because your phone connects to it.
  The server does not store it.
- **A push token** for each of your devices (from Google on Android, from
  Apple on iPhone) is stored on the server while that device is in the group,
  so it can wake the app when a message arrives. The server then asks Google
  or Apple to show a notification whose only text is "New message": they
  learn that your phone was woken, never who wrote, in which group, or what.
  If you turn notifications off, the app removes the token.

Everything travels over TLS 1.3 to the group's own server. Nothing is sold,
shared with anyone else, or used for advertising or tracking. The app contains
no third-party analytics, advertising or crash-reporting code. On Android it
includes Google's Firebase Cloud Messaging, only to receive those wake-ups.

## On your phone

Messages are kept in the app's private storage on your phone, excluded from
cloud backups. Deleting the app deletes them and your keys.

## Removing your account

Ask the group's admin to remove you: that deletes your username, your devices,
their keys and anything still waiting for you on the server. You can also
write to privacy@purplechaamba.com.

## Children

Vyajena is for members of one private group, invited by its admin. It is not
directed at children.

## Changes

If what the app sends changes, this page changes first, with a new date.

## Contact

privacy@purplechaamba.com
