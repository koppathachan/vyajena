## ADDED Requirements

### Requirement: Re-invite action
The app SHALL offer, on the admin's devices only, a one-tap "Re-invite `<name>`" action
on the admin members screen, alongside the existing Remove action, that re-onboards an
existing user under the same username in one step by calling `Admin.CreateInvite` with
that same username and showing the resulting invite text to share, with no separate
remove step. The app SHALL gate the action behind a confirm dialog that names the person
and states that re-inviting resets the person's devices (logging out any device they
still have) and that the person keeps the same username but loses their on-device
history; the app SHALL call `Admin.CreateInvite` only after the admin confirms. The
members screen SHOULD show a per-user activity hint (for example "no active device" or a
last-seen indication) so the admin can tell who has likely lost access and act on the
user rather than hand-managing MLS leaves. Other devices SHALL NOT show the action or the
admin members screen; the server rejects their admin RPCs anyway. The app enforces the
confirm gate.

#### Scenario: Admin re-invites a member who lost their phone
- **WHEN** the admin opens the members screen, sees Bob flagged as having no active
  device, and taps "Re-invite Bob"
- **THEN** the app shows a confirm dialog warning that this resets Bob's devices and
  that Bob keeps the username but loses on-device history, and does nothing until the
  admin confirms

#### Scenario: Confirming shows a shareable code
- **WHEN** the admin confirms the re-invite for Bob
- **THEN** the core calls `Admin.CreateInvite` for `bob` and the app shows the invite
  text with a share button, with no separate remove step

#### Scenario: Cancelling changes nothing
- **WHEN** the admin dismisses the re-invite confirm dialog
- **THEN** the app calls no RPC and Bob's account and devices are untouched

#### Scenario: Non-admin sees no re-invite action
- **WHEN** a non-admin opens the members screen
- **THEN** no admin members actions are shown, "Re-invite" among them
