#!/usr/bin/env bash
# Renders a template with the relay's settings and secrets to stdout (relay
# design D6), for the deploy scripts to pipe over SSH. Never redirect it into
# the repository.
#
#   deploy/relay/render.sh relay-server    deploy/relay/server.toml
#   deploy/relay/render.sh server-tunnel       deploy/server/tunnel.toml
#   deploy/relay/render.sh server-config       deploy/server/config.toml
#   deploy/relay/render.sh server-review-config deploy/server/review.toml
#   deploy/relay/render.sh server-tunnel-unit  deploy/server/vyajena-tunnel.service
#   deploy/relay/render.sh server-file <name>  harden.sh, nftables.conf, push.toml or
#                                          vyajena-egress.service, with the network settings
#
# RELAY_SECRETS overrides ~/.vyajena-relay (made by deploy/relay/secrets.sh;
# relay.env, with RELAY_IP, by deploy/relay/create.sh).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly HERE
readonly SECRETS="${RELAY_SECRETS:-$HOME/.vyajena-relay}"
# shellcheck source=relay.conf
. "$HERE/relay.conf"
# The real ports and addresses (change generic-deployment).
# shellcheck source=../network.sh
. "$HERE/../network.sh"
# The server's own settings (BLOB_DIR); SERVER_CONF points at another copy.
# shellcheck source=../server/server.conf
. "${SERVER_CONF:-$HERE/../server/server.conf}"

die() {
  echo "render.sh: $*" >&2
  exit 1
}

# read_value <file> <pattern>: the file's one line, if it matches.
read_value() {
  local file="$SECRETS/$1" value
  [ -f "$file" ] || die "missing $file; run deploy/relay/secrets.sh"
  value="$(tr -d '\n' <"$file")"
  [[ "$value" =~ $2 ]] || die "$file is not in the expected format"
  printf '%s' "$value"
}

relay_ip() {
  local ip
  [ -f "$SECRETS/relay.env" ] || die "missing $SECRETS/relay.env; run deploy/relay/create.sh"
  ip="$(sed -n 's/^RELAY_IP=//p' "$SECRETS/relay.env")"
  [[ "$ip" =~ ^[0-9]{1,3}(\.[0-9]{1,3}){3}$ ]] || die "$SECRETS/relay.env has no RELAY_IP"
  printf '%s' "$ip"
}

readonly KEY='^[A-Za-z0-9+/]{43}=$'
readonly TOKEN='^[0-9a-f]{64}$'

render() {
  local text
  text="$(cat "$1")"
  shift
  while [ $# -gt 0 ]; do
    text="${text//@$1@/$2}"
    shift 2
  done
  if [[ "$text" =~ @[A-Z_]+@ ]]; then die "unfilled ${BASH_REMATCH[0]}"; fi
  printf '%s\n' "$text"
}

# Each value is read into a variable first: a failed read must stop the
# script (set -e does not see a failure inside an argument's $(...)).
case "${1:-}" in
  relay-server)
    key="$(read_value noise.key "$KEY")"
    token="$(read_value token "$TOKEN")"
    review="$(read_value review-token "$TOKEN")"
    [ "$review" != "$token" ] || die "review-token must differ from token"
    render "$HERE/server.toml" TUNNEL_PORT "$TUNNEL_PORT" PUBLIC_PORT "$PUBLIC_PORT" \
      NOISE_PRIVATE_KEY "$key" TOKEN "$token" REVIEW_TOKEN "$review" REVIEW_PORT "$REVIEW_PORT"
    ;;
  server-tunnel)
    ip="$(relay_ip)"
    pub="$(read_value noise.pub "$KEY")"
    token="$(read_value token "$TOKEN")"
    review="$(read_value review-token "$TOKEN")"
    render "$HERE/../server/tunnel.toml" RELAY_IP "$ip" TUNNEL_PORT "$TUNNEL_PORT" \
      SERVER_PORT "$SERVER_PORT" REVIEW_SERVER_PORT "$REVIEW_SERVER_PORT" \
      NOISE_PUBLIC_KEY "$pub" TOKEN "$token" REVIEW_TOKEN "$review"
    ;;
  server-config)
    [[ "$RELAY_HOST" =~ ^[a-z0-9]([a-z0-9.-]*[a-z0-9])?$ ]] || die "RELAY_HOST in relay.conf is not a DNS name"
    render "$HERE/../server/config.toml" RELAY_HOST "$RELAY_HOST" PUBLIC_PORT "$PUBLIC_PORT" \
      SERVER_PORT "$SERVER_PORT" ADMIN_USERNAME "$ADMIN_USERNAME"
    # The blob store (server-deployment: Blob store deployment): only with a
    # BLOB_DIR, so a server without one has no [blob_store] and images are off.
    if [ -n "${BLOB_DIR:-}" ]; then
      [[ "$BLOB_DIR" =~ ^/[A-Za-z0-9._/-]*[A-Za-z0-9_-]$ ]] || die "BLOB_DIR in server.conf is not a plain absolute path"
      case "$BLOB_DIR/" in
        /var/lib/vyajena/* | /var/lib/vyajena-review/* | /home/* | /root/* | /etc/* | /usr/* | /tmp/*)
          die "BLOB_DIR $BLOB_DIR is where the service cannot use it or deploy and backup touch" ;;
      esac
      [[ "$BLOB_DIR" != *..* ]] || die "BLOB_DIR in server.conf has a .. in it"
      printf '\n# Image blobs (change media-blobs): the drive'"'"'s stable path, from server.conf.\n[blob_store]\nbackend = "filesystem"\npath = "%s"\n' "$BLOB_DIR"
    fi
    ;;
  server-review-config)
    [[ "$RELAY_HOST" =~ ^[a-z0-9]([a-z0-9.-]*[a-z0-9])?$ ]] || die "RELAY_HOST in relay.conf is not a DNS name"
    render "$HERE/../server/review.toml" RELAY_HOST "$RELAY_HOST" REVIEW_PORT "$REVIEW_PORT" \
      REVIEW_SERVER_PORT "$REVIEW_SERVER_PORT"
    ;;
  server-tunnel-unit)
    ip="$(relay_ip)"
    render "$HERE/../server/vyajena-tunnel.service" RELAY_IP "$ip"
    ;;
  server-file)
    # A host-side template with the real addresses and ports (change
    # generic-deployment): only the files that name them.
    case "${2:-}" in
      harden.sh | nftables.conf | push.toml | vyajena-egress.service) ;;
      *) die "server-file: not a template with network settings: ${2:-}" ;;
    esac
    render "$HERE/../server/$2" ADDRESS "$SERVER_ADDRESS" HOME_LAN "$HOME_LAN" ROUTER "$ROUTER" \
      EGRESS_PORT "$EGRESS_PORT"
    ;;
  *) die "usage: render.sh relay-server | server-tunnel | server-config | server-review-config | server-tunnel-unit | server-file <template>" ;;
esac
