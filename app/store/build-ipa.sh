#!/usr/bin/env bash
# Builds the signed App Store Connect IPA (app-release design D4, 7.4 result).
# `flutter build ipa --export-options-plist=...` archives with a development
# profile, which Apple only issues once a device is registered to the team. So
# archive unsigned, then let the export sign it for App Store Connect: that
# needs no device, and Xcode's automatic signing makes (or reuses) the
# cloud-managed Apple Distribution certificate and the App Store profile with
# the Apple account added in Xcode › Settings › Accounts.
#
#   app/store/build-ipa.sh            # then: app/store/release-check.sh --ios-only
#   app/store/build-ipa.sh --upload   # after the check passes: sends the archive
#                                     # to App Store Connect (Xcode's account)
set -euo pipefail

if [ "${1:-}" = "--upload" ]; then
  # Re-exports the archive the last build made with destination "upload".
  app="$(cd "$(dirname "$0")/.." && pwd)"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  cp "$app/ios/ExportOptions.plist" "$tmp/upload.plist"
  plutil -replace destination -string upload "$tmp/upload.plist"
  for try in 1 2 3; do
    if xcodebuild -exportArchive \
      -archivePath "$app/build/ios/archive/Runner.xcarchive" \
      -exportOptionsPlist "$tmp/upload.plist" \
      -exportPath "$tmp/out" \
      -allowProvisioningUpdates; then
      exit 0
    fi
    [ "$try" -lt 3 ] || exit 1
    echo "upload failed (try $try of 3); retrying"
    sleep 10
  done
fi

app="$(cd "$(dirname "$0")/.." && pwd)"
archive="$app/build/ios/archive/Runner.xcarchive"
out="$app/build/ios/ipa"

cd "$app"
rm -rf "$archive" "$out"
flutter build ipa --release --no-codesign "$@"
# The export talks to Apple (certificate, profile); retry a dropped connection.
for try in 1 2 3; do
  if xcodebuild -exportArchive \
    -archivePath "$archive" \
    -exportOptionsPlist ios/ExportOptions.plist \
    -exportPath "$out" \
    -allowProvisioningUpdates; then
    break
  fi
  [ "$try" -lt 3 ] || exit 1
  echo "export failed (try $try of 3); retrying"
  rm -rf "$out"
  sleep 10
done
ipa="$(ls "$out"/*.ipa)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
unzip -q "$ipa" -d "$tmp"
codesign --verify --deep --strict "$tmp"/Payload/*.app
codesign -dvv "$tmp"/Payload/*.app 2>&1 | grep -E '^(Authority=Apple Distribution|TeamIdentifier)'
echo "ipa: $ipa"
