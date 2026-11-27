#!/usr/bin/env bash
# Builds, checks and uploads the iOS app on a macOS runner (ci-release design
# D8, D9). Mirrors app/store/build-ipa.sh, with manual signing from the
# `release` environment instead of Xcode's account.
#
#   ios-ipa.sh dry-run        unsigned archive + release check; nothing signed
#   ios-ipa.sh release        signed App Store export, push entitlement check,
#                             release check; leaves app/build/ios/ipa/*.ipa
#   ios-ipa.sh upload <ipa>   uploads that IPA to App Store Connect (TestFlight)
#
# Separate steps on purpose: the build runs third-party code (pods, crates,
# Flutter plugins), so the App Store Connect key is not in its environment.
#
# release needs (environment secrets, never printed):
#   IOS_DIST_CERT_P12_B64, IOS_DIST_CERT_PASSWORD   Apple Distribution .p12
#   IOS_PROFILE_B64                                  app's App Store profile
#                                                    (Push + App Groups on)
#   IOS_NSE_PROFILE_B64                              NotificationService.appex's
#                                                    App Store profile (App Groups on)
#   RELEASE_TAG                                      the pushed tag
# upload needs: ASC_KEY_ID, ASC_ISSUER_ID, ASC_KEY_P8_B64 (App Store Connect API key)
#
# Two profiles because the app embeds the on-device notification extension
# (smarter-notifications): the app and the extension share an App Group, so each
# target is signed with its own App Store profile that carries that App Group.
# The Share Extension's source is in the repo but is not an Xcode target yet, so
# it is not built, signed or covered here (ios/ShareExtension/README.md).
#
# The certificate lives only in a temporary keychain and the profiles and the
# API key only in RUNNER_TEMP; each is removed when its step ends.
set -euo pipefail

readonly BUNDLE_ID="com.purplechaamba.vyajena"
readonly NSE_BUNDLE_ID="com.purplechaamba.vyajena.NotificationService"
readonly APP_GROUP="group.com.purplechaamba.vyajena"

mode="${1:-}"
case "$mode" in dry-run | release | upload) ;; *) echo "usage: ios-ipa.sh dry-run|release|upload <ipa>" >&2; exit 2 ;; esac

repo="$(git rev-parse --show-toplevel)"
app="$repo/app"
archive="$app/build/ios/archive/Runner.xcarchive"
out="$app/build/ios/ipa"
pb=/usr/libexec/PlistBuddy

if [ "$mode" = upload ]; then
  ipa="${2:-}"
  [ -f "$ipa" ] || { echo "ios-ipa.sh: upload needs the IPA path" >&2; exit 2; }
  for name in ASC_KEY_ID ASC_ISSUER_ID ASC_KEY_P8_B64; do
    [ -n "${!name:-}" ] || { echo "ios-ipa.sh: $name is not set (environment release)" >&2; exit 1; }
  done
  [[ "$ASC_KEY_ID" =~ ^[A-Z0-9]+$ ]] || { echo "ios-ipa.sh: ASC_KEY_ID is not a key id" >&2; exit 1; }
  umask 077
  keys="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/asc.XXXXXX")"
  trap 'rm -rf "$keys"' EXIT
  printf "%s" "$ASC_KEY_P8_B64" | base64 -D >"$keys/AuthKey_$ASC_KEY_ID.p8"
  for try in 1 2 3; do
    if API_PRIVATE_KEYS_DIR="$keys" xcrun altool --upload-app --type ios \
      --file "$ipa" --apiKey "$ASC_KEY_ID" --apiIssuer "$ASC_ISSUER_ID"; then
      echo "uploaded: $ipa"
      exit 0
    fi
    [ "$try" -lt 3 ] || exit 1
    echo "upload failed (try $try of 3); retrying"
    sleep 20
  done
fi

if [ "$mode" = dry-run ]; then
  cd "$app"
  rm -rf "$archive" "$out"
  flutter build ipa --release --no-codesign
  "$app/store/release-check.sh" --ios-only --xcarchive "$archive"
  echo "archive (unsigned, dry run): $archive"
  exit 0
fi

for name in IOS_DIST_CERT_P12_B64 IOS_DIST_CERT_PASSWORD IOS_PROFILE_B64 IOS_NSE_PROFILE_B64 RELEASE_TAG; do
  [ -n "${!name:-}" ] || { echo "ios-ipa.sh: $name is not set (environment release)" >&2; exit 1; }
done
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

