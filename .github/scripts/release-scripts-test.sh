#!/usr/bin/env bash
# Tests for the release scripts and the workflow checker (ci-release tasks
# 2.x, 3.x). Local and in release-lint; needs git, ruby, bash. Builds nothing,
# reads no secret, talks to no service.
#
#   .github/scripts/release-scripts-test.sh
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
guard="$here/release-guard.sh"
check="$here/release-workflows-check.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
passed=0
failed=0

ok() { passed=$((passed + 1)); echo "ok    $1"; }
bad() { failed=$((failed + 1)); echo "FAIL  $1"; }

# expect_pass <name> <cmd...> / expect_fail <name> <needle> <cmd...>
expect_pass() {
  local name="$1"; shift
  if out="$("$@" 2>&1)"; then ok "$name"; else bad "$name"; printf "%s\n" "$out" | sed 's/^/      /'; fi
}
expect_fail() {
  local name="$1" needle="$2"; shift 2
  if out="$("$@" 2>&1)"; then
    bad "$name (passed, expected a failure)"
  elif [[ "$out" == *"$needle"* ]]; then
    ok "$name"
  else
    bad "$name (failed without '$needle')"; printf "%s\n" "$out" | sed 's/^/      /'
  fi
}

pin="3d3c42e5aac5ba805825da76410c181273ba90b1"

# A minimal good release workflow, to break one rule at a time.
good_workflow() {
  cat <<YML
name: release-x
on:
  push:
    tags: ["app-v*"]
  workflow_dispatch:
permissions: {}
jobs:
  dry:
    if: github.event_name == 'workflow_dispatch'
    runs-on: ubuntu-latest
    permissions:
      contents: read
    steps:
      - uses: actions/checkout@$pin # v7.0.1
        with:
          persist-credentials: false
      - run: echo dry
  real:
    if: github.event_name == 'push' && startsWith(github.ref, 'refs/tags/app-v')
    runs-on: ubuntu-latest
    environment: release
    permissions:
      contents: write
    steps:
      - run: .github/scripts/release-guard.sh environment release
      - run: echo "\${{ secrets.SOME_KEY }}"
YML
}

fixture() { # fixture <name> <file content on stdin> -> dir
  local d="$tmp/wf-$1"
  mkdir -p "$d"
  cat >"$d/${2:-release-x.yml}"
  echo "$d"
}

# --- release-automation/release-triggers --------------------------------------
# spec: release-automation/release-triggers
expect_pass "real workflows pass the checker" "$check" "$repo/.github/workflows"
d="$(good_workflow | fixture good)"
expect_pass "good fixture passes" "$check" "$d"
# spec: release-automation/release-triggers
d="$(printf 'on: pull_request_target\npermissions: {}\njobs: {}\n' | fixture prt other.yml)"
expect_fail "pull_request_target is refused" "other.yml: uses pull_request_target" "$check" "$d"
# spec: release-automation/release-triggers
d="$(good_workflow | sed 's/  workflow_dispatch:/  workflow_dispatch:\n  pull_request:/' | fixture pr)"
expect_fail "release workflow with pull_request is refused" "triggers other than" "$check" "$d"
d="$(good_workflow | sed 's/    tags: \["app-v\*"\]/    branches: [main]/' | fixture branch)"
expect_fail "release workflow on a branch push is refused" "push trigger must have tags only" "$check" "$d"

# --- release-automation/owner-approval-gate -----------------------------------
# spec: release-automation/owner-approval-gate
d="$(good_workflow | sed '/environment: release/d' | fixture noenv)"
expect_fail "job with secrets outside the environment" "job real uses secrets SOME_KEY without environment release" "$check" "$d"
# spec: release-automation/owner-approval-gate
d="$(good_workflow | sed "s/    if: github.event_name == 'push'.*/    if: always()/" | fixture anyref)"
expect_fail "environment job must run only on a tag push" "job real (environment release) must run only on a tag push" "$check" "$d"
# spec: release-automation/owner-approval-gate
d="$(good_workflow | awk '!d && /      contents: read/ { sub(/read/, "write"); d = 1 } 1' | fixture drywrite)"
expect_fail "write permission outside the environment" "job dry asks for contents: write" "$check" "$d"

