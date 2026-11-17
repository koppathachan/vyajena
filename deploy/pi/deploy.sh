#!/usr/bin/env bash
# Installs or upgrades the vyajena2 server on the Pi over SSH (pi-deployment
# design D5). Run on the Mac from anywhere in the repo:
#
#   deploy/pi/deploy.sh               build, harden, install, restart
#   deploy/pi/deploy.sh --no-build    install target/pi/{vyajena-server,rathole} as is
#   deploy/pi/deploy.sh --rollback    swap back the previous server binary
#   deploy/pi/deploy.sh --reset       back up, then DELETE the group: a fresh
#                                     server with a new pin (asks for the server id)
#   deploy/pi/deploy.sh --reset-review  wipe only the store reviewers' server
#                                     (no backup; asks for its server id)
#
# PI=user@host overrides the target (default vyajan@192.168.1.8). The login
# needs passwordless sudo. The server config, the tunnel and its token come
# from ~/.vyajena-relay/ through deploy/relay/render.sh (relay design D5, D6).
# The push keys (fcm-service-account.json, AuthKey_<key id>.p8) come from the
# owner-only ~/.vyajena-push/ (PUSH_DIR overrides) and go only to
# /var/lib/vyajena/push (push design D6); without them pings are only logged.
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
PUSH_DIR="${PUSH_DIR:-$HOME/.vyajena-push}"
readonly HERE REPO BINARY RATHOLE RENDER BACKUP_ROOT PUSH_DIR

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

# remote_wait_ready [unit port]: the real server (default) or the review one.
remote_wait_ready() {
  local unit="${1:-vyajena.service}" port="${2:-8443}" _
  for _ in $(seq 1 40); do
    if systemctl is-active --quiet "$unit" &&
      ss -tlnH "sport = :$port" | grep -q .; then
      return 0
    fi
    sleep 0.5
  done
  journalctl -u "$unit" -n 30 --no-pager >&2
  echo "$unit is not active and listening on $port" >&2
  return 1
}

# remote_report <since> <first> [unit label]: the pin, and on a first start the
# bootstrap invite line; lines get "<label>: " so deploy.sh can tell the
# servers apart (the review server's are "review: ...").
remote_report() {
  local since="$1" first="$2" unit="${3:-vyajena.service}" label="${4:-}" log
  log="$(journalctl -u "$unit" -o cat --no-pager --since "@$since")"
  {
    grep -m1 "TLS key pin" <<<"$log" | grep -o 'tls_spki_sha256=[0-9a-f]*' || true
    if [ "$first" = yes ]; then
      echo "First install. The bootstrap invite (once; copy the payload to the admin's first device):"
      grep "bootstrap admin invite created" <<<"$log" || echo "(none logged: see journalctl -u $unit)"
    else
      grep "bootstrap invite printed at first start is still valid" <<<"$log" || true
    fi
  } | sed "s/^/${label:+$label: }/"
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

# Wipes only the store reviewers' server (review-server: Review reset): never
# /var/lib/vyajena. Called by remote_install with reset_review=yes.
remote_wipe_review() {
  set -euo pipefail
  systemctl stop vyajena-review.service 2>/dev/null || true
  if [ -d /var/lib/vyajena-review ]; then
    find /var/lib/vyajena-review -mindepth 1 -delete
  fi
  echo "deleted the review server's database and keys in /var/lib/vyajena-review" >&2
}

# Puts the staged push keys into /var/lib/vyajena/push (0700, files 0600,
# owner vyajena) and removes any the Mac no longer has (pi-deployment: Push
# credentials). The review server never gets them.
remote_push_keys() {
  set -euo pipefail
  local staged="$1" dir=/var/lib/vyajena/push f name
  install -d -m 0700 -o vyajena -g vyajena /var/lib/vyajena
  install -d -m 0700 -o vyajena -g vyajena "$dir"
  for f in "$dir"/*; do
    [ -e "$f" ] || continue
    name="${f##*/}"
    if [ ! -e "$staged/$name" ]; then
      rm -f -- "$f"
      echo "removed push key $name (not in the Mac's folder)" >&2
    fi
  done
  for f in "$staged"/*; do
    [ -e "$f" ] || continue
    install -m 0600 -o vyajena -g vyajena "$f" "$dir/${f##*/}"
  done
}

