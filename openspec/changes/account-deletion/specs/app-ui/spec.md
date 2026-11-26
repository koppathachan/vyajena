## ADDED Requirements

### Requirement: Deleting your account
The app SHALL offer a "Delete account" action in the Profile tab that lets a user
delete their own account and data from within the app (Apple Guideline 5.1.1(v);
Google Play account-deletion policy). Tapping it SHALL open an explicit confirmation
that names what is deleted — the account, the user's groups and DMs on this device,
and the server-held key packages, push tokens, queued messages and uploaded media —
and states that the deletion is **irreversible and immediate**, with no deactivation,
no cancellable grace period and no undo. Because the action is irreversible, the
confirmation SHALL require the user to **type their own username** to confirm: the
confirm control SHALL stay disabled until the typed text matches the user's username,
so no single tap or swipe can delete the account. Only that explicit typed
confirmation SHALL proceed to call the core's `delete_account()`. On success the app SHALL return to the
first-run / onboarding screen after the device wipe (account-deletion: The initiating
device wipes itself on success); on failure it SHALL keep the account and show the
error. Against a server too old to implement `Devices.DeleteAccount` the call fails
and the app shows that error rather than deleting (no new capability flag is added to
the wire; design D7). The client enforces the confirmation and the on-success
navigation.

#### Scenario: Delete account from Profile
- **WHEN** Bob opens Profile, taps "Delete account", reads the confirmation, types his
  username `bob` and confirms
- **THEN** the app calls `delete_account()`, and on an OK response wipes the device and
  returns to the onboarding screen

#### Scenario: Type-to-confirm gates the delete
- **WHEN** Bob taps "Delete account" but has not typed his username, or typed something
  that does not match it
- **THEN** the confirm control stays disabled and the app calls nothing

#### Scenario: Confirmation is required
- **WHEN** Bob taps "Delete account" but dismisses the confirmation without confirming
- **THEN** the app calls nothing and Bob's account is unchanged

#### Scenario: Failure keeps the account
- **WHEN** Bob confirms deletion but the core returns an error
- **THEN** the app keeps Bob signed in to his account and shows the error, without
  wiping local state

### Requirement: The admin sees no self-delete
The app SHALL NOT offer a working "Delete account" action to the admin's device
(account-deletion: The admin cannot self-delete through the RPC). The Profile tab on
the admin's device SHALL show the "Delete account" entry **greyed-out** with a short,
honest note — *"The admin account can't be deleted from the app. To reset it, run
`server admin-reset` on the server host (see admin-recovery)."* — so the admin cannot
lock themselves out from inside the app. The client enforces this using its admin
awareness (device-identity: Admin awareness).

#### Scenario: Admin's Profile has no delete
- **WHEN** the admin opens Profile
- **THEN** the "Delete account" entry is disabled (greyed-out) with the note that the
  admin account can't be deleted from the app and is reset by running `server
  admin-reset` on the host, and tapping it does nothing

#### Scenario: Member's Profile has delete
- **WHEN** an ordinary member opens Profile
- **THEN** the "Delete account" action is present and enabled
