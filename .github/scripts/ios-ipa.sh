#!/usr/bin/env bash
# Builds, checks and (release only) uploads the iOS app on a macOS runner
# (ci-release design D8, D9). Mirrors app/store/build-ipa.sh, with manual
# signing from the `release` environment instead of Xcode's account.
#
#   ios-ipa.sh dry-run   unsigned archive + release check; nothing signed
#   ios-ipa.sh release   signed App Store export, push entitlement check,
#                        release check, upload to App Store Connect (TestFlight)
#
# release needs (environment secrets, never printed):
#   IOS_DIST_CERT_P12_B64, IOS_DIST_CERT_PASSWORD   Apple Distribution .p12
#   IOS_PROFILE_B64                                  App Store profile (Push on)
#   ASC_KEY_ID, ASC_ISSUER_ID, ASC_KEY_P8_B64        App Store Connect API key
#   RELEASE_TAG                                      the pushed tag
#
# The certificate lives only in a temporary keychain, the profile and the API
# key only for this job; all are removed on exit.
set -euo pipefail

readonly BUNDLE_ID="com.purplechaamba.vyajena"

mode="${1:-}"
case "$mode" in dry-run | release) ;; *) echo "usage: ios-ipa.sh dry-run|release" >&2; exit 2 ;; esac

repo="$(git rev-parse --show-toplevel)"
app="$repo/app"
archive="$app/build/ios/archive/Runner.xcarchive"
out="$app/build/ios/ipa"
pb=/usr/libexec/PlistBuddy

if [ "$mode" = dry-run ]; then
  cd "$app"
  rm -rf "$archive" "$out"
  flutter build ipa --release --no-codesign
  "$app/store/release-check.sh" --ios-only --xcarchive "$archive"
  echo "archive (unsigned, dry run): $archive"
  exit 0
fi

for name in IOS_DIST_CERT_P12_B64 IOS_DIST_CERT_PASSWORD IOS_PROFILE_B64 \
  ASC_KEY_ID ASC_ISSUER_ID ASC_KEY_P8_B64 RELEASE_TAG; do
  [ -n "${!name:-}" ] || { echo "ios-ipa.sh: $name is not set (environment release)" >&2; exit 1; }
done
[[ "$ASC_KEY_ID" =~ ^[A-Z0-9]+$ ]] || { echo "ios-ipa.sh: ASC_KEY_ID is not a key id" >&2; exit 1; }

umask 077
work="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/ios-release.XXXXXX")"
keychain="$work/release.keychain-db"
kc_pw="$(openssl rand -hex 24)"
old_keychains="$(security list-keychains -d user | sed -e 's/^[[:space:]]*"//' -e 's/"$//')"
installed_profiles=()
cleanup() {
  # shellcheck disable=SC2086 # one path per line, no spaces in runner paths
  security list-keychains -d user -s $old_keychains >/dev/null 2>&1 || true
  security delete-keychain "$keychain" >/dev/null 2>&1 || true
  for p in "${installed_profiles[@]}"; do rm -f "$p"; done
  rm -rf "$work"
}
trap cleanup EXIT

# --- Temporary keychain with the distribution certificate ---------------------
printf '%s' "$IOS_DIST_CERT_P12_B64" | base64 -D >"$work/dist.p12"
security create-keychain -p "$kc_pw" "$keychain"
security set-keychain-settings -lut 3600 "$keychain"
security unlock-keychain -p "$kc_pw" "$keychain"
security import "$work/dist.p12" -k "$keychain" -f pkcs12 -P "$IOS_DIST_CERT_PASSWORD" \
  -T /usr/bin/codesign -T /usr/bin/security >/dev/null
rm -f "$work/dist.p12"
security set-key-partition-list -S apple-tool:,apple: -s -k "$kc_pw" "$keychain" >/dev/null
# shellcheck disable=SC2086
security list-keychains -d user -s "$keychain" $old_keychains
identity="$(security find-identity -v -p codesigning "$keychain" |
  awk '/"Apple Distribution/ { print $2; exit }')"
[ -n "$identity" ] || { echo "ios-ipa.sh: no Apple Distribution identity in IOS_DIST_CERT_P12_B64" >&2; exit 1; }

