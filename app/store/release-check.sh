#!/usr/bin/env bash
# The release check (app-release: Release check; design D13). Run it on the
# built bundle and archive before every upload. Read-only: it never signs,
# changes or uploads anything.
#
#   app/store/release-check.sh [--aab <app-release.aab>] [--xcarchive <Runner.xcarchive>]
#                              [--signing <signing.md>] [--pubspec <pubspec.yaml>]
#
# Defaults: the bundle and archive `flutter build appbundle` / `flutter build
# ipa` write under app/build, app/store/signing.md and app/pubspec.yaml. A
# missing bundle or archive is reported as not checked and fails the run; pass
# --android-only or --ios-only to check one platform on purpose.
# Needs only tools already on the Mac: unzip, keytool (JDK),
# llvm-readelf (Android NDK), python3, lipo, plutil, sips, cargo, git.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
app="$(cd "$here/.." && pwd)"
repo="$(cd "$app/.." && pwd)"

aab="$app/build/app/outputs/bundle/release/app-release.aab"
archive="$app/build/ios/archive/Runner.xcarchive"
signing="$here/signing.md"
pubspec="$app/pubspec.yaml"
check_android=1
check_ios=1

while [ $# -gt 0 ]; do
  case "$1" in
    --aab) aab="$2"; shift 2 ;;
    --xcarchive) archive="$2"; shift 2 ;;
    --signing) signing="$2"; shift 2 ;;
    --pubspec) pubspec="$2"; shift 2 ;;
    --android-only) check_ios=0; shift ;;
    --ios-only) check_android=0; shift ;;
    -h | --help) sed -n '2,15p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

failed=0
pass() { printf 'PASS  %-22s %s\n' "$1" "$2"; }
fail() { printf 'FAIL  %-22s %s\n' "$1" "$2"; failed=1; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}"
newest() { # the last match of a glob, by version order
  local found
  found="$(compgen -G "$1" | sort -V | tail -1 || true)"
  printf '%s' "$found"
}

# --- Both platforms -----------------------------------------------------------

# Version: pubspec's build number is above every released one (app-v<name>+<build>).
version="$(sed -n 's/^version:[[:space:]]*//p' "$pubspec" | head -1)"
build_name="${version%%+*}"
build_number="${version##*+}"
if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+\+[0-9]+$ ]]; then
  fail version "pubspec version '$version' is not <name>+<build>"
else
  last=0
  while read -r tag; do
    [ -z "$tag" ] && continue
    n="${tag##*+}"
    [[ "$n" =~ ^[0-9]+$ ]] && [ "$n" -gt "$last" ] && last="$n"
  done < <(git -C "$repo" tag -l 'app-v*')
  if [ "$build_number" -gt "$last" ]; then
    pass version "$version (last released build: ${last/#0/none})"
  else
    fail version "build $build_number is not above the last released build $last: raise the build number in pubspec.yaml"
  fi
fi

# No key, certificate or profile is tracked by git (design D3).
tracked="$(git -C "$repo" ls-files | grep -E '(^|/)key\.properties$|\.(jks|keystore|p12|mobileprovision|cer)$|(^|/)AuthKey_.*\.p8$' || true)"
if [ -z "$tracked" ]; then
  pass "no key files" "git tracks no key, certificate or profile"
else
  fail "no key files" "git tracks: $(echo "$tracked" | tr '\n' ' ')"
fi

# The core installs no tracing subscriber in the app (design D7).
subscriber=""
for target in aarch64-linux-android armv7-linux-androideabi x86_64-linux-android aarch64-apple-ios; do
  tree="$(cargo tree --manifest-path "$repo/Cargo.toml" -p bridge -e normal --target "$target" \
    --prefix none 2>/dev/null || true)"
  if [ -z "$tree" ] || grep -q '^tracing-subscriber ' <<<"$tree"; then
    subscriber="$subscriber $target"
  fi
done
if [ -z "$subscriber" ]; then
  pass "core logs nothing" "no tracing-subscriber in the bridge for the store targets"
else
  fail "core logs nothing" "tracing-subscriber in the bridge (or cargo tree failed) for:$subscriber"
fi

# --- Android bundle -------------------------------------------------------------

