# Shared by deploy/relay/{create,deploy,check,ssh}.sh (relay design D7):
# relay.conf, the secrets folder, and SSH to the relay over Google IAP with
# the relay key and its pinned host key. Sourced, not run.
# shellcheck shell=bash

LIB_HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=relay.conf
. "$LIB_HERE/relay.conf"
# The real ports and ranges (change generic-deployment).
# shellcheck source=../network.sh
. "$LIB_HERE/../network.sh"
SECRETS="${RELAY_SECRETS:-$HOME/.vyajena-relay}"
# gcloud never waits on a prompt (an "enable this API?" question hung
# create.sh once); a missing answer fails the command instead.
export CLOUDSDK_CORE_DISABLE_PROMPTS=1

# One SSH master per script run; relay_ssh and relay_scp reuse it.
RELAY_CONTROL="$(mktemp -u "${TMPDIR:-/tmp}/vyajena-relay.XXXXXX")"

# The ssh/scp options for the relay (bash 3.2: no mapfile, so one array).
RELAY_SSH_OPTS=(
  -o "IdentityFile=$SECRETS/ssh_ed25519" -o IdentitiesOnly=yes
  -o "UserKnownHostsFile=$SECRETS/known_hosts" -o "HostKeyAlias=$NAME"
  -o StrictHostKeyChecking=yes -o ConnectTimeout=20
  -o "ProxyCommand=gcloud compute start-iap-tunnel $NAME 22 --listen-on-stdin --project=$PROJECT --zone=$ZONE --verbosity=error"
  -o ServerAliveInterval=10
)

relay_open() {
  [ -s "$SECRETS/known_hosts" ] || { echo "no $SECRETS/known_hosts; run deploy/relay/create.sh" >&2; return 1; }
  ssh "${RELAY_SSH_OPTS[@]}" -o BatchMode=yes -o ControlMaster=yes \
    -o "ControlPath=$RELAY_CONTROL" -fN "$SSH_USER@$NAME"
  trap relay_close EXIT
}

relay_close() {
  ssh -o "ControlPath=$RELAY_CONTROL" -O exit "$SSH_USER@$NAME" 2>/dev/null || true
}

relay_ssh() {
  # shellcheck disable=SC2029 # the command is meant for the relay as given
  ssh "${RELAY_SSH_OPTS[@]}" -o BatchMode=yes -o "ControlPath=$RELAY_CONTROL" "$SSH_USER@$NAME" "$@"
}

relay_scp() { # relay_scp <local files...> <remote dir>
  local dest="${*: -1}"
  scp -q "${RELAY_SSH_OPTS[@]}" -o BatchMode=yes -o "ControlPath=$RELAY_CONTROL" \
    "${@:1:$#-1}" "$SSH_USER@$NAME:$dest"
}

relay_ip() {
  sed -n 's/^RELAY_IP=//p' "$SECRETS/relay.env" 2>/dev/null
}
