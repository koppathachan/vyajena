## Why

Releases are manual today (change `app-release`, `deploy/server/build.sh`): the
owner builds the Android bundle and the iOS archive on the Mac, uploads them by
hand, and builds the server in a local container. That is slow, depends on
one Mac's state, and leaves no public record of what was built from which
commit. The repo is public (koppathachan/vyajena), so release builds can come
from GitHub Actions, where anyone can see the source, the workflow and the
build's provenance.

The real risk is not the build: it is a taken-over GitHub account (or a leaked
secret) shipping an update to the group's phones. So every job that signs,
uploads or publishes waits for the owner's approval in a GitHub Environment
that alone holds the secrets.

## What Changes

- **Server release:** pushing tag `server-v<version>` builds the arm64 server
  Server and rathole with `deploy/server/build.sh` on an arm64 runner, and, after
  the owner approves, publishes them as GitHub Release files with
  `SHA256SUMS` and a build provenance attestation. The files hold no config,
  pin, invite or key; `deploy/server/deploy.sh` fills those in on the server, as today.
- **App release:** pushing tag `app-v<name>+<build>` builds, after the owner
  approves, the signed Android App Bundle and uploads it to the Play
  **internal** track, and the signed IPA on a macOS runner and uploads it to
  **TestFlight**. Both run `app/store/release-check.sh` before any upload.
- **Approval:** every signing, uploading or publishing job runs in the GitHub
  Environment `release` (required reviewer: the owner; only `server-v*` and
  `app-v*` tags may deploy to it). The secrets live only there.
- **Guard:** before anything is built, `.github/scripts/release-guard.sh`
  checks that the tag matches the version in the repo, that its commit is on
  `main`, that the app build number is above every earlier one, and that every
  action in the release workflows is pinned to a full commit SHA.
- **Dry run:** `workflow_dispatch` on either workflow builds everything with no
  environment, no secrets and no upload (Android signed with a throwaway key),
  so the workflows can be tested before any secret exists.
- **Pull requests never reach release secrets:** the release workflows have no
  `pull_request` trigger and nothing uses `pull_request_target`. A new
  `release-lint` workflow checks the release workflows and scripts on pull
  requests, with read-only permissions and no secrets.
- **Server deploys stay manual** (`deploy/server/`). Letting `deploy.sh` fetch and
  verify a release file is an open item in `design.md`, not built here.
- `docs/ci-release.md`: the owner's step-by-step (secrets, roles, environment,
  cutting a release, undoing a leak).

No `.proto` change. No change to `app/` or `deploy/`. `ci.yml` only gains a
"what changed" job so each check runs when its code changed (design D16).

### Open decisions this change resolves

None from `docs/architecture-decisions.md`. It needs two owner answers,
recorded in `design.md` "Questions for the owner": Q1 (this change reverses
`app-release`'s "no key is given to CI"; how to record that) and Q2 (Play
access by workload identity, not a JSON key).

## Capabilities

### New Capabilities

- `release-automation`: what the release workflows do, what they may touch,
  who approves, how versions and tags are checked, and the dry run.

### Modified Capabilities

None in `openspec/specs/`. The unarchived change `app-release` has the
requirement "Release signing keys stay with the owner", which says no key is
given to CI; this change contradicts it (design D2, Q1).

## Impact

- New: `.github/workflows/release-server.yml`, `release-app.yml`,
  `release-lint.yml`; `.github/scripts/` (guard, Android, iOS, Play upload,
  tests); `docs/ci-release.md`.
- Owner-only setup: the `release` environment, tag rulesets, secrets, the
  Play service account's workload identity binding, an exportable Apple
  Distribution certificate and an App Store profile, an App Store Connect API
  key (`docs/ci-release.md`).
- Unchanged: `ci.yml`, the manual release path (`app/store/*.sh`,
  `deploy/server/build.sh`), the server and the relay.