# --- The App Store profile -----------------------------------------------------
printf '%s' "$IOS_PROFILE_B64" | base64 -D >"$work/profile.mobileprovision"
security cms -D -i "$work/profile.mobileprovision" >"$work/profile.plist"
uuid="$("$pb" -c 'Print :UUID' "$work/profile.plist")"
team="$("$pb" -c 'Print :TeamIdentifier:0' "$work/profile.plist")"
app_id="$("$pb" -c 'Print :Entitlements:application-identifier' "$work/profile.plist")"
aps="$("$pb" -c 'Print :Entitlements:aps-environment' "$work/profile.plist" 2>/dev/null || true)"
devices="$("$pb" -c 'Print :ProvisionedDevices' "$work/profile.plist" 2>/dev/null || true)"
[ "$app_id" = "$team.$BUNDLE_ID" ] || { echo "ios-ipa.sh: profile is for $app_id, not $team.$BUNDLE_ID" >&2; exit 1; }
[ "$aps" = production ] || { echo "ios-ipa.sh: profile has no aps-environment = production (Push)" >&2; exit 1; }
[ -z "$devices" ] || { echo "ios-ipa.sh: profile lists devices: not an App Store profile" >&2; exit 1; }
for dir in "$HOME/Library/MobileDevice/Provisioning Profiles" \
  "$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles"; do
  mkdir -p "$dir"
  cp "$work/profile.mobileprovision" "$dir/$uuid.mobileprovision"
  installed_profiles+=("$dir/$uuid.mobileprovision")
done

# --- Build: unsigned archive, signed once, App Store export --------------------
cd "$app"
rm -rf "$archive" "$out"
flutter build ipa --release --no-codesign
# As in build-ipa.sh: the archived app carries Runner.entitlements so the export
# keeps aps-environment (and sets it to production from the profile).
codesign --force --sign "$identity" --keychain "$keychain" \
  --entitlements ios/Runner/Runner.entitlements \
  "$archive/Products/Applications/Runner.app"

cp ios/ExportOptions.plist "$work/export.plist"
"$pb" -c 'Set :signingStyle manual' "$work/export.plist"
"$pb" -c "Set :teamID $team" "$work/export.plist"
"$pb" -c 'Add :signingCertificate string Apple Distribution' "$work/export.plist"
"$pb" -c 'Add :provisioningProfiles dict' "$work/export.plist"
"$pb" -c "Add :provisioningProfiles:$BUNDLE_ID string $uuid" "$work/export.plist"
xcodebuild -exportArchive -archivePath "$archive" \
  -exportOptionsPlist "$work/export.plist" -exportPath "$out"

ipa="$(ls "$out"/*.ipa)"
mkdir "$work/ipa"
unzip -q "$ipa" -d "$work/ipa"
codesign --verify --deep --strict "$work"/ipa/Payload/*.app
codesign -dvv "$work"/ipa/Payload/*.app 2>&1 | grep -E '^(Authority=Apple Distribution|TeamIdentifier)'
entitlements="$(codesign -d --entitlements :- "$work"/ipa/Payload/*.app 2>/dev/null)"
if [[ "$entitlements" != *"<key>aps-environment</key><string>production</string>"* ]]; then
  echo "ios-ipa.sh: the exported app has no aps-environment = production: push would not arrive" >&2
  exit 1
fi
echo "push: aps-environment = production"

# --- Release check, then upload ------------------------------------------------
git -C "$repo" tag -d "$RELEASE_TAG" >/dev/null
"$app/store/release-check.sh" --ios-only --xcarchive "$archive"

mkdir "$work/private_keys"
printf '%s' "$ASC_KEY_P8_B64" | base64 -D >"$work/private_keys/AuthKey_$ASC_KEY_ID.p8"
for try in 1 2 3; do
  if API_PRIVATE_KEYS_DIR="$work/private_keys" xcrun altool --upload-app --type ios \
    --file "$ipa" --apiKey "$ASC_KEY_ID" --apiIssuer "$ASC_ISSUER_ID"; then
    echo "uploaded: $ipa"
    exit 0
  fi
  [ "$try" -lt 3 ] || exit 1
  echo "upload failed (try $try of 3); retrying"
  sleep 20
done
