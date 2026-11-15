#!/usr/bin/env bash
# Installs or upgrades the vyajena2 server on the Pi over SSH (pi-deployment
# design D5). Run on the Mac from anywhere in the repo:
#
#   deploy/pi/deploy.sh               build, harden, install, restart
#   deploy/pi/deploy.sh --no-build    install target/pi/{vyajena-server,rathole} as is
#   deploy/pi/deploy.sh --rollback    swap back the previous server binary
#   deploy/pi/deploy.sh --reset       back up, then DELETE the group: a fresh
#                                     server with a new pin (asks for the server id)
#
# PI=user@host overrides the target (default vyajan@192.168.1.8). The login
# needs passwordless sudo. The server config, the tunnel and its token come
# from ~/.vyajena-relay/ through deploy/relay/render.sh (relay design D5, D6).
# Except with --reset, the database and keys in /var/lib/vyajena are never
# touched, so the TLS pin and the users survive every upgrade. A new bootstrap
# invite is saved to an owner-only file in BACKUP_ROOT (~/vyajena-pi-backups),
# never printed.
set -euo pipefail

PI="${PI:-vyajan@192.168.1.8}"
readonly PI
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
BINARY="$REPO/target/pi/vyajena-server"
RATHOLE="$REPO/target/pi/rathole"
RENDER="$REPO/deploy/relay/render.sh"
BACKUP_ROOT="${BACKUP_ROOT:-$HOME/vyajena-pi-backups}"
readonly HERE REPO BINARY RATHOLE RENDER BACKUP_ROOT

die() {
  echo "deploy.sh: $*" >&2
  exit 1
}

say() {
  echo "==> $*" >&2
}

# One SSH session stays open for the whole deploy; staged changes go through
# it, and access is proved with new connections (prove_access).
CONTROL="$(mktemp -u "${TMPDIR:-/tmp}/vyajena-deploy.XXXXXX")"
readonly CONTROL

pi() {
  ssh -o ControlPath="$CONTROL" "$PI" "$@"
}

open_session() {
  ssh -o BatchMode=yes -o ConnectTimeout=10 -o ControlMaster=yes \
    -o ControlPath="$CONTROL" -o ServerAliveInterval=5 -fN "$PI" ||
    die "cannot reach $PI over SSH"
  trap close_session EXIT
}

close_session() {
  ssh -o ControlPath="$CONTROL" -O exit "$PI" 2>/dev/null || true
}

# A brand-new SSH connection, not the open session: proves the Pi still lets
# the owner in after a staged change.
prove_access() {
  local _
  for _ in $(seq 1 15); do
    if ssh -o BatchMode=yes -o ConnectTimeout=5 -o ControlMaster=no \
      -o ControlPath=none "$PI" true 2>/dev/null; then
      return 0
    fi
    sleep 2
  done
  return 1
}

# Applies one lock-out-prone step with harden.sh's automatic undo armed.
staged_step() {
  local staging="$1" step="$2"
  if ! pi sudo bash "$staging/harden.sh" needs "$step"; then
    say "$step: already in place"
    return 0
  fi
  say "$step: applying (undone automatically in 2 minutes unless proved)"
  pi sudo bash "$staging/harden.sh" stage "$step"
  sleep 3
  if ! prove_access; then
    die "$step: a new SSH connection to $PI failed; the undo restores it within 2 minutes. Do not retry before then."
  fi
  pi sudo bash "$staging/harden.sh" commit "$step"
}

# --- run on the Pi as root (sent with declare -f) ----------------------------

remote_wait_ready() {
  local _
  for _ in $(seq 1 40); do
    if systemctl is-active --quiet vyajena.service &&
      ss -tlnH 'sport = :8443' | grep -q .; then
      return 0
    fi
    sleep 0.5
  done
  journalctl -u vyajena.service -n 30 --no-pager >&2
  echo "vyajena.service is not active and listening on 8443" >&2
  return 1
}

