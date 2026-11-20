#!/usr/bin/env bash
# Builds the pinned rathole from its crates.io source (relay design D3) in a
# Linux container on the Mac, with rathole's own Cargo.lock (--locked):
#
#   deploy/relay/build.sh           linux/amd64 for the relay -> target/relay/rathole
#   deploy/relay/build.sh arm64     linux/arm64 for the server    -> target/server/rathole
#
# deploy/server/build.sh runs the arm64 build itself.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly HERE
# shellcheck source=relay.conf
. "$HERE/relay.conf"
repo="$(cd "$HERE/../.." && pwd)"

case "${1:-amd64}" in
  amd64) arch=amd64 out="$repo/target/relay/rathole" ;;
  arm64) arch=arm64 out="$repo/target/server/rathole" ;;
  *) echo "usage: build.sh [amd64|arm64]" >&2; exit 1 ;;
esac

if ! docker info >/dev/null 2>&1; then
  echo "Starting Docker Desktop..." >&2
  open -a Docker
  for _ in $(seq 1 60); do
    docker info >/dev/null 2>&1 && break
    sleep 2
  done
  docker info >/dev/null 2>&1 || { echo "Docker is not running" >&2; exit 1; }
fi

docker run --rm --platform "linux/$arch" \
  -v "$repo/target":/work/target \
  -v vyajena-cargo-registry:/usr/local/cargo/registry \
  -e CARGO_TARGET_DIR="/work/target/rathole-build-$arch" \
  "$RATHOLE_IMAGE" \
  cargo install "rathole@$RATHOLE_VERSION" --locked --no-default-features \
  --features "$RATHOLE_FEATURES" --root "/work/target/rathole-$arch" --force -j 4

mkdir -p "$(dirname "$out")"
install -m 0755 "$repo/target/rathole-$arch/bin/rathole" "$out"
file "$out"
shasum -a 256 "$out"
