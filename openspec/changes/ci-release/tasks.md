No `.proto` changes, no invariant changes. Groups 1–4 are for a session;
group 5 is the owner's alone (GitHub settings, secrets, keys, accounts,
pushes). Nothing in `ci.yml`, `app/`, `deploy/` or the active changes' files
changes (design D2, D14).

## 1. Guard and checks

- [x] 1.1 `.github/scripts/release-guard.sh`: `pins`, `tag` (on `origin/main`, HEAD is the tag, version = pubspec / server Cargo.toml, app build above earlier tags), `version` (D5, D6) (release-automation: Actions pinned to commit SHAs; Release tag matches the repository)
- [x] 1.2 `.github/scripts/release-workflows-check.sh`: triggers, `permissions: {}`, environment-only secrets and writes, tag-only environment jobs, no caches, `persist-credentials: false`, no ssh/scp/deploy.sh, pins (D3, D9–D11) (release-automation: Release triggers; Owner approval gate; Pi deploys stay manual)
- [x] 1.3 `.github/scripts/release-scripts-test.sh`: fixtures for every rule and guard case, each tagged `# spec: release-automation/<slug>`

## 2. Build and upload scripts

- [x] 2.1 `.github/scripts/android-aab.sh dry-run|release`: keys in `RUNNER_TEMP` (0600, deleted on exit), Gradle `-P` properties, release check against the recorded or throwaway fingerprint (D7, D9) (release-automation: Android release to internal testing; Release secrets only in the release environment)
- [x] 2.2 `.github/scripts/play-upload.sh`: Android Publisher REST, internal track fixed, version code checked, failed edit deleted, token in a header file (D7)
- [x] 2.3 `.github/scripts/ios-ipa.sh dry-run|release`: temp keychain, profile checks, manual export, push entitlement check, release check, altool upload with the API key (D8) (release-automation: iOS release to TestFlight)

## 3. Workflows

- [x] 3.1 `.github/workflows/release-server.yml`: arm64 build with `deploy/pi/build.sh`, three files, `publish` in `release` with provenance and `gh release create` (D4) (release-automation: Server release files)
- [x] 3.2 `.github/workflows/release-app.yml`: guard, dry-run jobs, `android` and `ios` in `release` (D7–D9) (release-automation: Dry run builds without secrets)
- [x] 3.3 `.github/workflows/release-lint.yml` on pull requests; `.github/actionlint.yaml` for the `xcode-27` label (D11)
- [x] 3.4 Every action pinned to a full SHA of a release at least two weeks old (D10 table)

## 4. Docs and local checks

- [x] 4.1 `docs/ci-release.md`: secrets, roles, environment, ruleset, WIF condition, cutting a release, dry run, undo per threat (D12, D13, D14)
- [x] 4.2 `openspec validate ci-release --strict`, `actionlint`, `shellcheck .github/scripts/*.sh`, `release-scripts-test.sh`, `cargo build --workspace`, `cd app && flutter analyze` all pass (results in design.md "Verification")
- [x] 4.3 Hardening (D15): environment check first in every release job; build steps get no OIDC token; App Store Connect key only in the upload step; checker rules + tests (48)
- [x] 4.4 `ci.yml` runs each check only when its code changed (D16, owner 2026-11-17): `what changed` job + `ci-changes.sh`, job names unchanged; 10 tests

## 5. Owner only (in order, `docs/ci-release.md`)

- [ ] 5.1 Answer Q1–Q3 in design.md
- [ ] 5.2 Push branch `ci-release`, open the PR, merge after green CI (the owner says yes to the exact commits first)
- [ ] 5.3 Environment `release` (reviewer, tag rules, no admin bypass); tag ruleset for `server-v*` and `app-v*`; fork PR approval setting
- [ ] 5.4 Dry runs: Actions › release-app › Run workflow (main); Actions › release-server › Run workflow (main); both green
- [ ] 5.5 Android secrets; Play service account + workload identity condition; Play permissions for testing tracks only
- [ ] 5.6 Apple Distribution certificate (.p12), App Store profile with Push, App Store Connect API key (Developer); iOS secrets
- [ ] 5.7 First server release: bump `server/Cargo.toml`, tag `server-v…`, approve, verify sums + attestation
- [ ] 5.8 First app release: raise the build number, tag `app-v…`, approve; check Play internal and TestFlight on a phone
- [ ] 5.9 Resolve Q1 (app-release requirement) before archiving either change; then archive `ci-release`