if [ "$check_android" = 1 ]; then
  if [ ! -f "$aab" ]; then
    fail "android bundle" "not found: $aab"
  else
    entries="$(unzip -Z1 "$aab")"

    # Architectures (app-release: Store architectures).
    missing=""
    for abi in arm64-v8a armeabi-v7a x86_64; do
      grep -qx "base/lib/$abi/libbridge.so" <<<"$entries" || missing="$missing $abi"
    done
    if [ -z "$missing" ]; then
      pass architectures "libbridge.so for arm64-v8a, armeabi-v7a, x86_64"
    else
      fail architectures "libbridge.so missing for:$missing"
    fi

    # 16 KB alignment of the 64-bit core.
    readelf="$(newest "$sdk/ndk/*/toolchains/llvm/prebuilt/*/bin/llvm-readelf")"
    if [ -z "$readelf" ]; then
      fail "16 KB alignment" "llvm-readelf not found under $sdk/ndk"
    else
      bad=""
      for abi in arm64-v8a x86_64; do
        grep -qx "base/lib/$abi/libbridge.so" <<<"$entries" || continue
        unzip -p "$aab" "base/lib/$abi/libbridge.so" >"$work/libbridge-$abi.so"
        while read -r align; do
          [ $((align)) -ge 16384 ] || bad="$bad $abi:$align"
        done < <("$readelf" -lW "$work/libbridge-$abi.so" | awk '$1 == "LOAD" { print $NF }')
      done
      if [ -z "$bad" ]; then
        pass "16 KB alignment" "every LOAD segment of the 64-bit libbridge.so is 16 KB aligned"
      else
        fail "16 KB alignment" "LOAD alignment below 16 KB:$bad"
      fi
    fi

    # Signer: the owner's upload key, never the debug key.
    signer="$( (keytool -printcert -jarfile "$aab" 2>/dev/null || true) | sed -n 's/^[[:space:]]*SHA256:[[:space:]]*//p' | head -1)"
    expected=""
    [ -f "$signing" ] && expected="$(sed -n 's/^Upload key SHA-256:[[:space:]]*//p' "$signing" | head -1)"
    debug_fp=""
    if [ -f "$HOME/.android/debug.keystore" ]; then
      debug_fp="$(keytool -list -v -keystore "$HOME/.android/debug.keystore" -storepass android \
        -alias androiddebugkey 2>/dev/null | sed -n 's/^[[:space:]]*SHA256:[[:space:]]*//p' | head -1)"
    fi
    if [ -z "$signer" ]; then
      fail signer "the bundle is not signed"
    elif [ -n "$debug_fp" ] && [ "$signer" = "$debug_fp" ]; then
      fail signer "signed with the debug key"
    elif [ -z "$expected" ] || [[ "$expected" == *"<"* ]]; then
      fail signer "no upload key fingerprint in $signing (owner step: record it)"
    elif [ "$signer" = "$expected" ]; then
      pass signer "the owner's upload key ($signer)"
    else
      fail signer "signed by $signer, expected the upload key $expected"
    fi

    # Manifest: permissions, label, version. The bundle's manifest is a
    # protobuf XmlNode (aapt2 cannot read a bundle); this reads it with
    # python3 and prints "element attr=value ..." lines.
    unzip -p "$aab" base/manifest/AndroidManifest.xml >"$work/manifest.pb"
    python3 -I - "$work/manifest.pb" >"$work/manifest.txt" <<'PY'
import sys

def varint(b, i):
    r = s = 0
    while True:
        c = b[i]
        i += 1
        r |= (c & 0x7F) << s
        s += 7
        if c < 0x80:
            return r, i

def fields(b):
    i = 0
    while i < len(b):
        k, i = varint(b, i)
        f, t = k >> 3, k & 7
        if t == 0:
            v, i = varint(b, i)
        elif t == 2:
            n, i = varint(b, i)
            v = b[i:i + n]
            i += n
        elif t == 5:
            v, i = b[i:i + 4], i + 4
        elif t == 1:
            v, i = b[i:i + 8], i + 8
        else:
            raise ValueError("wire type %d" % t)
        yield f, v

def node(b):  # XmlNode: 1 = element
    for f, v in fields(b):
        if f == 1:
            element(v)

def element(b):  # XmlElement: 3 = name, 4 = attribute, 5 = child
    name, attrs, kids = "", [], []
    for f, v in fields(b):
        if f == 3:
            name = v.decode()
        elif f == 4:
            a = {}
            for g, w in fields(v):  # XmlAttribute: 2 = name, 3 = value
                if g in (2, 3):
                    a[g] = w.decode()
            attrs.append("%s=%s" % (a.get(2, ""), a.get(3, "")))
        elif f == 5:
            kids.append(v)
    print(name, *attrs)
    for k in kids:
        node(k)

