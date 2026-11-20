# Loads the deployment's real network settings (change generic-deployment).
# Sourced, not run, by deploy/relay/*.sh and deploy/server/*.sh. The values are
# in ~/.vyajena-relay/network.conf (RELAY_SECRETS overrides the folder), made
# from deploy/network.conf.example: the repository never holds the real ones.
# SERVER=user@host in the environment overrides the file's SERVER.
# shellcheck shell=bash

NETWORK_CONF="${RELAY_SECRETS:-$HOME/.vyajena-relay}/network.conf"
if [ ! -f "$NETWORK_CONF" ]; then
  echo "no $NETWORK_CONF: copy deploy/network.conf.example there and set your own values" >&2
  exit 1
fi
_env_server="${SERVER:-}"
# shellcheck source=network.conf.example
. "$NETWORK_CONF"
[ -z "$_env_server" ] || SERVER="$_env_server"
unset _env_server
for _key in SERVER HOME_LAN ROUTER ADMIN_USERNAME SERVER_PORT REVIEW_SERVER_PORT EGRESS_PORT \
  TUNNEL_PORT REVIEW_PORT SUBNET_RANGE IAP_RANGE; do
  if [ -z "${!_key:-}" ]; then
    echo "$NETWORK_CONF has no $_key (see deploy/network.conf.example)" >&2
    exit 1
  fi
done
unset _key
# shellcheck disable=SC2034 # used by the scripts that source this file
SERVER_ADDRESS="${SERVER#*@}"
