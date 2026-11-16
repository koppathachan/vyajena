## Why

A new member's iPhone (TestFlight 1.0.0 (3), 2026-11-16 21:24) is stuck on
"Waiting for the admin's device to add you". The admin's iPhone committed the
add with a Welcome (seq 31, epoch 3 → 4), the Pi counts the device as a member
and its cursor acked everything, but the device never joined and never sent
anything.

In the core, any Welcome the device cannot use (`StagedWelcome` build error,
or a leaf that fails the identity checks) is logged, reported as
`CommitRefused` and ACKed. The app shows that only in the chat, which a
device in no group never reaches, so the reason is invisible. And because
the Pi counts the device as a member, it never announces it again with a
`JoinNotice`: the device is stuck for good, and only a wipe and a new invite
get it out.

## What Changes

- **Visible reason (app):** the waiting screen shows the refusal's text and
  time, so the tester can tell the admin what went wrong.
- **Recovery (proto, Pi, core):** a device that has an account but no group,
  while the Pi counts it as a member, asks the Pi to be added again. The Pi
  checks that it really is stuck and treats it like a returning inactive
  device: its leaf gets a Pi Remove proposal, and the devices that may re-add
  it get a `RejoinNotice`. One of them commits Remove + Add from a fresh
  KeyPackage (the existing `Readd` job). The stuck device replaces its
  KeyPackages on the Pi first, so the re-add cannot pick an old one.
- **Limits:** the Pi accepts a bounded number of such requests per device per
  day; the device asks once per refused Welcome and once per start.
- **Bridge and app:** the waiting screen says, in the owner's words, that the
  phone could not open the invitation, asked to be added again, and why.
- **Fresh keys at signup (core):** a reinstalled iPhone (the Keychain keeps
  items after the app is deleted) re-registers with new keys, not the revoked
  device id. That is what broke the owner's remove-and-re-invite of the tester.
- Details, decisions and the security reasoning: `design.md`.

## Open decisions resolved

- Re-add: automatic and silent, no admin tap (owner, 2026-11-16; design D6).
- Trigger: automatic on the stuck device, no button (owner, 2026-11-16).
- Waiting-screen text and the 3-a-day limit (owner, 2026-11-16).

## Proto files touched

- `proto/vyajena/v1/membership.proto` (or the service that owns
  `JoinNotice`): a new RPC for a stuck device to ask for a re-add.