remote_install() {
  set -euo pipefail
  local staging="$1" reset="$2" reset_review="$3" bin=/usr/local/bin/vyajena-server first=no
  local review_first=no since
  [ -e "$bin" ] || first=yes
  [ -e /var/lib/vyajena-review/keys ] || review_first=yes
  if [ "$reset" = yes ]; then
    remote_wipe
    first=yes
  fi
  if [ "$reset_review" = yes ]; then
    remote_wipe_review
    review_first=yes
  fi
  install -m 0644 -o root -g root "$staging/config.toml" /etc/vyajena/config.toml
  install -m 0644 -o root -g root "$staging/push.toml" /etc/vyajena/push.toml
  install -m 0644 -o root -g root "$staging/vyajena-egress.service" /etc/systemd/system/vyajena-egress.service
  remote_push_keys "$staging/push"
  install -m 0644 -o root -g root "$staging/review.toml" /etc/vyajena/review.toml
  install -m 0644 -o root -g root "$staging/vyajena-review.service" /etc/systemd/system/vyajena-review.service
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
  systemctl enable --quiet vyajena-egress.service vyajena.service vyajena-review.service
  systemctl restart vyajena-egress.service
  remote_wait_ready vyajena-egress.service 8445
  since="$(date +%s)"
  systemctl restart vyajena.service
  remote_wait_ready
  systemctl restart vyajena-review.service
  remote_wait_ready vyajena-review.service 8444
  systemctl enable --quiet vyajena-tunnel.service
  systemctl restart vyajena-tunnel.service
  remote_wait_tunnel
  remote_report "$since" "$first"
  remote_report "$since" "$review_first" vyajena-review.service review
}

remote_rollback() {
  set -euo pipefail
  local bin=/usr/local/bin/vyajena-server since
  [ -e "$bin.prev" ] || { echo "no previous binary ($bin.prev)" >&2; return 1; }
  mv -f "$bin" "$bin.rollback"
  mv -f "$bin.prev" "$bin"
  mv -f "$bin.rollback" "$bin.prev"
  # A binary from before push has no --push and no egress-proxy (push design,
  # Rollback): run it without them. The next normal deploy puts them back.
  if ! "$bin" --help 2>/dev/null | grep -q -- '--push'; then
    sed -i 's| --push /etc/vyajena/push.toml||' /etc/systemd/system/vyajena.service
    systemctl disable --now --quiet vyajena-egress.service 2>/dev/null || true
    systemctl daemon-reload
    echo "the previous binary predates push: vyajena.service runs without --push, vyajena-egress is off" >&2
  else
    systemctl restart vyajena-egress.service
  fi
  since="$(date +%s)"
  systemctl restart vyajena.service vyajena-review.service
  remote_wait_ready
  remote_wait_ready vyajena-review.service 8444
  remote_report "$since" no
}

# -----------------------------------------------------------------------------

run_remote() {
  local fn="$1"
  shift
  { declare -f remote_wait_ready remote_wait_tunnel remote_wipe remote_wipe_review remote_report \
      remote_push_keys "$fn"
    printf '%s' "$fn"; printf ' %q' "$@"; echo; } |
    pi sudo bash -s
}

# Prints the deploy report, but saves a bootstrap invite line to an
# owner-only file outside the repo instead of showing it (design D9).
divert_invite() {
  local line file
  while IFS= read -r line; do
    if [[ "$line" == *"bootstrap admin invite created"* ]]; then
      local kind=bootstrap-invite who="real server"
      if [[ "$line" == "review: "* ]]; then
        kind=review-bootstrap-invite
        who="review server"
      fi
      file="$BACKUP_ROOT/$kind-$(date -u +%Y%m%dT%H%M%SZ).txt"
      (umask 077; mkdir -p "$BACKUP_ROOT"; : >>"$file")
      printf '%s\n' "$line" >>"$file"
      echo "$who: bootstrap invite saved to $file (owner only; paste its payload on that server's admin device)"
    else
      printf '%s\n' "$line"
    fi
  done
}

# mode_of <path>: permission bits, on macOS or Linux. GNU form first: on
# Linux `stat -f` means filesystem status and succeeds with the wrong output;
# on macOS `stat -c` fails and the BSD form answers.
mode_of() {
  stat -c '%a' "$1" 2>/dev/null || stat -f '%Lp' "$1"
}

