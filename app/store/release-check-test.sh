#!/usr/bin/env bash
# Tests release-check.sh (app-release task 6.2) on a real bundle and archive.
# Give it a bundle signed with a throwaway upload key, that key's public
# fingerprint file, and an archive; it checks that the good pair passes and
# that copies broken one way each fail. Read-only for its inputs.
#
#   app/store/release-check-test.sh <good.aab> <signing.md> <Runner.xcarchive>
#
# spec: app-release/release-check
# spec: app-release/store-architectures
# spec: app-release/release-signing-keys-stay-with-the-owner
# spec: app-release/one-version-source
# spec: app-release/declared-permissions
set -euo pipefail

good="${1:?good bundle}"
signing="${2:?signing.md with the throwaway key fingerprint}"
archive="${3:?Runner.xcarchive}"
check="$(cd "$(dirname "$0")" && pwd)/release-check.sh"
pubspec="$(cd "$(dirname "$0")/.." && pwd)/pubspec.yaml"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
failures=0

# expect <pass|fail> <check name that must fail, or -> <label> <args...>
expect() {
  local want="$1" line="$2" label="$3"
  shift 3
  local out status=0
  out="$("$check" "$@" 2>&1)" || status=$?
  if [ "$want" = pass ] && [ "$status" = 0 ]; then
    echo "ok    $label"
  elif [ "$want" = fail ] && [ "$status" != 0 ] && grep -q "^FAIL  $line " <<<"$out"; then
    echo "ok    $label (FAIL $line)"
  else
    echo "WRONG $label: wanted $want${line:+ on $line}, exit $status"
    while IFS= read -r l; do echo "      $l"; done <<<"$out"
    failures=$((failures + 1))
  fi
}

# All good: every check passes.
expect pass - "good bundle and archive" --aab "$good" --signing "$signing" --xcarchive "$archive"

# Signed with the debug key instead of the upload key.
cp "$good" "$work/debug.aab"
zip -q -d "$work/debug.aab" 'META-INF/*'
jarsigner -keystore "$HOME/.android/debug.keystore" -storepass android \
  "$work/debug.aab" androiddebugkey >/dev/null 2>&1
expect fail signer "debug-signed bundle" --android-only --aab "$work/debug.aab" --signing "$signing"

# No fingerprint recorded yet (the committed placeholder).
expect fail signer "no recorded upload key" --android-only --aab "$good" \
  --signing "$(dirname "$check")/signing.md"

# Missing armeabi-v7a.
cp "$good" "$work/no-v7a.aab"
zip -q -d "$work/no-v7a.aab" 'base/lib/armeabi-v7a/*'
expect fail architectures "bundle without armeabi-v7a" --android-only --aab "$work/no-v7a.aab" --signing "$signing"

# Build number not above the last release: 0 is never above anything.
sed 's/^version: \(.*\)+[0-9]*$/version: \1+0/' "$pubspec" >"$work/pubspec.yaml"
expect fail version "reused build number" --aab "$good" --signing "$signing" \
  --xcarchive "$archive" --pubspec "$work/pubspec.yaml"

# An extra permission (CAMERA) in the bundle's protobuf manifest.
mkdir -p "$work/extra/base/manifest"
cp "$good" "$work/extra.aab"
unzip -p "$good" base/manifest/AndroidManifest.xml |
  python3 -I -c '
import sys

def varint(n):
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        out.append(b | (0x80 if n else 0))
        if not n:
            return bytes(out)

def field(num, payload):  # length-delimited
    return varint(num << 3 | 2) + varint(len(payload)) + payload

def read_varint(b, i):
    r = s = 0
    while True:
        c = b[i]
        i += 1
        r |= (c & 0x7F) << s
        s += 7
        if c < 0x80:
            return r, i

root = sys.stdin.buffer.read()
# The root XmlNode: field 1 is the <manifest> element; keep any other field.
i, rest, element = 0, b"", None
while i < len(root):
    key, j = read_varint(root, i)
    n, k = read_varint(root, j)
    if key == (1 << 3 | 2) and element is None:
        element = root[k:k + n]
    else:
        rest += root[i:k + n]
    i = k + n
attribute = field(1, b"http://schemas.android.com/apk/res/android") + field(2, b"name") + field(3, b"android.permission.CAMERA")
child = field(1, field(3, b"uses-permission") + field(4, attribute))
sys.stdout.buffer.write(field(1, element + field(5, child)) + rest)
' >"$work/extra/base/manifest/AndroidManifest.xml"
(cd "$work/extra" && zip -q "$work/extra.aab" base/manifest/AndroidManifest.xml)
expect fail permissions "bundle asking for CAMERA too" --android-only --aab "$work/extra.aab" --signing "$signing"

if [ "$failures" = 0 ]; then
  echo "release-check tests: all passed"
else
  echo "release-check tests: $failures wrong"
  exit 1
fi