# --- release-automation/release-secrets-only-in-the-release-environment -------
# spec: release-automation/release-secrets-only-in-the-release-environment
d="$(good_workflow | sed 's/      - run: echo dry/      - run: echo "${{ secrets.X }}"/' | fixture drysecret)"
expect_fail "dry-run job cannot read a secret" "job dry uses secrets X" "$check" "$d"
# spec: release-automation/release-secrets-only-in-the-release-environment
mkdir -p "$tmp/runner"
expect_fail "android release refuses to run without its secrets" "ANDROID_UPLOAD_KEYSTORE_B64 is not set" \
  env -i PATH="$PATH" HOME="$HOME" RUNNER_TEMP="$tmp/runner" bash -c "cd '$repo' && '$here/android-aab.sh' release"
if [ -z "$(ls -A "$tmp/runner")" ]; then ok "android key folder is deleted on exit"; else bad "android key folder left in RUNNER_TEMP"; fi
# spec: release-automation/ios-release-to-testflight
expect_fail "ios release refuses to run without its secrets" "IOS_DIST_CERT_P12_B64 is not set" \
  env -i PATH="$PATH" HOME="$HOME" RUNNER_TEMP="$tmp/runner" bash -c "cd '$repo' && '$here/ios-ipa.sh' release"
d="$(good_workflow | sed 's/          persist-credentials: false/          fetch-depth: 0/' | fixture persist)"
expect_fail "checkout must not keep the token" "checkout lacks persist-credentials: false" "$check" "$d"
d="$(good_workflow | sed "s|      - run: echo dry|      - uses: actions/cache@$pin # v4.0.0|" | fixture cache)"
expect_fail "release jobs use no cache" "uses a cache" "$check" "$d"

# --- release-automation/actions-pinned-to-commit-shas -------------------------
# spec: release-automation/actions-pinned-to-commit-shas
printf '    steps:\n      - uses: actions/checkout@v7\n' >"$tmp/unpinned.yml"
expect_fail "unpinned action" "unpinned.yml:2: actions/checkout@v7" "$guard" pins "$tmp/unpinned.yml"
printf '      - uses: actions/checkout@%s # v7.0.1\n      - uses: ./.github/actions/local\n' "$pin" >"$tmp/pinned.yml"
expect_pass "pinned action (and a local action) pass" "$guard" pins "$tmp/pinned.yml"
printf '      - uses: actions/checkout@%s\n' "$pin" >"$tmp/nocomment.yml"
expect_fail "pinned action without a version comment" "nocomment.yml:1" "$guard" pins "$tmp/nocomment.yml"
printf '      - uses: "actions/checkout@%s" # v7.0.1\n' "${pin:0:12}" >"$tmp/short.yml"
expect_fail "short SHA is refused" "short.yml:1" "$guard" pins "$tmp/short.yml"
d="$(good_workflow | sed "s|actions/checkout@$pin # v7.0.1|actions/checkout@v7|" | fixture unpinned)"
expect_fail "checker runs the pin check" "actions/checkout@v7" "$check" "$d"

# --- release-automation/release-tag-matches-the-repository --------------------
# A throwaway repo. main: "one" (1.0.0+4, server 0.1.0, tag app-v1.0.0+4) then
# "two" (1.0.0+5, server 0.2.0). Branch low: "two" + pubspec 1.0.1+4. Branch
# feature: "two" + one commit not on main.
r="$tmp/repo"
mkdir -p "$r/app" "$r/server"
git -C "$r" init -q -b main
git -C "$r" config user.email t@example.invalid
git -C "$r" config user.name test
git -C "$r" config commit.gpgsign false
git -C "$r" config tag.gpgsign false
printf "name: app\nversion: 1.0.0+4\n" >"$r/app/pubspec.yaml"
printf "[package]\nname = \"server\"\nversion = \"0.1.0\"\n\n[dependencies]\nfoo = { version = \"9.9.9\" }\n" >"$r/server/Cargo.toml"
git -C "$r" add -A && git -C "$r" commit -qm one
git -C "$r" tag app-v1.0.0+4
printf "name: app\nversion: 1.0.0+5\n" >"$r/app/pubspec.yaml"
printf "[package]\nname = \"server\"\nversion = \"0.2.0\"\n" >"$r/server/Cargo.toml"
git -C "$r" commit -qam two
git -C "$r" checkout -q -b low
printf "name: app\nversion: 1.0.1+4\n" >"$r/app/pubspec.yaml"
git -C "$r" commit -qam low
git -C "$r" checkout -q -b feature main
echo x >"$r/x" && git -C "$r" add x && git -C "$r" commit -qm off-main
git -C "$r" checkout -q main