remote_report() {
  local since="$1" first="$2" log
  log="$(journalctl -u vyajena.service -o cat --no-pager --since "@$since")"
  grep -m1 "TLS key pin" <<<"$log" | grep -o 'tls_spki_sha256=[0-9a-f]*' || true
  if [ "$first" = yes ]; then
    echo "First install. The bootstrap invite (once; copy the payload to the admin's first device):"
    grep "bootstrap admin invite created" <<<"$log" || echo "(none logged: see journalctl -u vyajena)"
  else
    grep "bootstrap invite printed at first start is still valid" <<<"$log" || true
  fi
}

remote_wait_tunnel() {
  local _
  for _ in $(seq 1 20); do
    systemctl is-active --quiet vyajena-tunnel.service && return 0
    sleep 0.5
  done
  journalctl -u vyajena-tunnel.service -n 30 --no-pager >&2
  echo "vyajena-tunnel.service is not active" >&2
  return 1
}

# Deletes the group (pi-deployment: Scripted reset; relay design D9): only
# ever called by remote_install with reset=yes, after deploy.sh made a backup.
remote_wipe() {
  set -euo pipefail
  systemctl stop vyajena-tunnel.service vyajena.service 2>/dev/null || true
  if [ -d /var/lib/vyajena ]; then
    find /var/lib/vyajena -mindepth 1 -delete
  fi
  echo "deleted the database and keys in /var/lib/vyajena" >&2
}

remote_install() {
  set -euo pipefail
  local staging="$1" reset="$2" bin=/usr/local/bin/vyajena-server first=no since
  [ -e "$bin" ] || first=yes
  if [ "$reset" = yes ]; then
    remote_wipe
    first=yes
  fi
  install -m 0644 -o root -g root "$staging/config.toml" /etc/vyajena/config.toml
  install -m 0600 -o root -g root "$staging/tunnel.toml" /etc/vyajena/tunnel.toml
  install -m 0644 -o root -g root "$staging/vyajena-tunnel.service" /etc/systemd/system/vyajena-tunnel.service
  install -m 0755 -o root -g root "$staging/rathole" /usr/local/bin/rathole
  install -m 0644 -o root -g root "$staging/vyajena.service" /etc/systemd/system/vyajena.service
  install -m 0755 -o root -g root "$staging/vyajena-server" "$bin.new"
  if [ "$first" = yes ]; then
    mv "$bin.new" "$bin"
  elif cmp -s "$bin" "$bin.new"; then
    # The same build again: keep the real previous binary for rollback.
    rm -f "$bin.new"
  else
    mv -f "$bin" "$bin.prev"
    mv "$bin.new" "$bin"
  fi
  systemctl daemon-reload
  systemctl enable --quiet vyajena.service
  since="$(date +%s)"
  systemctl restart vyajena.service
  remote_wait_ready
  systemctl enable --quiet vyajena-tunnel.service
  systemctl restart vyajena-tunnel.service
  remote_wait_tunnel
  remote_report "$since" "$first"
}

remote_rollback() {
  set -euo pipefail
  local bin=/usr/local/bin/vyajena-server since
  [ -e "$bin.prev" ] || { echo "no previous binary ($bin.prev)" >&2; return 1; }
  mv -f "$bin" "$bin.rollback"
  mv -f "$bin.prev" "$bin"
  mv -f "$bin.rollback" "$bin.prev"
  since="$(date +%s)"
  systemctl restart vyajena.service
  remote_wait_ready
  remote_report "$since" no
}

# -----------------------------------------------------------------------------

run_remote() {
  local fn="$1"
  shift
  { declare -f remote_wait_ready remote_wait_tunnel remote_wipe remote_report "$fn"
    printf '%s' "$fn"; printf ' %q' "$@"; echo; } |
    pi sudo bash -s
}