# --- The App Store profiles ----------------------------------------------------
# install_profile <env var> <expected bundle id> <aps: yes|no>. Decodes the
# base64 profile from that variable, checks it is an App Store profile for the
# bundle id with the shared App Group (and Push, for the app), installs it where
# Xcode's export looks, and leaves its UUID in OUT_UUID and team in OUT_TEAM.
install_profile() {
  local var="$1" want_bundle="$2" want_aps="$3"
  local mp="$work/$var.mobileprovision" plist="$work/$var.plist"
  printf '%s' "${!var}" | base64 -D >"$mp"
  security cms -D -i "$mp" >"$plist"
  local uuid team app_id aps devices group
  uuid="$("$pb" -c 'Print :UUID' "$plist")"
  team="$("$pb" -c 'Print :TeamIdentifier:0' "$plist")"
  app_id="$("$pb" -c 'Print :Entitlements:application-identifier' "$plist")"
  devices="$("$pb" -c 'Print :ProvisionedDevices' "$plist" 2>/dev/null || true)"
  group="$("$pb" -c 'Print :Entitlements:com.apple.security.application-groups:0' "$plist" 2>/dev/null || true)"
  [ "$app_id" = "$team.$want_bundle" ] || { echo "ios-ipa.sh: $var is for $app_id, not $team.$want_bundle" >&2; exit 1; }
  [ -z "$devices" ] || { echo "ios-ipa.sh: $var lists devices: not an App Store profile" >&2; exit 1; }
  [ "$group" = "$APP_GROUP" ] || { echo "ios-ipa.sh: $var has no App Group $APP_GROUP: enable App Groups on $want_bundle" >&2; exit 1; }
  if [ "$want_aps" = yes ]; then
    aps="$("$pb" -c 'Print :Entitlements:aps-environment' "$plist" 2>/dev/null || true)"
    [ "$aps" = production ] || { echo "ios-ipa.sh: $var has no aps-environment = production (Push)" >&2; exit 1; }
  fi
  local dir
  for dir in "$HOME/Library/MobileDevice/Provisioning Profiles" \
    "$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles"; do
    mkdir -p "$dir"
    cp "$mp" "$dir/$uuid.mobileprovision"
    installed_profiles+=("$dir/$uuid.mobileprovision")
  done
  OUT_UUID="$uuid"
  OUT_TEAM="$team"
}

install_profile IOS_PROFILE_B64 "$BUNDLE_ID" yes
uuid="$OUT_UUID"
team="$OUT_TEAM"
install_profile IOS_NSE_PROFILE_B64 "$NSE_BUNDLE_ID" no
nse_uuid="$OUT_UUID"
[ "$OUT_TEAM" = "$team" ] || { echo "ios-ipa.sh: the app and extension profiles are from different teams ($team vs $OUT_TEAM)" >&2; exit 1; }

# --- Build: unsigned archive, signed once, App Store export --------------------
cd "$app"
rm -rf "$archive" "$out"
flutter build ipa --release --no-codesign
# As in build-ipa.sh: the archived app carries its entitlements so the export
# keeps aps-environment (set to production from the profile) and the App Group.
# Sign inside out — the embedded extension first, then the app that seals it —
# so the extension keeps its App Group across the export.
appex="$archive/Products/Applications/Runner.app/PlugIns/NotificationService.appex"
[ -d "$appex" ] || { echo "ios-ipa.sh: no NotificationService.appex in the archive" >&2; exit 1; }
codesign --force --sign "$identity" --keychain "$keychain" \
  --entitlements ios/NotificationService/NotificationService.entitlements \
  "$appex"
codesign --force --sign "$identity" --keychain "$keychain" \
  --entitlements ios/Runner/Runner.entitlements \
  "$archive/Products/Applications/Runner.app"

# ExportOptions.plist stays automatic for the owner's local build-ipa.sh; the CI
# export overrides a copy to manual, one provisioning profile per built target.
cp ios/ExportOptions.plist "$work/export.plist"
"$pb" -c 'Set :signingStyle manual' "$work/export.plist"
"$pb" -c "Set :teamID $team" "$work/export.plist"
"$pb" -c 'Add :signingCertificate string Apple Distribution' "$work/export.plist"
"$pb" -c 'Add :provisioningProfiles dict' "$work/export.plist"
"$pb" -c "Add :provisioningProfiles:$BUNDLE_ID string $uuid" "$work/export.plist"
"$pb" -c "Add :provisioningProfiles:$NSE_BUNDLE_ID string $nse_uuid" "$work/export.plist"
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
# The app and its notification extension must keep the shared App Group, or the
# extension cannot read the metadata the app leaves for it (smarter-notifications).
appex_out="$(ls -d "$work"/ipa/Payload/*.app/PlugIns/NotificationService.appex)"
for bundle in "$work"/ipa/Payload/*.app "$appex_out"; do
  ent="$(codesign -d --entitlements :- "$bundle" 2>/dev/null)"
  if [[ "$ent" != *"$APP_GROUP"* ]]; then
    echo "ios-ipa.sh: ${bundle##*/} is missing the App Group $APP_GROUP after export" >&2
    exit 1
  fi
done
echo "app group: $APP_GROUP on the app and the notification extension"

# --- Release check -------------------------------------------------------------
git -C "$repo" tag -d "$RELEASE_TAG" >/dev/null
"$app/store/release-check.sh" --ios-only --xcarchive "$archive"
echo "ipa: $ipa"
