#!/usr/bin/env bash
# Which CI checks a change needs (ci-release design D16). Prints GitHub step
# outputs, one per check:
#
#   proto=true|false     buf lint and breaking
#   rust=true|false      cargo build, test, fmt and clippy
#   flutter=true|false   flutter analyze and test
#   openspec=true|false  openspec validate --strict
#
#   CI_BASE=<commit or ref> ci-changes.sh    compares CI_BASE...HEAD
#
# With no usable CI_BASE (a new branch, a first push) every check runs: when
# in doubt, test. ci.yml itself turns every check on.
set -euo pipefail

all() { printf 'proto=true\nrust=true\nflutter=true\nopenspec=true\n'; exit 0; }

base="${CI_BASE:-}"
if [ -z "$base" ] || [ "$base" = 0000000000000000000000000000000000000000 ] ||
  ! git rev-parse -q --verify "$base^{commit}" >/dev/null; then
  echo "ci-changes: no base commit; running every check" >&2
  all
fi

proto=false rust=false flutter=false openspec=false
while IFS= read -r path; do
  case "$path" in
    .github/workflows/ci.yml | .github/scripts/ci-changes.sh) all ;;
  esac
  case "$path" in
    proto/* | buf.yaml | buf.lock) proto=true ;;
  esac
  case "$path" in
    # The crates, the committed protos they are generated from, and deploy/
    # (server/tests read deploy/server and deploy/relay scripts).
    Cargo.toml | Cargo.lock | rust-toolchain* | .cargo/* | protocol/* | server/* | \
      client/* | bridge/* | pbgen/* | proto/* | deploy/*) rust=true ;;
  esac
  case "$path" in
    app/*) flutter=true ;;
  esac
  case "$path" in
    openspec/*) openspec=true ;;
  esac
done < <(git diff --name-only "$base...HEAD")

echo "ci-changes: proto=$proto rust=$rust flutter=$flutter openspec=$openspec" >&2
printf 'proto=%s\nrust=%s\nflutter=%s\nopenspec=%s\n' "$proto" "$rust" "$flutter" "$openspec"