# guard_case <commit> <tag> [main ref]: only app-v1.0.0+4 and <tag> exist.
guard_case() {
  (
    cd "$r"
    for t in $(git tag -l); do [ "$t" = app-v1.0.0+4 ] || git tag -d "$t" >/dev/null; done
    git tag "$2" "$1"
    git checkout -q "$2"
    MAIN_REF="${3:-main}" "$guard" tag "$2"
  )
}

# spec: release-automation/release-tag-matches-the-repository
expect_pass "app tag matches pubspec, build above earlier" guard_case main app-v1.0.0+5
# spec: release-automation/release-tag-matches-the-repository
expect_fail "app tag does not match pubspec" "app-v1.0.0+6 does not match app/pubspec.yaml version 1.0.0+5" guard_case main app-v1.0.0+6
# spec: release-automation/release-tag-matches-the-repository
expect_fail "build number not raised" "build 4 is not above the last released build 4" guard_case low app-v1.0.1+4 low
# spec: release-automation/release-tag-matches-the-repository
expect_fail "tag off main" "is not on main" guard_case feature server-v0.2.0
expect_fail "app tag off main" "is not on main" guard_case feature app-v1.0.0+5
# spec: release-automation/release-tag-matches-the-repository
expect_pass "server tag matches Cargo.toml" guard_case main server-v0.2.0
expect_fail "server tag does not match Cargo.toml" "does not match server/Cargo.toml version 0.2.0" guard_case main server-v0.3.0
expect_fail "other tag names are refused" "neither app-v* nor server-v*" guard_case main v1
expect_fail "HEAD must be the tag's commit" "HEAD is not the commit of app-v1.0.0+4" \
  bash -c "cd '$r' && git checkout -q main && MAIN_REF=main '$guard' tag app-v1.0.0+4"
expect_pass "version app" bash -c "cd '$r' && git checkout -q main && test \"\$('$guard' version app)\" = 1.0.0+5"
expect_pass "version server" bash -c "cd '$r' && test \"\$('$guard' version server)\" = 0.2.0"

# --- release-automation/server-release-files ----------------------------------
# spec: release-automation/server-release-files
wf="$repo/.github/workflows/release-server.yml"
if grep -q 'deploy/pi/build.sh' "$wf" && [ "$(grep -c '= "SHA256SUMS rathole vyajena-server "' "$wf")" = 2 ] &&
  grep -q 'actions/attest-build-provenance@' "$wf"; then
  ok "server release: build.sh, three files checked twice, provenance"
else
  bad "server release: build.sh, file list or provenance missing"
fi

# --- release-automation/android-release-to-internal-testing -------------------
# spec: release-automation/android-release-to-internal-testing
up="$here/play-upload.sh"
if grep -qx 'readonly TRACK="internal"' "$up" && ! grep -Eq '(production|alpha|beta)' "$up" &&
  [ "$(grep -o 'tracks/[^"]*' "$up" | sort -u)" = "tracks/\$TRACK" ]; then
  ok "play upload reaches only the internal track"
else
  bad "play upload names a track other than internal"
fi
expect_fail "play upload needs a token" "PLAY_ACCESS_TOKEN is not set" \
  env -i PATH="$PATH" bash -c "cd '$repo' && touch '$tmp/a.aab' && '$up' '$tmp/a.aab'"
