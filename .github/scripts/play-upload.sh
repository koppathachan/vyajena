#!/usr/bin/env bash
# Uploads the App Bundle to the Play **internal** track and completes the
# release for internal testers (ci-release design D7). The track and the
# package are fixed here on purpose: this script reaches no other track.
#
#   play-upload.sh <app-release.aab>
#
# Needs PLAY_ACCESS_TOKEN (an OAuth token for the androidpublisher scope, from
# google-github-actions/auth through workload identity; never printed) and
# reads the version from app/pubspec.yaml.
set -euo pipefail

readonly PACKAGE="com.purplechaamba.vyajena"
readonly TRACK="internal"

aab="${1:-}"
[ -f "$aab" ] || { echo "usage: play-upload.sh <app-release.aab>" >&2; exit 2; }
[ -n "${PLAY_ACCESS_TOKEN:-}" ] || { echo "play-upload.sh: PLAY_ACCESS_TOKEN is not set" >&2; exit 1; }

repo="$(git rev-parse --show-toplevel)"
version="$("$repo/.github/scripts/release-guard.sh" version app)"
name="${version%%+*}"
build="${version##*+}"

api="https://androidpublisher.googleapis.com/androidpublisher/v3/applications/$PACKAGE"
upload="https://androidpublisher.googleapis.com/upload/androidpublisher/v3/applications/$PACKAGE"

# The token goes to curl in a 0600 header file, not on its command line.
umask 077
work="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/play.XXXXXX")"
edit=""
cleanup() {
  if [ -n "$edit" ]; then
    curl -sS -o /dev/null -H @"$work/auth" -X DELETE "$api/edits/$edit" || true
  fi
  rm -rf "$work"
}
trap cleanup EXIT
printf 'Authorization: Bearer %s\n' "$PLAY_ACCESS_TOKEN" >"$work/auth"

call() { curl --fail-with-body -sS --max-time 900 -H @"$work/auth" "$@"; }

edit="$(call -X POST -H 'Content-Type: application/json' -d '{}' "$api/edits" | jq -r '.id')"
[ -n "$edit" ] && [ "$edit" != null ] || { echo "play-upload.sh: no edit id" >&2; exit 1; }

code="$(call -X POST -H 'Content-Type: application/octet-stream' --data-binary @"$aab" \
  "$upload/edits/$edit/bundles?uploadType=media" | jq -r '.versionCode')"
[ "$code" = "$build" ] || { echo "play-upload.sh: Play says version code $code, pubspec says $build" >&2; exit 1; }

body="$(jq -n --arg track "$TRACK" --arg name "$name ($build)" --arg code "$code" \
  '{track: $track, releases: [{name: $name, versionCodes: [$code], status: "completed"}]}')"
call -X PUT -H 'Content-Type: application/json' -d "$body" "$api/edits/$edit/tracks/$TRACK" >/dev/null
call -X POST "$api/edits/$edit:commit" >/dev/null
edit=""
echo "play: $name ($build) on the $TRACK track"
