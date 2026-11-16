#!/usr/bin/env bash
# Makes the relay's secrets on the Mac, once (relay: Relay secrets; design D6):
#
#   deploy/relay/secrets.sh     -> ~/.vyajena-relay/ (0700, files 0600)
#
#   token                the rathole service token (64 hex digits)
#   review-token         the review server's service token (review-server)
#   noise.key noise.pub  the relay's X25519 Noise key pair (base64, raw 32 bytes);
#                        the Pi pins noise.pub
#   ssh_ed25519(.pub)    the only key the relay VM accepts, over IAP
#
# Idempotent: existing files are kept. Prints file names, never their contents.
# Keep a copy of the folder in the password manager. RELAY_SECRETS overrides
# the folder; it must be outside the repository.
set -euo pipefail

readonly SECRETS="${RELAY_SECRETS:-$HOME/.vyajena-relay}"

die() {
  echo "secrets.sh: $*" >&2
  exit 1
}

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
umask 077
mkdir -p "$SECRETS"
chmod 0700 "$SECRETS"
dir="$(cd "$SECRETS" && pwd)"
case "$dir/" in
  "$repo"/*) die "$dir is inside the repository; set RELAY_SECRETS elsewhere" ;;
esac

made=""
if [ ! -s "$dir/token" ]; then
  openssl rand -hex 32 >"$dir/token"
  made="$made token"
fi

if [ ! -s "$dir/review-token" ]; then
  openssl rand -hex 32 >"$dir/review-token"
  made="$made review-token"
fi

if [ ! -s "$dir/noise.key" ]; then
  [ ! -e "$dir/noise.pub" ] || die "noise.pub exists without noise.key; move it away first"
  pem="$(mktemp "$dir/.noise.XXXXXX")"
  openssl genpkey -algorithm X25519 -out "$pem"
  # The last 32 bytes of the DER encodings are the raw keys rathole wants.
  openssl pkey -in "$pem" -outform DER | tail -c 32 | base64 >"$dir/noise.key"
  openssl pkey -in "$pem" -pubout -outform DER | tail -c 32 | base64 >"$dir/noise.pub"
  rm -f "$pem"
  made="$made noise.key noise.pub"
fi
[ -s "$dir/noise.pub" ] || die "noise.key exists without noise.pub"

if [ ! -s "$dir/ssh_ed25519" ]; then
  ssh-keygen -q -t ed25519 -N '' -C vyajena-relay -f "$dir/ssh_ed25519"
  made="$made ssh_ed25519 ssh_ed25519.pub"
fi

find "$dir" -type f -exec chmod 0600 {} +
if [ -n "$made" ]; then
  echo "secrets.sh: made$made in $dir (copy the folder into the password manager)"
else
  echo "secrets.sh: $dir already holds every secret; nothing changed"
fi
