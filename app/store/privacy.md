# Privacy answers: the one source

app-release: Privacy disclosures match the app (design D10). Play Data safety,
the App Store privacy answers ("nutrition label"), `ios/Runner/PrivacyInfo.xcprivacy`
and `docs/privacy-policy.md` all come from this file. A change that sends new
data off the phone updates this file, the policy and the manifest in the same
change.

The "developer" is the owner, who also runs the server (the server) the app talks
to. Nothing goes to anyone else.

## What leaves the phone

| Data | Leaves the phone as | Goes to | Kept |
|---|---|---|---|
| Message text, group name, display names, "about" text | End-to-end encrypted (MLS, RFC 9420) ciphertext; the server cannot read it | The owner's server | Until every recipient device has it, at most 7 days |
| Username (chosen by the group's admin) | Readable | The owner's server | While the user is in the group |
| Identity and device public keys, device certificates, KeyPackages | Readable (public keys) | The owner's server | While the user or device is in the group |
| IP address | The network connection itself | The owner's server | Not stored (only in a debug log when a TLS handshake fails, off by default) |
| Push token (FCM registration token on Android, APNs device token on iPhone) | Readable | The owner's server, which sends Google (FCM) or Apple (APNs) a wake-up whose only text is "New message": no sender, group or content | While the device is in the group; cleared when notifications are refused or the device is removed |

Nothing else: no contacts, location, photos, files, analytics, crash reports,
advertising ids or tracking. The only third-party SDK is Google's Firebase
Cloud Messaging on Android (no Analytics), which receives the wake-ups; Google
and Apple act as service providers delivering them, so the token is not
"shared" in the stores' sense. All traffic
is TLS 1.3 to a server whose key the invite pins.

## Google Play: Data safety

- **Does your app collect or share any of the required user data types?** Yes.
- **Is all of the user data collected by your app encrypted in transit?** Yes.
- **Do you provide a way for users to request that their data is deleted?** Yes:
  **in-app** — Profile › "Delete account" deletes the account and the data the
  server holds for the user (account-deletion). It is immediate and irreversible
  (type-to-confirm the username): the server removes the user's devices, key
  packages, push tokens, queued messages and uploaded media and frees the
  username, keeping only a minimal removed marker; the device wipes its local
  account, chats, history and keys. Messages other members already received stay
  on their devices (the server holds no history and cannot reach other members'
  copies). The admin account is the one exception: it is reset only by the
  operator on the server host (`server admin-reset`), not from the app. The
  admin can also still remove any user on request.
- **Data types collected:**
  - *Personal info › User IDs* (the username): collected, not shared,
    processed ephemerally: no; required; purpose: App functionality.
  - *Device or other IDs* (device public keys and certificates; the FCM push
    token): collected,
    not shared; required; purpose: App functionality.
- **Messages:** not declared as collected. They leave the phone only end-to-end
  encrypted, and nobody but the sender and recipients can read them; Play's
  guidance excludes end-to-end encrypted data from "collected". Re-check that
  wording at submission.
- **Shared with third parties:** none.

## App Store: privacy answers

- **Do you or your third-party partners collect data from this app?** Yes.
- **Identifiers › User ID:** collected; linked to the user; not used for
  tracking; purpose: App Functionality.
- **Identifiers › Device ID** (device keys and certificates; the APNs push
  token): collected; linked to the user; not used for tracking; purpose: App
  Functionality.
- **User Content › Other User Content / Emails or Text Messages:** not collected
  (end-to-end encrypted; the developer cannot read it).
- **Tracking:** none. `NSPrivacyTracking = false`, no tracking domains.

## Privacy manifest (`PrivacyInfo.xcprivacy`)

- Collected types: `NSPrivacyCollectedDataTypeUserID`,
  `NSPrivacyCollectedDataTypeDeviceID` (linked, not tracking, App Functionality).
- Required-reason APIs (design D9, spike S4), both from the core's SQLite:
  `NSPrivacyAccessedAPICategoryFileTimestamp` (`C617.1`, its own files) and
  `NSPrivacyAccessedAPICategoryDiskSpace` (`E174.1`, before writing files).