if grep -q 'release-check.sh" --android-only' "$here/android-aab.sh"; then ok "android runs the release check"; else bad "android skips the release check"; fi
if grep -q 'release-check.sh" --ios-only' "$here/ios-ipa.sh"; then ok "ios runs the release check"; else bad "ios skips the release check"; fi

# --- release-automation/dry-run-builds-without-secrets ------------------------
# spec: release-automation/dry-run-builds-without-secrets
if ruby -ryaml -e '
  %w[release-app.yml release-server.yml].each do |f|
    d = YAML.safe_load(File.read(File.join(ARGV[0], f)))
    on = d["on"] || d[true]
    abort "#{f}: no workflow_dispatch" unless on.key?("workflow_dispatch")
  end
  d = YAML.safe_load(File.read(File.join(ARGV[0], "release-app.yml")))
  %w[android-dry-run ios-dry-run].each do |j|
    job = d["jobs"][j] or abort "no job #{j}"
    abort "#{j} has an environment" if job.key?("environment")
    abort "#{j} not dispatch-only" unless job["if"] == "github.event_name == '"'"'workflow_dispatch'"'"'"
  end' "$repo/.github/workflows"; then ok "dry-run jobs exist, dispatch-only, no environment"; else bad "dry-run jobs"; fi

# --- release-automation/pi-deploys-stay-manual --------------------------------
# spec: release-automation/pi-deploys-stay-manual
d="$(good_workflow | sed 's/      - run: echo dry/      - run: ssh pi@host true/' | fixture ssh)"
expect_fail "workflow reaching the Pi is refused" "reaches the Pi or relay" "$check" "$d"
d="$(good_workflow | sed 's|      - run: echo dry|      - run: deploy/pi/deploy.sh --no-build|' | fixture deploy)"
expect_fail "workflow running deploy.sh is refused" "reaches the Pi or relay" "$check" "$d"

# --- release-automation/owner-approval-gate: the environment is really protected
# spec: release-automation/owner-approval-gate
d="$(good_workflow | sed "/release-guard.sh environment release/d" | fixture noenvcheck)"
expect_fail "release job must first check the environment" "job real must first run release-guard.sh environment release" "$check" "$d"
env_json() { # env_json <reviewer> <admin bypass> <custom policies>
  printf '{"name":"release","can_admins_bypass":%s,"protection_rules":[{"type":"required_reviewers","reviewers":[{"type":"User","reviewer":{"login":"%s"}}]},{"type":"branch_policy"}],"deployment_branch_policy":{"protected_branches":false,"custom_branch_policies":%s}}' "$2" "$1" "$3"
}
tags_json='{"total_count":2,"branch_policies":[{"name":"server-v*","type":"tag"},{"name":"app-v*","type":"tag"}]}'
env_json koppathachan false true >"$tmp/env-good.json"
printf "%s" "$tags_json" >"$tmp/pol-good.json"
# spec: release-automation/owner-approval-gate
expect_pass "protected environment passes" "$guard" environment-json release koppathachan "$tmp/env-good.json" "$tmp/pol-good.json"
printf '{"name":"release","can_admins_bypass":true,"protection_rules":[],"deployment_branch_policy":null}' >"$tmp/env-new.json"
printf '{"total_count":0,"branch_policies":[]}' >"$tmp/pol-none.json"
expect_fail "auto-created environment is refused" "no required reviewer koppathachan" "$guard" environment-json release koppathachan "$tmp/env-new.json" "$tmp/pol-none.json"
env_json mallory false true >"$tmp/env-other.json"
expect_fail "another reviewer is refused" "no required reviewer koppathachan" "$guard" environment-json release koppathachan "$tmp/env-other.json" "$tmp/pol-good.json"
env_json koppathachan true true >"$tmp/env-bypass.json"
expect_fail "admin bypass is refused" "administrators can bypass it" "$guard" environment-json release koppathachan "$tmp/env-bypass.json" "$tmp/pol-good.json"
printf '{"total_count":3,"branch_policies":[{"name":"server-v*","type":"tag"},{"name":"app-v*","type":"tag"},{"name":"main","type":"branch"}]}' >"$tmp/pol-branch.json"
expect_fail "a branch rule is refused" "not exactly tags app-v* and server-v*" "$guard" environment-json release koppathachan "$tmp/env-good.json" "$tmp/pol-branch.json"

