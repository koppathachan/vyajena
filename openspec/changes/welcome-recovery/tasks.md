## 1. Visible reason

- [x] 1.1 App: `ChatModel.refusal` (newest `CommitRefused` with its time, cleared on `JoinedGroup`/`Rejoined`); the waiting screen shows it; widget test `app-ui/join-from-an-invite` ("Welcome refused while waiting")

## 2. Proto and shared limits

- [x] 2.1 `keys.proto`: `Keys.RequestReadd`, `RequestReaddRequest`, `RequestReaddResponse`, `ReaddOutcome`; `buf lint`, `buf breaking`; `cargo run -p pbgen`
- [x] 2.2 `protocol::group_membership`: `MAX_READD_REQUESTS_PER_DAY`, `READD_REQUEST_WINDOW`, `MAX_READD_REASON_BYTES`, `readd_reason`; test `group-membership/re-add-after-an-unusable-welcome`

## 3. Server

- [x] 3.1 In-memory ask limit (`membership::readd::ReaddAsks`, no schema change); `Keys.RequestReadd` checks (member, Welcome queued after the seq, already inactive, limit) and, on ASKED, one transaction: replace KeyPackages, mark inactive + returned, record the ask, log the reason; `RejoinNotice` to connected helpers
- [x] 3.2 server tests `group-membership/re-add-after-an-unusable-welcome`: each outcome, nothing stored on a refusal, KeyPackages replaced, helper gets `RejoinNotice`

## 4. Core

- [x] 4.1 `Job::RequestReadd` after a refused Welcome and once per start with no group (non-admin devices); unused KeyPackages deleted; fresh KeyPackages; `Event::ReaddRequested`; a device in no group ignores `RemovedNotice`
- [x] 4.2 End-to-end test: a Welcome is refused, the device asks, the admin's device re-adds it, it joins and reads a new message; stuck-at-start variant; limit

## 5. Bridge and app

- [x] 5.1 Bridge `CoreEvent::ReaddRequested`; frb regenerated
- [x] 5.2 Waiting screen shows the ask's outcome (provisional text until the owner answers D6); widget test `app-ui/join-from-an-invite` ("Asked to be added again")

## 5b. Fresh keys at signup (D8)

- [x] 5b.1 `keystore::create_fresh`; `register` and `link_request` create new device (and identity) keys, a link request deletes a leftover identity key; tests `device-identity/fresh-keys-for-a-new-signup` (reinstall after removal joins again; link after reinstall)

## 6. Release

- [x] 6.1 `cargo test --workspace`, clippy, fmt, `flutter analyze`, `flutter test`, `openspec validate welcome-recovery --strict`
- [ ] 6.2 Build 1.0.0 (4): `build-ipa.sh`, `build-aab.sh`, `release-check.sh` all PASS (2026-11-16, d59b499); owner uploads
- [ ] 6.3 server deploy (owner yes; `backup.sh` first), before build 4 reaches testers
