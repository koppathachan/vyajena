#!/usr/bin/env bash
# Scratch phones for the integration tests (the tests wipe the app's data on
# the device they run on, so they never use the phones you chat on):
#
#   deploy/local/devices.sh ios      # prints the id of a booted "Vyajena Test" simulator
#   deploy/local/devices.sh android  # prints the serial of a running "vyajena_test" emulator
#
# Creates and boots them when needed. Override the names with
# IOS_TEST_NAME and ANDROID_TEST_AVD. Progress goes to stderr, the id to stdout.
set -euo pipefail

ios_name="${IOS_TEST_NAME:-Vyajena Test}"
avd="${ANDROID_TEST_AVD:-vyajena_test}"
adb="$(command -v adb || echo "${ANDROID_HOME:-$HOME/Library/Android/sdk}/platform-tools/adb")"

ios() {
  local udid
  udid="$(xcrun simctl list devices | grep -F "    $ios_name (" | grep -Eo '[0-9A-F]{8}-([0-9A-F]{4}-){3}[0-9A-F]{12}' | head -1 || true)"
  if [[ -z "$udid" ]]; then
    echo "creating the simulator \"$ios_name\"" >&2
    local runtime
    runtime="$(xcrun simctl list runtimes -j | python3 -c '
import json, sys
runtimes = [r for r in json.load(sys.stdin)["runtimes"] if r["identifier"].split(".")[-1].startswith("iOS") and r.get("isAvailable")]
print(sorted(runtimes, key=lambda r: [int(p) for p in r["version"].split(".")])[-1]["identifier"])')"
    udid="$(xcrun simctl create "$ios_name" "iPhone 16" "$runtime")"
  fi
  xcrun simctl boot "$udid" 2>/dev/null || true
  xcrun simctl bootstatus "$udid" -b >&2
  echo "$udid"
}

android() {
  local serial
  find_serial() {
    local s
    for s in $("$adb" devices | awk '/^emulator-.*device$/ {print $1}'); do
      if [[ "$("$adb" -s "$s" emu avd name 2>/dev/null | head -1 | tr -d '\r')" == "$avd" ]]; then
        echo "$s"
        return 0
      fi
    done
    return 1
  }
  if ! serial="$(find_serial)"; then
    echo "starting the emulator \"$avd\"" >&2
    (cd "$(dirname "$0")/../../app" && flutter emulators --launch "$avd" >&2)
    for _ in $(seq 1 90); do
      serial="$(find_serial)" && break
      sleep 2
    done
    [[ -n "${serial:-}" ]] || { echo "the emulator \"$avd\" did not start" >&2; exit 1; }
  fi
  until [[ "$("$adb" -s "$serial" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" == 1 ]]; do sleep 2; done
  echo "$serial"
}

case "${1:-}" in
  ios) ios ;;
  android) android ;;
  *) echo "usage: $0 ios|android" >&2; exit 1 ;;
esac
