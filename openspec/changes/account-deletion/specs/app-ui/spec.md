## ADDED Requirements

### Requirement: Deleting your account
The app SHALL offer a "Delete account" action in the Profile tab that lets a user
delete their own account and data from within the app (Apple Guideline 5.1.1(v);
Google Play account-deletion policy). Tapping it SHALL open an explicit confirmation
that names what is deleted — the account, the user's groups and DMs on this device,
and the server-held key packages, push tokens, queued messages and uploaded media —
and states that the deletion is **irreversible and immediate**, with no deactivation
and no undo. Only an explicit confirming action (not a bare tap or swipe) SHALL
proceed to call the core's `delete_account()`. On success the app SHALL return to the
first-run / onboarding screen after the device wipe (account-deletion: The initiating
device wipes itself on success); on failure it SHALL keep the account and show the
error. The action SHALL be gated on a server that advertises `Devices.DeleteAccount`
(an older server hides it). The client enforces the confirmation and the on-success
navigation.

#### Scenario: Delete account from Profile
- **WHEN** Bob opens Profile, taps "Delete account", reads the confirmation and
  confirms
- **THEN** the app calls `delete_account()`, and on an OK response wipes the device and
  returns to the onboarding screen

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
the admin's device SHALL either hide the action or show it disabled with a short note
that the admin account is reset only by the operator on the server host, so the admin
cannot lock themselves out from inside the app. The client enforces this using its
admin awareness (device-identity: Admin awareness).

#### Scenario: Admin's Profile has no delete
- **WHEN** the admin opens Profile
- **THEN** there is no working "Delete account" action; any shown entry is disabled with
  a note that admin reset is an operator action on the host

#### Scenario: Member's Profile has delete
- **WHEN** an ordinary member opens Profile
- **THEN** the "Delete account" action is present and enabled
