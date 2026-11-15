#!/usr/bin/env bash
# A shell (or one command) on the relay over Google IAP (relay design D7):
#
#   deploy/relay/ssh.sh                 interactive shell as vyajena
#   deploy/relay/ssh.sh sudo journalctl -u vyajena-relay -n 50
#
# Needs a valid `gcloud auth login`; RELAY_SECRETS overrides ~/.vyajena-relay.
set -euo pipefail
# shellcheck source=lib.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"
exec ssh "${RELAY_SSH_OPTS[@]}" -o ControlPath=none -t "$SSH_USER@$NAME" "$@"