node(open(sys.argv[1], "rb").read())
PY
    package="$(awk '$1 == "manifest" { for (i = 2; i <= NF; i++) if ($i ~ /^package=/) print substr($i, 9) }' "$work/manifest.txt")"
    # The app's own signature-level permission AndroidX adds for its
    # not-exported receivers is not a permission the user grants.
    perms="$(awk '$1 == "uses-permission" { sub(/^name=/, "", $2); print $2 }' "$work/manifest.txt" |
      grep -vx "$package.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION" | sort | tr '\n' ' ' || true)"
    # push design D12: POST_NOTIFICATIONS (the app), WAKE_LOCK and c2dm RECEIVE
    # (firebase-messaging) for the "New message" wake-up. audio-messages:
    # RECORD_AUDIO, for recording voice messages (record plugin).
    if [ "$perms" = "android.permission.ACCESS_NETWORK_STATE android.permission.INTERNET android.permission.POST_NOTIFICATIONS android.permission.RECORD_AUDIO android.permission.WAKE_LOCK com.google.android.c2dm.permission.RECEIVE " ]; then
      pass permissions "exactly INTERNET, ACCESS_NETWORK_STATE, RECORD_AUDIO (voice messages) and push (POST_NOTIFICATIONS, WAKE_LOCK, c2dm RECEIVE)"
    else
      fail permissions "the bundle asks for: ${perms:-nothing readable}"
    fi
    attr() { # <element> <attribute>
      awk -v e="$1" -v a="$2=" '$1 == e { for (i = 2; i <= NF; i++) if (index($i, a) == 1) { print substr($i, length(a) + 1); exit } }' "$work/manifest.txt"
    }
    code="$(attr manifest versionCode)"
    name="$(attr manifest versionName)"
    if [ "$code" = "$build_number" ] && [ "$name" = "$build_name" ]; then
      pass "android version" "versionName $name, versionCode $code"
    else
      fail "android version" "versionName '$name' versionCode '$code', pubspec says $version"
    fi
    if [ "$(attr application label)" = "Vyajena" ]; then
      pass "android name" "Vyajena"
    else
      fail "android name" "the application label is '$(attr application label)'"
    fi

    # Native libraries are stripped, their symbols kept aside in the bundle
    # (what Flutter's own post-build check looks for).
    unstripped=""
    for lib in libflutter libapp libbridge; do
      grep -q "^BUNDLE-METADATA/com.android.tools.build.debugsymbols/.*/$lib.so.sym$" <<<"$entries" ||
        unstripped="$unstripped $lib"
    done
    if [ -n "$readelf" ] && [ -f "$work/libbridge-arm64-v8a.so" ] &&
      grep -q '\.debug_' <<<"$("$readelf" -SW "$work/libbridge-arm64-v8a.so")"; then
      unstripped="$unstripped libbridge(debug sections)"
    fi
    if [ -z "$unstripped" ]; then
      pass "stripped" "native libraries stripped, symbols in BUNDLE-METADATA"
    else
      fail "stripped" "not stripped:$unstripped"
    fi

    # No tracking SDK in the code.
    sdks='Lcom/google/firebase/analytics|Lcom/google/firebase/crashlytics|Lcom/crashlytics|Lcom/google/android/gms/ads|Lio/sentry|Lcom/bugsnag|Lcom/datadog|Lcom/amplitude|Lcom/mixpanel|Lcom/facebook/appevents'
    found="$(unzip -p "$aab" 'base/dex/*.dex' | LC_ALL=C grep -aoE "$sdks" | sort -u | tr '\n' ' ' || true)"
    if [ -z "$found" ]; then
      pass "no tracking sdk" "no analytics, ads or crash-reporting classes in the bundle"
    else
      fail "no tracking sdk" "found: $found"
    fi
  fi
fi

# --- iOS archive ----------------------------------------------------------------

