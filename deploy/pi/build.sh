#!/usr/bin/env bash
# Builds the Pi server for aarch64 Linux on the Mac (pi-deployment design D2):
# a release build in a linux/arm64 Debian bookworm container, so the binary
# needs only glibc 2.34+ and the Pi needs no toolchain.
#
#   deploy/pi/build.sh        -> target/pi/vyajena-server
set -euo pipefail

# Same Rust as the Mac; bookworm's glibc (2.36) is older than the Pi's (2.41).
readonly IMAGE="rust:1.96-bookworm"
# The Docker VM has 8 GB; more parallel jobs ran it out of memory once.
readonly JOBS=4

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out="$repo/target/pi/vyajena-server"

if ! docker info >/dev/null 2>&1; then
  echo "Starting Docker Desktop..." >&2
  open -a Docker
  for _ in $(seq 1 60); do
    docker info >/dev/null 2>&1 && break
    sleep 2
  done
  docker info >/dev/null 2>&1 || { echo "Docker is not running" >&2; exit 1; }
fi

docker run --rm --platform linux/arm64 \
  -v "$repo":/work -w /work \
  -v vyajena-cargo-registry:/usr/local/cargo/registry \
  -e CARGO_TARGET_DIR=/work/target/linux-arm64 \
  "$IMAGE" \
  cargo build --release --locked -p server -j "$JOBS"

mkdir -p "$(dirname "$out")"
install -m 0755 "$repo/target/linux-arm64/release/server" "$out"
file "$out"
shasum -a 256 "$out"
