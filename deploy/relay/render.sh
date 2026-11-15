#!/usr/bin/env bash
# Renders a template with the relay's settings and secrets to stdout (relay
# design D6), for the deploy scripts to pipe over SSH. Never redirect it into
# the repository.
#
#   deploy/relay/render.sh relay-server    deploy/relay/server.toml
#   deploy/relay/render.sh pi-tunnel       deploy/pi/tunnel.toml
#   deploy/relay/render.sh pi-config       deploy/pi/config.toml
#   deploy/relay/render.sh pi-tunnel-unit  deploy/pi/vyajena-tunnel.service
#
# RELAY_SECRETS overrides ~/.vyajena-relay (made by deploy/relay/secrets.sh;
# relay.env, with RELAY_IP, by deploy/relay/create.sh).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly HERE
readonly SECRETS="${RELAY_SECRETS:-$HOME/.vyajena-relay}"
# shellcheck source=relay.conf
. "$HERE/relay.conf"

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
    render "$HERE/server.toml" TUNNEL_PORT "$TUNNEL_PORT" PUBLIC_PORT "$PUBLIC_PORT" \
      NOISE_PRIVATE_KEY "$key" TOKEN "$token"
    ;;
  pi-tunnel)
    ip="$(relay_ip)"
    pub="$(read_value noise.pub "$KEY")"
    token="$(read_value token "$TOKEN")"
    render "$HERE/../pi/tunnel.toml" RELAY_IP "$ip" TUNNEL_PORT "$TUNNEL_PORT" \
      NOISE_PUBLIC_KEY "$pub" TOKEN "$token"
    ;;
  pi-config)
    [[ "$RELAY_HOST" =~ ^[a-z0-9]([a-z0-9.-]*[a-z0-9])?$ ]] || die "RELAY_HOST in relay.conf is not a DNS name"
    render "$HERE/../pi/config.toml" RELAY_HOST "$RELAY_HOST" PUBLIC_PORT "$PUBLIC_PORT"
    ;;
  pi-tunnel-unit)
    ip="$(relay_ip)"
    render "$HERE/../pi/vyajena-tunnel.service" RELAY_IP "$ip"
    ;;
  *) die "usage: render.sh relay-server | pi-tunnel | pi-config | pi-tunnel-unit" ;;
esac