if [ "$check_ios" = 1 ]; then
  bundle="$archive/Products/Applications/Runner.app"
  if [ ! -d "$bundle" ]; then
    fail "ios archive" "not found: $bundle"
  else
    plist="$bundle/Info.plist"
    value() { plutil -extract "$1" raw -o - "$plist" 2>/dev/null || true; }

    # The cargokit pod links the core as bridge.framework (spike S4).
    core="$bundle/Frameworks/bridge.framework/bridge"
    archs="$(lipo -archs "$bundle/Runner" 2>/dev/null || true) / $(lipo -archs "$core" 2>/dev/null || true)"
    if [ "$archs" = "arm64 / arm64" ]; then
      pass "ios architectures" "Runner and bridge.framework are arm64 only"
    else
      fail "ios architectures" "Runner / bridge.framework are '$archs', expected arm64 only"
    fi
    # Whole output first: grep -q would end the pipe early (SIGPIPE + pipefail).
    exports="$(nm -gU "$core" 2>/dev/null || true)"
    if grep -q 'frb_get_rust_content_hash' <<<"$exports"; then
      pass "ios core linked" "bridge.framework exports the bridge's symbols"
    else
      fail "ios core linked" "no bridge.framework with the bridge's symbols"
    fi

    if [ "$(value ITSAppUsesNonExemptEncryption)" = "false" ]; then
      pass "export compliance" "ITSAppUsesNonExemptEncryption = NO (design D8)"
    else
      fail "export compliance" "ITSAppUsesNonExemptEncryption is not NO (design D8)"
    fi
    usages="$(plutil -convert xml1 -o - "$plist" | sed -n 's/.*<key>\(NS[A-Za-z]*UsageDescription\)<\/key>.*/\1/p' | sort | tr '\n' ' ')"
    # audio-messages: the microphone, for recording voice messages (record
    # plugin). Nothing else: picking a picture uses PHPicker, which needs no
    # photo-library usage description, and there is no in-app camera capture.
    if [ "$usages" = "NSMicrophoneUsageDescription " ]; then
      pass "ios permissions" "microphone only (voice messages)"
    else
      fail "ios permissions" "usage descriptions: ${usages:-none}, expected NSMicrophoneUsageDescription"
    fi
    # The one capability beyond the keychain is Push (push design D12). The
    # archive is unsigned (build-ipa.sh signs at export), so read the source.
    ent="$app/ios/Runner/Runner.entitlements"
    ent_keys="$( [ -f "$ent" ] && plutil -convert json -o - "$ent" | python3 -c 'import json,sys; print(" ".join(sorted(json.load(sys.stdin))))' || true)"
    if [ "$ent_keys" = "aps-environment" ]; then
      pass "ios capabilities" "Push only (aps-environment)"
    else
      fail "ios capabilities" "Runner.entitlements keys: ${ent_keys:-none}"
    fi
    plist_xml="$(plutil -convert xml1 -o - "$plist")"
    if [[ "$plist_xml" == *UIBackgroundModes* ]]; then
      fail "ios background" "UIBackgroundModes is set"
    else
      pass "ios background" "no background modes"
    fi
    if [ "$(value CFBundleDisplayName)" = "Vyajena" ]; then
      pass "ios name" "Vyajena"
    else
      fail "ios name" "CFBundleDisplayName is '$(value CFBundleDisplayName)'"
    fi
    if [ "$(value CFBundleShortVersionString)" = "$build_name" ] && [ "$(value CFBundleVersion)" = "$build_number" ]; then
      pass "ios version" "$build_name ($build_number)"
    else
      fail "ios version" "$(value CFBundleShortVersionString) ($(value CFBundleVersion)), pubspec says $version"
    fi
    if [ -f "$bundle/PrivacyInfo.xcprivacy" ] &&
      grep -q NSPrivacyAccessedAPICategoryFileTimestamp "$bundle/PrivacyInfo.xcprivacy" &&
      grep -q NSPrivacyAccessedAPICategoryDiskSpace "$bundle/PrivacyInfo.xcprivacy"; then
      pass "privacy manifest" "PrivacyInfo.xcprivacy with file timestamp and disk space reasons"
    else
      fail "privacy manifest" "PrivacyInfo.xcprivacy missing or incomplete in Runner.app"
    fi
    store_icon="$app/ios/Runner/Assets.xcassets/AppIcon.appiconset/Icon-App-1024x1024@1x.png"
    if [ "$(sips -g hasAlpha "$store_icon" | awk '/hasAlpha/ { print $2 }')" = "no" ] &&
      [ -f "$bundle/Assets.car" ]; then
      pass "ios icon" "1024 px icon has no alpha; compiled into Assets.car"
    else
      fail "ios icon" "the App Store icon has alpha, or the app has no Assets.car"
    fi
    frameworks="$(find "$bundle/Frameworks" -maxdepth 1 -name '*.framework' -exec basename {} \; 2>/dev/null | grep -iE 'firebase|crashlytics|sentry|googlemobileads|bugsnag|datadog|amplitude|mixpanel|fbsdk' || true)"
    if [ -z "$frameworks" ]; then
      pass "no tracking sdk" "no analytics, ads or crash-reporting frameworks in the app"
    else
      fail "no tracking sdk" "found: $frameworks"
    fi
  fi
fi

if [ "$failed" = 0 ]; then
  echo "release check: all passed"
else
  echo "release check: FAILED (nothing was uploaded)"
fi
exit "$failed"
