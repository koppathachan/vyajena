## ADDED Requirements

### Requirement: Release triggers
A release workflow SHALL start only from a pushed tag (`server-v*` for
`release-server`, `app-v*` for `release-app`) or from `workflow_dispatch`; it
SHALL NOT have a `pull_request`, `pull_request_target` or `workflow_run`
trigger, and no workflow in the repository SHALL use `pull_request_target`.
GitHub enforces the triggers; `.github/scripts/release-workflows-check.sh`
(run by `release-lint` on every pull request) enforces that the files say so.

#### Scenario: Tag starts a release
- **WHEN** the admin pushes tag `app-v1.0.0+5`
- **THEN** `release-app` starts for that tag and `release-server` does not

#### Scenario: A pull request cannot start a release
- **WHEN** Mallory opens a pull request from a fork that edits `release-app.yml`
- **THEN** no release workflow runs for it, and `release-lint` runs with a
  read-only token and no secrets

#### Scenario: pull_request_target is refused
- **WHEN** a pull request adds a workflow with `on: pull_request_target`
- **THEN** `release-workflows-check.sh` fails and names the file

### Requirement: Owner approval gate
A release job SHALL run in the GitHub Environment `release` when it reads a
release secret, signs with a release key, uploads to a store, creates a GitHub
Release or writes an attestation; that environment requires the owner
(`koppathachan`) as reviewer and accepts only `server-v*` and `app-v*` tags. GitHub enforces the
approval and the tag rule; `release-workflows-check.sh` enforces that every such
job names the environment, and each such job first runs
`release-guard.sh environment release`, which stops the job unless the
environment has the owner as required reviewer, no administrator bypass, and
exactly the tag rules `app-v*` and `server-v*`.

#### Scenario: Upload waits for the owner
- **WHEN** the admin pushes tag `app-v1.0.0+5`
- **THEN** the `android` and `ios` jobs wait for the owner's approval before
  any step runs, and nothing is uploaded until the owner approves

#### Scenario: Environment missing or loose
- **WHEN** the admin pushes `server-v0.2.0` before environment `release`
  exists, so GitHub creates it without protection