# The push keys to send, one per line (pi-deployment: Push credentials):
# refuses a folder or file open to group or others, more than one .p8, or an
# odd .p8 name. No folder: none (pings are only logged).
push_keys() {
  [ -d "$PUSH_DIR" ] || return 0
  local m f p8=0
  m="$(mode_of "$PUSH_DIR")"
  [ "$m" = 700 ] || die "$PUSH_DIR has mode $m; push credentials need 700 (chmod 700 $PUSH_DIR)"
  for f in "$PUSH_DIR"/*; do
    [ -f "$f" ] || continue
    m="$(mode_of "$f")"
    [ "$m" = 600 ] || [ "$m" = 400 ] || die "$f has mode $m; push credentials need 600 (chmod 600 $f)"
  done
  [ ! -e "$PUSH_DIR/fcm-service-account.json" ] || echo "$PUSH_DIR/fcm-service-account.json"
  for f in "$PUSH_DIR"/AuthKey_*.p8; do
    [ -e "$f" ] || continue
    [[ "${f##*/}" =~ ^AuthKey_[A-Za-z0-9]{10}\.p8$ ]] || die "$f: name it AuthKey_<10-character key id>.p8, as Apple does"
    p8=$((p8 + 1))
    echo "$f"
  done
  [ "$p8" -le 1 ] || die "$PUSH_DIR has $p8 AuthKey_*.p8 files; keep exactly one"
}

# The owner types the server id before anything is deleted.
# confirm_reset [config file]: the real server (default) or the review one.
confirm_reset() {
  local config="${1:-$HERE/config.toml}" id answer
  id="$(sed -n 's/^server_id = "\(.*\)"$/\1/p' "$config")"
  [ -n "$id" ] || die "no server_id in $config"
  if [ "$config" = "$HERE/config.toml" ]; then
    echo "RESET: this deletes the group on $PI (users, devices, keys, queued messages)." >&2
    echo "Every device has to re-join with a new invite. A backup is taken first." >&2
  else
    echo "REVIEW RESET: this deletes the store reviewers' server on $PI (not the real group)." >&2
    echo "No backup: it holds no real data. The review admin re-joins with a new invite." >&2
  fi
  printf 'Type the server id (%s) to go on: ' "$id" >&2
  IFS= read -r answer || true
  [ "$answer" = "$id" ] || die "not confirmed; nothing changed"
}

main() {
  local build=yes rollback=no reset=no reset_review=no
  while [ $# -gt 0 ]; do
    case "$1" in
      --no-build) build=no ;;
      --rollback) rollback=yes ;;
      --reset) reset=yes ;;
      --reset-review) reset_review=yes ;;
      -h | --help)
        sed -n '2,19p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
      *) die "unknown argument $1" ;;
    esac
    shift
  done

  [ "$rollback" = no ] || { [ "$reset" = no ] && [ "$reset_review" = no ]; } ||
    die "--rollback and the resets do not mix"
  case "$(mkdir -p "$BACKUP_ROOT" && cd "$BACKUP_ROOT" && pwd)/" in
    "$REPO"/*) die "BACKUP_ROOT $BACKUP_ROOT is inside the repository; set it elsewhere" ;;
  esac
  # Fail before touching anything if the push keys are open to others or the
  # relay's settings are missing.
  local push_files
  push_files="$(push_keys)" || exit 1
  if [ -z "$push_files" ]; then
    say "no push keys in $PUSH_DIR: wake pings will only be logged"
  fi
  "$RENDER" pi-config >/dev/null
  "$RENDER" pi-tunnel >/dev/null
  "$RENDER" pi-review-config >/dev/null
  if [ "$reset_review" = yes ]; then
    confirm_reset "$HERE/review.toml"
  fi
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
    "$HERE/vyajena.service" "$HERE/vyajena-review.service" "$HERE/nftables.conf" \
    "$HERE/push.toml" "$HERE/vyajena-egress.service" "$PI:$staging/"
  # The push keys: owner-only in the staging folder, never printed.
  pi "umask 077; mkdir '$staging/push'"
  local key
  while IFS= read -r key; do
    [ -n "$key" ] || continue
    pi "umask 077; cat > '$staging/push/${key##*/}'" <"$key"
  done <<<"$push_files"
  "$RENDER" pi-review-config | pi "cat > '$staging/review.toml'"
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
  run_remote remote_install "$staging" "$reset" "$reset_review" | divert_invite
  pi rm -rf "$staging"
  say "done; check with deploy/pi/check.sh"
}

main "$@"