# Prints the deploy report, but saves a bootstrap invite line to an
# owner-only file outside the repo instead of showing it (design D9).
divert_invite() {
  local line file=""
  while IFS= read -r line; do
    if [[ "$line" == *"bootstrap admin invite created"* ]]; then
      if [ -z "$file" ]; then
        (umask 077; mkdir -p "$BACKUP_ROOT")
        file="$BACKUP_ROOT/bootstrap-invite-$(date -u +%Y%m%dT%H%M%SZ).txt"
        (umask 077; : >"$file")
      fi
      printf '%s\n' "$line" >>"$file"
      echo "bootstrap invite saved to $file (owner only; paste its payload on the admin's first device)"
    else
      printf '%s\n' "$line"
    fi
  done
}

# The owner types the server id before anything is deleted.
confirm_reset() {
  local id answer
  id="$(sed -n 's/^server_id = "\(.*\)"$/\1/p' "$HERE/config.toml")"
  [ -n "$id" ] || die "no server_id in $HERE/config.toml"
  echo "RESET: this deletes the group on $PI (users, devices, keys, queued messages)." >&2
  echo "Every device has to re-join with a new invite. A backup is taken first." >&2
  printf 'Type the server id (%s) to go on: ' "$id" >&2
  IFS= read -r answer || true
  [ "$answer" = "$id" ] || die "not confirmed; nothing changed"
}

main() {
  local build=yes rollback=no reset=no
  while [ $# -gt 0 ]; do
    case "$1" in
      --no-build) build=no ;;
      --rollback) rollback=yes ;;
      --reset) reset=yes ;;
      -h | --help)
        sed -n '2,17p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
      *) die "unknown argument $1" ;;
    esac
    shift
  done

  [ "$rollback" = no ] || [ "$reset" = no ] || die "--rollback and --reset do not mix"
  case "$(mkdir -p "$BACKUP_ROOT" && cd "$BACKUP_ROOT" && pwd)/" in
    "$REPO"/*) die "BACKUP_ROOT $BACKUP_ROOT is inside the repository; set it elsewhere" ;;
  esac
  # Fail before touching anything if the relay's settings are missing.
  "$RENDER" pi-config >/dev/null
  "$RENDER" pi-tunnel >/dev/null
  if [ "$reset" = yes ]; then
    confirm_reset
    say "backing up first"
    env BACKUP_ROOT="$BACKUP_ROOT" PI="$PI" "$HERE/backup.sh" || die "the backup failed; nothing was deleted"
  fi

  open_session
  if [ "$rollback" = yes ]; then
    say "rolling back to the previous binary on $PI"
    run_remote remote_rollback
    return
  fi

  if [ "$build" = yes ]; then
    say "building"
    "$HERE/build.sh"
  fi
  local f
  for f in "$BINARY" "$RATHOLE"; do
    [ -x "$f" ] || die "no $f; run deploy/pi/build.sh"
    file -b "$f" | grep -q "ARM aarch64" || die "$f is not an aarch64 binary"
  done

  local staging
  staging="$(pi mktemp -d /tmp/vyajena-deploy.XXXXXX)"
  scp -q -o ControlPath="$CONTROL" "$BINARY" "$RATHOLE" "$HERE/harden.sh" \
    "$HERE/vyajena.service" "$HERE/nftables.conf" "$PI:$staging/"
  # Rendered from ~/.vyajena-relay/; tunnel.toml holds the token.
  "$RENDER" pi-config | pi "cat > '$staging/config.toml'"
  "$RENDER" pi-tunnel | pi "umask 077; cat > '$staging/tunnel.toml'"
  "$RENDER" pi-tunnel-unit | pi "cat > '$staging/vyajena-tunnel.service'"

  say "host setup"
  pi sudo bash "$staging/harden.sh" base
  staged_step "$staging" sshd
  staged_step "$staging" firewall
  staged_step "$staging" address

  if [ "$reset" = yes ]; then
    say "resetting: deleting the group, then a fresh vyajena.service and the tunnel"
  else
    say "installing and restarting vyajena.service and vyajena-tunnel.service"
  fi
  run_remote remote_install "$staging" "$reset" | divert_invite
  pi rm -rf "$staging"
  say "done; check with deploy/pi/check.sh"
}

main "$@"