- **THEN** the `publish` job's first step, `release-guard.sh environment
  release`, fails because the owner is not a required reviewer, and nothing
  is published

#### Scenario: Branch cannot deploy
- **WHEN** a workflow run on branch `feature` names environment `release`
- **THEN** GitHub refuses the job, because the environment accepts only
  release tags

#### Scenario: Job with secrets outside the environment
- **WHEN** a job in a release workflow uses `secrets.ASC_KEY_ID` without
  `environment: release`
- **THEN** `release-workflows-check.sh` fails and names the job

### Requirement: Release secrets only in the release environment
Release secrets (store keys, signing keys and passwords, API keys) SHALL be
stored only as secrets of the `release` environment, never as repository or
organization secrets, never in the repository, and never printed in a log.
Scripts SHALL write secret files only under the job's `RUNNER_TEMP` with mode
0600 and delete them when the job ends. Jobs outside the environment SHALL
receive no release secret. A step that builds (and so runs third-party code)
SHALL NOT receive a store upload credential: it cannot request the job's OIDC
token, and the App Store Connect key reaches only the upload step. GitHub
enforces the scoping; the release scripts enforce the file handling;
`release-workflows-check.sh` enforces the step rules; the owner creates the
secrets.

#### Scenario: Build code cannot unlock Play
- **WHEN** a malicious crate's build script in the `android` job's build step
  tries to read `ACTIONS_ID_TOKEN_REQUEST_TOKEN`
- **THEN** the variable is not set in that step, so it cannot get the token
  that workload identity exchanges for Play access

#### Scenario: Dry run has no secrets
- **WHEN** the admin starts `release-app` with `workflow_dispatch`
- **THEN** the dry-run jobs run without environment `release` and every
  `secrets.*` they could read is empty

#### Scenario: Key files are temporary
- **WHEN** the `android` job finishes, whether it passed or failed
- **THEN** the decoded upload keystore and Firebase file under `RUNNER_TEMP`
  are deleted

### Requirement: Actions pinned to commit SHAs
Every `uses:` of an action outside this repository in a release workflow SHALL
name a full 40-character commit SHA, followed by a comment naming the version.
`release-guard.sh` enforces it at the start of every release run, and
`release-workflows-check.sh` on every pull request.

#### Scenario: Unpinned action
- **WHEN** `release-app.yml` says `uses: actions/checkout@v7`
- **THEN** the guard fails before any build and names the line

#### Scenario: Pinned action
- **WHEN** it says `uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1`
- **THEN** the pin check passes

### Requirement: Release tag matches the repository
Before any build, the release guard SHALL check that the pushed tag's commit is
on `origin/main` and that the tag matches the version in the repository:
`app-v<version>` equal to `app/pubspec.yaml` `version:` (`<name>+<build>`)
with the build number above that of every other `app-v*` tag, and
`server-v<version>` equal to `server/Cargo.toml` `version`. A mismatch SHALL
fail the run with nothing built or uploaded. `release-guard.sh` enforces this.

#### Scenario: App tag matches pubspec
- **WHEN** pubspec says `1.0.0+5`, the highest earlier tag is `app-v1.0.0+4`,
  and the admin pushes `app-v1.0.0+5` on a commit of `main`
- **THEN** the guard passes

#### Scenario: App tag does not match pubspec
- **WHEN** pubspec says `1.0.0+5` and the admin pushes `app-v1.0.0+6`
- **THEN** the guard fails, naming both versions

#### Scenario: Build number not raised
- **WHEN** pubspec says `1.0.0+4`, tag `app-v1.0.0+4` already exists, and the
  admin pushes `app-v1.0.1+4`
- **THEN** the guard fails because build 4 is not above the last build 4

#### Scenario: Tag off main
- **WHEN** Mallory pushes `server-v0.1.0` on a commit that is not on `main`
- **THEN** the guard fails and nothing is built

#### Scenario: Server tag matches Cargo.toml
- **WHEN** `server/Cargo.toml` says `version = "0.2.0"` and the tag is
  `server-v0.2.0`
- **THEN** the guard passes

### Requirement: Server release files
On tag `server-v<version>`, `release-server` SHALL build the server and
rathole for linux/arm64 with `deploy/server/build.sh`, and, after approval, publish
a GitHub Release named after the tag with exactly three files:
`vyajena-server`, `rathole` and `SHA256SUMS`, plus a build provenance
attestation for both binaries. The files SHALL hold no configuration, pin,
invite, key or address of the owner's server; those are written on the server by
`deploy/server/deploy.sh`. The workflow enforces the file list; the owner verifies
the attestation before deploying.

#### Scenario: Server release published
- **WHEN** the owner approves the `publish` job for `server-v0.2.0`
- **THEN** GitHub Release `server-v0.2.0` has `vyajena-server`, `rathole` and
  `SHA256SUMS`, and `gh attestation verify vyajena-server --repo koppathachan/vyajena`
  succeeds

#### Scenario: Unexpected file
- **WHEN** the build leaves a fourth file in the release folder
- **THEN** the build job fails and nothing is published

### Requirement: Android release to internal testing
On tag `app-v<version>`, after approval, `release-app` SHALL build the App
Bundle signed with the owner's upload key, run
`app/store/release-check.sh --android-only` (every line PASS), and upload the
bundle to the Play **internal** track only, with the release completed for
internal testers. It SHALL NOT upload to any other track. Play access SHALL use
a service account reached by workload identity federation from this
repository's `release` environment, with no long-lived Google key.
`release-app.yml` and `.github/scripts/play-upload.sh` enforce the track; Play
App Signing keeps the app signing key with Google.

#### Scenario: Internal release
- **WHEN** the owner approves the `android` job for `app-v1.0.0+5`
- **THEN** version code 5 is on the Play internal track for the internal
  testers, and no other track changes

#### Scenario: Release check fails
- **WHEN** the bundle lacks `lib/armeabi-v7a/libbridge.so`
- **THEN** the release check fails and nothing is uploaded

### Requirement: iOS release to TestFlight
On tag `app-v<version>`, after approval, `release-app` SHALL build the IPA on a
macOS runner, signed with the owner's Apple Distribution certificate and App
Store profile, check that the app has `aps-environment = production`, run
`app/store/release-check.sh --ios-only` (every line PASS), and upload it to App
Store Connect for TestFlight with an App Store Connect API key. The
certificate, profile and key SHALL live only in a temporary keychain and
`RUNNER_TEMP` for the job. `.github/scripts/ios-ipa.sh` enforces the checks.

#### Scenario: TestFlight upload
- **WHEN** the owner approves the `ios` job for `app-v1.0.0+5`
- **THEN** build 5 of 1.0.0 is uploaded to App Store Connect and the job's
  temporary keychain is deleted

#### Scenario: Push entitlement lost
- **WHEN** the exported app's entitlements lack `aps-environment = production`
- **THEN** the job fails before upload

### Requirement: Dry run builds without secrets
`workflow_dispatch` of `release-server` or `release-app` SHALL run the guard
(without the tag checks) and build everything the tag run builds, with no
environment, no secret and no upload: the Android bundle signed with a
throwaway key made in the job (checked against that key's fingerprint), the iOS
archive unsigned, and the server files kept only as a workflow artifact for at
most 3 days. The workflow files enforce this; `release-workflows-check.sh`
checks that dry-run jobs name no environment and no secret.

#### Scenario: Dry run before secrets exist
- **WHEN** the admin starts `release-app` from the Actions tab on `main` with
  no secret created yet
- **THEN** `android-dry-run` builds and checks a throwaway-signed bundle,
  `ios-dry-run` builds and checks an unsigned archive, and nothing is uploaded

### Requirement: server deploys stay manual
No workflow SHALL deploy to, connect to or hold credentials for the server or the
relay: no SSH key, relay secret, invite or pin is a secret of any environment.
The owner deploys with `deploy/server/deploy.sh`, as today.
`release-workflows-check.sh` enforces that no workflow runs `ssh`, `scp` or
`deploy/server/deploy.sh`.

#### Scenario: Server release does not touch the server
- **WHEN** GitHub Release `server-v0.2.0` is published
- **THEN** the server still runs its previous build until the owner runs
  `deploy/server/deploy.sh`
