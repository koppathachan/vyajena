#!/usr/bin/env bash
# Runs the app's integration test (flutter-app task 6.2, design D6/D7) on one
# iOS Simulator or Android emulator:
#
#   app/integration_test/run.sh <device id> [chat|images]   # flutter devices lists them
#
# chat (the default) is chat_test.dart with bridge/tests/app_peer.rs; images is
# images_test.dart with bridge/tests/app_peer_images.rs (the server gets a picture
# store in its temp folder).
#
# 1. starts a temp server (cargo's debug/server) on a free port with its data in
#    a temp folder: never server/local.toml, so the owner's invite is safe;
# 2. starts the admin's device on the host (bridge/tests/app_peer.rs,
#    driven through the core), which registers from the server's bootstrap
#    invite, creates the group and an invite for bob;
# 3. on an Android emulator, `adb reverse` so the invite's 127.0.0.1:<port>
#    reaches the Mac (the iOS Simulator reaches it directly);
# 4. runs integration_test/chat_test.dart on the device as Bob;
# 5. stops the peer and the server and removes the temp folder.
#
# On Xcode 27.2 never `flutter build ios --simulator` (lipo fails, design
# D2); `flutter test integration_test -d <simulator>` builds one
# architecture and works.
set -euo pipefail

device="${1:?usage: $0 <device id> [chat|images]}"
scenario="${2:-chat}"
case "$scenario" in
  chat) peer_test=app_peer app_test=chat_test.dart ;;
  images) peer_test=app_peer_images app_test=images_test.dart ;;
  *) echo "usage: $0 <device id> [chat|images]" >&2; exit 1 ;;
esac
app_dir="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$app_dir/.." && pwd)"
work="$(mktemp -d "${TMPDIR:-/tmp}/vyajena-it.XXXXXX")"
server_pid=""
peer_pid=""
reversed=""
adb="$(command -v adb || echo "${ANDROID_HOME:-$HOME/Library/Android/sdk}/platform-tools/adb")"

cleanup() {
  [[ -n "$peer_pid" ]] && kill "$peer_pid" 2>/dev/null || true
  [[ -n "$server_pid" ]] && kill "$server_pid" 2>/dev/null || true
  wait 2>/dev/null || true
  [[ -n "$reversed" ]] && "$adb" -s "$device" reverse --remove "tcp:$reversed" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

wait_for() { # <what> <seconds> <command...>
  local what="$1" seconds="$2"
  shift 2
  for _ in $(seq 1 $((seconds * 4))); do
    if "$@"; then return 0; fi
    sleep 0.25
  done
  echo "timed out waiting for $what" >&2
  return 1
}

port="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"
mkdir -p "$work/blobs"
cat > "$work/server.toml" <<EOF
server_id = "chat"
admin_username = "akhil"
listen_address = "127.0.0.1:$port"
public_address = "127.0.0.1:$port"
database_path = "server.sqlite"
key_dir = "keys"

[blob_store]
backend = "filesystem"
path = "blobs"
EOF

echo "== building the server and the host peer"
(cd "$repo" && cargo build -q -p server)
# Cargo's target folder (CARGO_TARGET_DIR may move it out of the repo).
target_dir="$(cd "$repo" && cargo metadata -q --format-version 1 --no-deps |
  python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"
peer_bin="$(cd "$repo" && cargo test -q -p bridge --test "$peer_test" --no-run --message-format=json |
  PEER_TEST="$peer_test" python3 -c 'import json, os, sys
for line in sys.stdin:
    m = json.loads(line)
    if m.get("reason") == "compiler-artifact" and m["target"]["name"] == os.environ["PEER_TEST"] and m.get("executable"):
        print(m["executable"])')"

echo "== temp server on 127.0.0.1:$port ($work)"
NO_COLOR=1 "$target_dir/debug/server" --config "$work/server.toml" > "$work/server.log" 2>&1 &
server_pid=$!
wait_for "the bootstrap invite" 30 grep -q 'bootstrap admin invite created' "$work/server.log"
bootstrap="$(sed -n 's/.*bootstrap admin invite created.* payload=\([A-Za-z0-9_-]*\).*/\1/p' "$work/server.log")"

echo "== the admin's device on the host"
VYAJENA_PEER_BOOTSTRAP="$bootstrap" VYAJENA_PEER_DIR="$work" \
  "$peer_bin" --ignored --nocapture "$peer_test" > "$work/peer.log" 2>&1 &
peer_pid=$!
wait_for "the invite for bob" 240 test -s "$work/invite.txt"

if [[ -x "$adb" ]] && "$adb" devices 2>/dev/null | grep -q "^${device}[[:space:]]"; then
  "$adb" -s "$device" reverse "tcp:$port" "tcp:$port"
  reversed="$port"
fi

echo "== the app on $device"
status=0
(cd "$app_dir" && flutter test "integration_test/$app_test" -d "$device" \
  --dart-define="VYAJENA_INVITE=$(cat "$work/invite.txt")") || status=$?

if [[ $status -eq 0 ]] && ! wait_for "the peer to finish" 30 test -e "$work/done"; then
  status=1
fi
if [[ $status -ne 0 ]]; then
  echo "== peer log"; cat "$work/peer.log"
  echo "== server log"; cat "$work/server.log"
fi
exit $status