# --- release-automation/release-secrets-only-in-the-release-environment: store keys away from build code
# spec: release-automation/release-secrets-only-in-the-release-environment
d="$(good_workflow | sed "s/      contents: write/      contents: read\n      id-token: write/; s|      - run: echo \"\${{ secrets.SOME_KEY }}\"|      - run: .github/scripts/android-aab.sh release|" | fixture oidc)"
expect_fail "build step with id-token must not reach the OIDC token" "can ask for the OIDC token" "$check" "$d"
d="$(good_workflow | sed "s/      contents: write/      contents: read\n      id-token: write/; s|      - run: echo \"\${{ secrets.SOME_KEY }}\"|      - run: env -u ACTIONS_ID_TOKEN_REQUEST_TOKEN -u ACTIONS_ID_TOKEN_REQUEST_URL .github/scripts/android-aab.sh release|" | fixture oidcok)"
expect_pass "build step without the OIDC token passes" "$check" "$d"
# spec: release-automation/ios-release-to-testflight
d="$(good_workflow | sed "s|      - run: echo \"\${{ secrets.SOME_KEY }}\"|      - run: .github/scripts/ios-ipa.sh release \${{ secrets.ASC_KEY_ID }}|" | fixture asc)"
expect_fail "App Store Connect key only in the upload step" "gets the App Store Connect key but does not only upload" "$check" "$d"
expect_fail "ios upload needs the API key" "ASC_KEY_ID is not set" \
  env -i PATH="$PATH" HOME="$HOME" bash -c "cd '$repo' && touch '$tmp/a.ipa' && '$here/ios-ipa.sh' upload '$tmp/a.ipa'"

# --- ci.yml: each check runs only when its code changed (design D16) ---------
c="$tmp/changes"
git -C "$tmp" init -q -b main changes
git -C "$c" config user.email t@example.invalid
git -C "$c" config user.name test
git -C "$c" config commit.gpgsign false
echo a >"$c/README.md" && git -C "$c" add -A && git -C "$c" commit -qm base
changes_for() { # changes_for <path> -> the detector's output for one changed file
  (
    cd "$c"
    git checkout -q main
    git checkout -q -B "try" main
    mkdir -p "$(dirname "$1")" && echo x >>"$1" && git add -A && git commit -qm change
    CI_BASE=main "$here/ci-changes.sh" 2>/dev/null | tr "\n" " "
  )
}
expect_out() { # expect_out <name> <path> <expected>
  local got
  got="$(changes_for "$2")"
  if [ "$got" = "$3" ]; then ok "$1"; else bad "$1 (got: $got)"; fi
}
expect_out "docs only: nothing runs" docs/x.md "proto=false rust=false flutter=false "
expect_out "openspec only: nothing runs" openspec/changes/x/tasks.md "proto=false rust=false flutter=false "
expect_out "release workflow only: nothing runs here" .github/workflows/release-app.yml "proto=false rust=false flutter=false "
expect_out "server code: rust" server/src/main.rs "proto=false rust=true flutter=false "
expect_out "Cargo.lock: rust" Cargo.lock "proto=false rust=true flutter=false "
expect_out "deploy files: rust (server tests read them)" deploy/pi/deploy.sh "proto=false rust=true flutter=false "
expect_out "proto: proto and rust" proto/vyajena/v1/x.proto "proto=true rust=true flutter=false "
expect_out "app: flutter" app/lib/main.dart "proto=false rust=false flutter=true "
expect_out "ci.yml: everything" .github/workflows/ci.yml "proto=true rust=true flutter=true "
got="$(cd "$c" && CI_BASE=0000000000000000000000000000000000000000 "$here/ci-changes.sh" 2>/dev/null | tr "\n" " ")"
if [ "$got" = "proto=true rust=true flutter=true " ]; then ok "no base: everything"; else bad "no base: everything (got: $got)"; fi

echo
echo "$passed passed, $failed failed"
[ "$failed" = 0 ]
