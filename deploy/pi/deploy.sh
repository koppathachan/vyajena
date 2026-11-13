#!/usr/bin/env bash
# Installs or upgrades the vyajena2 server on the Pi over SSH (pi-deployment
# design D5). Run on the Mac from anywhere in the repo:
#
#   deploy/pi/deploy.sh               build, harden, install, restart
#   deploy/pi/deploy.sh --no-build    install target/pi/vyajena-server as is
#   deploy/pi/deploy.sh --rollback    swap back the previous binary
#
# PI=user@host overrides the target (default vyajan@192.168.1.8). The login
# needs passwordless sudo. The database and keys in /var/lib/vyajena are never
# touched, so the TLS pin and the users survive every upgrade.
set -euo pipefail

PI="${PI:-vyajan@192.168.1.8}"
readonly PI
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
BINARY="$REPO/target/pi/vyajena-server"
readonly HERE REPO BINARY

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

remote_install() {
  set -euo pipefail
  local staging="$1" bin=/usr/local/bin/vyajena-server first=no since
  [ -e "$bin" ] || first=yes
  install -m 0644 -o root -g root "$staging/config.toml" /etc/vyajena/config.toml
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
  { declare -f remote_wait_ready remote_report "$fn"; printf '%s' "$fn"; printf ' %q' "$@"; echo; } |
    pi sudo bash -s
}

main() {
  local build=yes rollback=no
  while [ $# -gt 0 ]; do
    case "$1" in
      --no-build) build=no ;;
      --rollback) rollback=yes ;;
      -h | --help)
        sed -n '2,11p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
      *) die "unknown argument $1" ;;
    esac
    shift
  done

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
  [ -x "$BINARY" ] || die "no $BINARY; run deploy/pi/build.sh"
  file -b "$BINARY" | grep -q "ARM aarch64" || die "$BINARY is not an aarch64 binary"

  local staging
  staging="$(pi mktemp -d /tmp/vyajena-deploy.XXXXXX)"
  scp -q -o ControlPath="$CONTROL" "$BINARY" "$HERE/harden.sh" "$HERE/config.toml" \
    "$HERE/vyajena.service" "$HERE/nftables.conf" "$PI:$staging/"

  say "host setup"
  pi sudo bash "$staging/harden.sh" base
  staged_step "$staging" sshd
  staged_step "$staging" firewall
  staged_step "$staging" address

  say "installing and restarting vyajena.service"
  run_remote remote_install "$staging"
  pi rm -rf "$staging"
  say "done; check with deploy/pi/check.sh"
}

main "$@"
