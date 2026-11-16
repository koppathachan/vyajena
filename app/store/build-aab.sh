#!/usr/bin/env bash
# Builds the signed release bundle with the owner's upload key (app-release
# design D3). The key properties file (~/.vyajena-release/key.properties) names
# the keystore and alias; the two passwords come from the macOS Keychain here,
# so no password sits in a file:
#
#   security add-generic-password -s vyajena-upload -a store -w   # asks for it
#   security add-generic-password -s vyajena-upload -a key -w
#
#   app/store/build-aab.sh            # then: app/store/release-check.sh --android-only
set -euo pipefail

app="$(cd "$(dirname "$0")/.." && pwd)"
aab="$app/build/app/outputs/bundle/release/app-release.aab"

# Push (change push): Gradle stops a release build without the owner's
# Firebase config; say so before asking the Keychain for anything.
if [ ! -f "$HOME/.vyajena-push/google-services.json" ]; then
  echo "missing ~/.vyajena-push/google-services.json (Firebase, change push; owner steps in docs/release.md)" >&2
  exit 1
fi

VYAJENA_UPLOAD_STORE_PASSWORD="$(security find-generic-password -s vyajena-upload -a store -w)"
VYAJENA_UPLOAD_KEY_PASSWORD="$(security find-generic-password -s vyajena-upload -a key -w)"
export VYAJENA_UPLOAD_STORE_PASSWORD VYAJENA_UPLOAD_KEY_PASSWORD

cd "$app"
rm -f "$aab"
log="$(mktemp)"
trap 'rm -f "$log"' EXIT
status=0
flutter build appbundle --release "$@" 2>&1 | tee "$log" || status=$?
if [ "$status" -ne 0 ]; then
  # Flutter's last step runs the SDK's apkanalyzer to see that the native
  # libraries were stripped. apkanalyzer's script breaks when the Android SDK
  # sits on a path with a space (for example an external drive), so that step
  # fails although Gradle built the bundle. release-check.sh checks the
  # stripping itself ("stripped"), so only that failure is let through.
  if [ -f "$aab" ] && grep -q 'failed to strip debug symbols' "$log"; then
    echo "note: Flutter's apkanalyzer step failed (SDK path with a space?);"
    echo "      release-check.sh checks the stripping instead."
  else
    exit "$status"
  fi
fi
echo "bundle: $aab"
