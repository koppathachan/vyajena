#!/usr/bin/env bash
# Builds and checks the Android App Bundle in CI (ci-release design D7, D9).
#
#   android-aab.sh dry-run    throwaway upload key made here, no Firebase file,
#                             checked against the throwaway key's fingerprint
#   android-aab.sh release    the owner's upload key and Firebase file from the
#                             `release` environment, checked against the
#                             fingerprint in app/store/signing.md
#
# release needs (environment secrets, never printed):
#   ANDROID_UPLOAD_KEYSTORE_B64, ANDROID_UPLOAD_STORE_PASSWORD,
#   ANDROID_UPLOAD_KEY_PASSWORD, ANDROID_GOOGLE_SERVICES_JSON_B64,
#   and RELEASE_TAG (the pushed tag, so the release check skips it).
#
# Key files live only in $RUNNER_TEMP (mode 0600) and are deleted on exit.
# Gradle reads them through -Pvyajena.keyProperties / -Pvyajena.googleServices
# (app/android/app/build.gradle.kts); nothing in app/ changes.
set -euo pipefail

mode="${1:-}"
case "$mode" in dry-run | release) ;; *) echo "usage: android-aab.sh dry-run|release" >&2; exit 2 ;; esac

repo="$(git rev-parse --show-toplevel)"
app="$repo/app"
aab="$app/build/app/outputs/bundle/release/app-release.aab"

umask 077
keys="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/android-keys.XXXXXX")"
trap 'rm -rf "$keys"' EXIT

gradle_args=(-P "vyajena.keyProperties=$keys/key.properties")
printf 'storeFile=upload.jks\nkeyAlias=upload\n' >"$keys/key.properties"

if [ "$mode" = dry-run ]; then
  pw="$(openssl rand -hex 24)"
  keytool -genkeypair -noprompt -keystore "$keys/upload.jks" -storetype PKCS12 \
    -alias upload -keyalg RSA -keysize 2048 -validity 2 \
    -dname "CN=vyajena ci dry run (throwaway)" -storepass "$pw" -keypass "$pw" >/dev/null 2>&1
  VYAJENA_UPLOAD_STORE_PASSWORD="$pw"
  VYAJENA_UPLOAD_KEY_PASSWORD="$pw"
  fingerprint="$(keytool -list -v -keystore "$keys/upload.jks" -storepass "$pw" -alias upload |
    sed -n 's/^[[:space:]]*SHA256:[[:space:]]*//p' | head -1)"
  printf 'Upload key SHA-256: %s\n' "$fingerprint" >"$keys/signing.md"
  signing="$keys/signing.md"
  # A dry run has no Firebase file: the bundle builds without push (push D8).
  # Naming a missing file keeps Gradle off its default (~/.vyajena-push/,
  # owner-only) when this runs on the owner's Mac.
  gradle_args+=(-P "vyajena.googleServices=$keys/none.json" -P vyajena.allowNoPush=true)
else
  for name in ANDROID_UPLOAD_KEYSTORE_B64 ANDROID_UPLOAD_STORE_PASSWORD \
    ANDROID_UPLOAD_KEY_PASSWORD ANDROID_GOOGLE_SERVICES_JSON_B64 RELEASE_TAG; do
    [ -n "${!name:-}" ] || { echo "android-aab.sh: $name is not set (environment release)" >&2; exit 1; }
  done
  printf '%s' "$ANDROID_UPLOAD_KEYSTORE_B64" | base64 -d >"$keys/upload.jks"
  printf '%s' "$ANDROID_GOOGLE_SERVICES_JSON_B64" | base64 -d >"$keys/google-services.json"
  VYAJENA_UPLOAD_STORE_PASSWORD="$ANDROID_UPLOAD_STORE_PASSWORD"
  VYAJENA_UPLOAD_KEY_PASSWORD="$ANDROID_UPLOAD_KEY_PASSWORD"
  signing="$app/store/signing.md"
  gradle_args+=(-P "vyajena.googleServices=$keys/google-services.json")
  # The release check wants the build number above every *earlier* app-v tag;
  # the tag being released is itself one, so this throwaway clone drops it.
  git -C "$repo" tag -d "$RELEASE_TAG" >/dev/null
fi
export VYAJENA_UPLOAD_STORE_PASSWORD VYAJENA_UPLOAD_KEY_PASSWORD

cd "$app"
rm -f "$aab"
log="$keys/build.log"
status=0
flutter build appbundle --release "${gradle_args[@]}" 2>&1 | tee "$log" || status=$?
if [ "$status" -ne 0 ]; then
  # As in app/store/build-aab.sh: only Flutter's apkanalyzer step may fail (an
  # SDK path with a space, on a Mac); release-check.sh checks stripping itself.
  if [ -f "$aab" ] && grep -q 'failed to strip debug symbols' "$log"; then
    echo "note: Flutter's apkanalyzer step failed; release-check.sh checks the stripping instead."
  else
    exit "$status"
  fi
fi
unset VYAJENA_UPLOAD_STORE_PASSWORD VYAJENA_UPLOAD_KEY_PASSWORD

"$app/store/release-check.sh" --android-only --aab "$aab" --signing "$signing"
echo "bundle: $aab"
