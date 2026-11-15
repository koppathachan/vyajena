#!/usr/bin/env bash
# Store screenshots (app-release design D15) from the app on the FAKE core.
# Use a throwaway simulator or emulator: the run installs the app there.
#
#   app/store/screenshots.sh <device id> <ios|android>
#
# iOS: an iPhone 16/17 Pro Max simulator (6.9", 1320x2868) is what App Store
# Connect asks for. Writes app/store/screenshots/<platform>/<n>-<name>.png.
set -euo pipefail

device="${1:?device id}"
platform="${2:?ios or android}"
app="$(cd "$(dirname "$0")/.." && pwd)"

cd "$app"
VYAJENA_SHOTS="$app/store/screenshots/$platform" flutter drive \
  --driver=test_driver/screenshots.dart \
  --target=integration_test/screenshots_test.dart \
  -d "$device"
ls -1 "$app/store/screenshots/$platform"
