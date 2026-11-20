#!/usr/bin/env bash
# shellcheck disable=SC2153 # the ports are set by network.sh
# Installs or upgrades the vyajena2 server on the server over SSH (server-deployment
# design D5). Run on the Mac from anywhere in the repo:
#
#   deploy/server/deploy.sh               build, harden, install, restart
#   deploy/server/deploy.sh --no-build    install target/server/{vyajena-server,rathole} as is
#   deploy/server/deploy.sh --rollback    swap back the previous server binary
#   deploy/server/deploy.sh --reset       back up, then DELETE the group: a fresh
#                                     Server with a new pin (asks for the server id)
#   deploy/server/deploy.sh --reset-review  wipe only the store reviewers' server
#                                     (no backup; asks for its server id)
#
# SERVER=user@host overrides the target (default: SERVER in
# ~/.vyajena-relay/network.conf, deploy/network.sh). The login
# needs passwordless sudo. The server config, the tunnel and its token come
# from ~/.vyajena-relay/ through deploy/relay/render.sh (relay design D5, D6).
# The push keys (fcm-service-account.json, AuthKey_<key id>.p8) come from the
# owner-only ~/.vyajena-push/ (PUSH_DIR overrides) and go only to
# /var/lib/vyajena/push (push design D6); without them pings are only logged.
# Except with --reset, the database and keys in /var/lib/vyajena are never
# touched, so the TLS pin and the users survive every upgrade. A new bootstrap
# invite is saved to an owner-only file in BACKUP_ROOT (~/vyajena-server-backups),
# never printed.
#
# Images (change media-blobs): BLOB_DIR in deploy/server/server.conf, empty by default,
# names the path the owner's drive is mounted at. Set, deploy renders the
# server's [blob_store] from it, lets the service write that path through a
# systemd drop-in and, only if the path is on a mount of its own, gives it to
# the service user (0700). Deploy never mounts anything and never writes blobs
# to the SD card; with the drive absent it warns and the server serves text.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# The real addresses and ports (change generic-deployment).
# shellcheck source=../network.sh
. "$HERE/../network.sh"
readonly SERVER
REPO="$(cd "$HERE/../.." && pwd)"
BINARY="$REPO/target/server/vyajena-server"
RATHOLE="$REPO/target/server/rathole"
RENDER="$REPO/deploy/relay/render.sh"
BACKUP_ROOT="${BACKUP_ROOT:-$HOME/vyajena-server-backups}"
PUSH_DIR="${PUSH_DIR:-$HOME/.vyajena-push}"
readonly HERE REPO BINARY RATHOLE RENDER BACKUP_ROOT PUSH_DIR
# BLOB_DIR: where image blobs live (empty: images off). SERVER_CONF overrides.
# shellcheck source=server.conf
. "${SERVER_CONF:-$HERE/server.conf}"

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

server() {
  ssh -o ControlPath="$CONTROL" "$SERVER" "$@"
}

open_session() {
  ssh -o BatchMode=yes -o ConnectTimeout=10 -o ControlMaster=yes \
    -o ControlPath="$CONTROL" -o ServerAliveInterval=5 -fN "$SERVER" ||
    die "cannot reach $SERVER over SSH"
  trap close_session EXIT
}

close_session() {
  ssh -o ControlPath="$CONTROL" -O exit "$SERVER" 2>/dev/null || true
}

# A brand-new SSH connection, not the open session: proves the server still lets
# the owner in after a staged change.
prove_access() {
  local _
  for _ in $(seq 1 15); do
    if ssh -o BatchMode=yes -o ConnectTimeout=5 -o ControlMaster=no \
      -o ControlPath=none "$SERVER" true 2>/dev/null; then
      return 0
    fi
    sleep 2
  done
  return 1
}

# Applies one lock-out-prone step with harden.sh's automatic undo armed.
staged_step() {
  local staging="$1" step="$2"
  if ! server sudo bash "$staging/harden.sh" needs "$step"; then
    say "$step: already in place"
    return 0
  fi
  say "$step: applying (undone automatically in 2 minutes unless proved)"
  server sudo bash "$staging/harden.sh" stage "$step"
  sleep 3
  if ! prove_access; then
    die "$step: a new SSH connection to $SERVER failed; the undo restores it within 2 minutes. Do not retry before then."
  fi
  server sudo bash "$staging/harden.sh" commit "$step"
}

# --- run on the server as root (sent with declare -f) ----------------------------

# remote_wait_ready <unit> <port>: the unit is active and listens on loopback <port>.
remote_wait_ready() {
  local unit="$1" port="$2" _
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

# Deletes the group (server-deployment: Scripted reset; relay design D9): only
# ever called by remote_install with reset=yes, after deploy.sh made a backup.
# The group's image blobs go too: with the database gone nothing would ever
# delete them. Only files named as blobs are removed (32 hex digits, or a
# leftover temporary one), and only in the blob store's own folder.
remote_wipe() {
  set -euo pipefail
  local blob_dir="${1:-}"
  systemctl stop vyajena-tunnel.service vyajena.service 2>/dev/null || true
  if [ -d /var/lib/vyajena ]; then
    find /var/lib/vyajena -mindepth 1 -delete
  fi
  echo "deleted the database and keys in /var/lib/vyajena" >&2
  if [ -n "$blob_dir" ] && [ -d "$blob_dir" ]; then
    # A shell glob, not find -regex: GNU find (the server) reads \{32\} differently
    # from BSD find, and a reset must really remove the blobs.
    local pat f base
    pat="$(printf '[0-9a-f]%.0s' $(seq 1 32))"
    for f in "$blob_dir"/* "$blob_dir"/.tmp-* "$blob_dir"/.probe-*; do
      # Only plain files (an unmatched glob stays literal and is skipped).
      if [ -f "$f" ] && [ ! -L "$f" ]; then
        base="${f##*/}"
        # shellcheck disable=SC2254 # the unquoted pattern is the point
        case "$base" in
          $pat | .tmp-* | .probe-*) rm -f -- "$f" ;;
        esac
      fi
    done
    echo "deleted the image blobs in $blob_dir" >&2
  fi
}

# The blob store's part of an install (server-deployment: Blob store deployment):
# lets the service write the blob path and gives the path to the service user,
# when that is safe. An unset path removes the drop-in: images are off.
#
# Deploy never mounts a drive and never makes the folder. It chowns and chmods
# the path only when the path is on a mount of its own: if the drive is not
# mounted the path is missing or on the SD card, and handing that to the
# service would let blobs land there. The drop-in's leading "-" lets the
# service start when the path is absent; it picks the path up at its next
# start once the drive is mounted.
remote_blob_store() {
  set -euo pipefail
  # VYAJENA_DROPIN_DIR is only for the tests that run this function; the
  # function is sent over SSH, where it is never set.
  local dir="${1:-}" dropin_dir="${VYAJENA_DROPIN_DIR:-/etc/systemd/system/vyajena.service.d}" target
  local dropin="$dropin_dir/blob-store.conf"
  if [ -z "$dir" ]; then
    rm -f "$dropin"
    rmdir "$dropin_dir" 2>/dev/null || true
    echo "blob store: BLOB_DIR is not set; images are off" >&2
    return 0
  fi
  install -d -m 0755 -o root -g root "$dropin_dir"
  printf '[Service]\n# media-blobs: the blob store'"'"'s folder is the one path besides the\n# state directory the server may write. "-": absent while the drive is.\nReadWritePaths=-%s\n' "$dir" >"$dropin"
  chmod 0644 "$dropin"
  if [ ! -d "$dir" ]; then
    echo "WARN blob store: $dir does not exist (drive not mounted?); images stay unavailable until it is mounted and vyajena.service is restarted" >&2
    return 0
  fi
  target="$(findmnt -n -o TARGET -T "$dir")"
  if [ "$target" = / ]; then
    echo "WARN blob store: $dir is on the main storage, not on a mount of its own; left alone, images stay unavailable" >&2
    return 0
  fi
  if chown vyajena:vyajena "$dir" && chmod 0700 "$dir"; then
    echo "blob store: $dir (on $target) is the service's, mode 0700" >&2
  else
    echo "WARN blob store: cannot give $dir to the service user (is the drive ext4?); images stay unavailable" >&2
  fi
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
# owner vyajena) and removes any the Mac no longer has (server-deployment: Push
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

# Databases from before the server was named generically (change
# generic-deployment) are moved to their new name, with their WAL, once, with the
# services stopped; nothing is copied or deleted. The next restart opens them.
remote_rename_legacy_database() {
  set -euo pipefail
  local legacy="pi.sqlite" current="server.sqlite" dir suffix stopped=no
  for dir in /var/lib/vyajena /var/lib/vyajena-review; do
    if [ -e "$dir/$legacy" ] && [ ! -e "$dir/$current" ]; then
      if [ "$stopped" = no ]; then
        systemctl stop vyajena-tunnel.service vyajena.service vyajena-review.service 2>/dev/null || true
        stopped=yes
      fi
      for suffix in "" -wal -shm; do
        if [ -e "$dir/$legacy$suffix" ]; then mv "$dir/$legacy$suffix" "$dir/$current$suffix"; fi
      done
      echo "renamed $dir/$legacy to $current" >&2
    fi
  done
}

remote_install() {
  set -euo pipefail
  local staging="$1" reset="$2" reset_review="$3" blob_dir="${4:-}"
  local server_port="$5" review_port="$6" egress_port="$7"
  local bin=/usr/local/bin/vyajena-server first=no review_first=no since
  [ -e "$bin" ] || first=yes
  [ -e /var/lib/vyajena-review/keys ] || review_first=yes
  if [ "$reset" = yes ]; then
    remote_wipe "$blob_dir"
    first=yes
  fi
  if [ "$reset_review" = yes ]; then
    remote_wipe_review
    review_first=yes
  fi
  remote_rename_legacy_database
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
  remote_blob_store "$blob_dir"
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
  remote_wait_ready vyajena-egress.service "$egress_port"
  since="$(date +%s)"
  systemctl restart vyajena.service
  remote_wait_ready vyajena.service "$server_port"
  systemctl restart vyajena-review.service
  remote_wait_ready vyajena-review.service "$review_port"
  systemctl enable --quiet vyajena-tunnel.service
  systemctl restart vyajena-tunnel.service
  remote_wait_tunnel
  remote_report "$since" "$first"
  remote_report "$since" "$review_first" vyajena-review.service review
}

remote_rollback() {
  set -euo pipefail
  local server_port="$1" review_port="$2"
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
  remote_wait_ready vyajena.service "$server_port"
  remote_wait_ready vyajena-review.service "$review_port"
  remote_report "$since" no
}

# -----------------------------------------------------------------------------

run_remote() {
  local fn="$1"
  shift
  { declare -f remote_wait_ready remote_wait_tunnel remote_wipe remote_wipe_review remote_report \
      remote_push_keys remote_blob_store remote_rename_legacy_database "$fn"
    printf '%s' "$fn"; printf ' %q' "$@"; echo; } |
    server sudo bash -s
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

# The push keys to send, one per line (server-deployment: Push credentials):
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
    echo "RESET: this deletes the group on $SERVER (users, devices, keys, queued messages)." >&2
    echo "Every device has to re-join with a new invite. A backup is taken first." >&2
  else
    echo "REVIEW RESET: this deletes the store reviewers' server on $SERVER (not the real group)." >&2
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
  "$RENDER" server-config >/dev/null
  "$RENDER" server-tunnel >/dev/null
  "$RENDER" server-review-config >/dev/null
  if [ "$reset_review" = yes ]; then
    confirm_reset "$HERE/review.toml"
  fi
  if [ "$reset" = yes ]; then
    confirm_reset
    say "backing up first"
    env BACKUP_ROOT="$BACKUP_ROOT" SERVER="$SERVER" "$HERE/backup.sh" || die "the backup failed; nothing was deleted"
  fi

  open_session
  if [ "$rollback" = yes ]; then
    say "rolling back to the previous binary on $SERVER"
    run_remote remote_rollback "$SERVER_PORT" "$REVIEW_SERVER_PORT"
    return
  fi

  if [ "$build" = yes ]; then
    say "building"
    "$HERE/build.sh"
  fi
  for f in "$BINARY" "$RATHOLE"; do
    [ -x "$f" ] || die "no $f; run deploy/server/build.sh"
    file -b "$f" | grep -q "ARM aarch64" || die "$f is not an aarch64 binary"
  done

  local staging
  staging="$(server mktemp -d /tmp/vyajena-deploy.XXXXXX)"
  scp -q -o ControlPath="$CONTROL" "$BINARY" "$RATHOLE" \
    "$HERE/vyajena.service" "$HERE/vyajena-review.service" "$SERVER:$staging/"
  # The files that name addresses and ports are rendered from network.conf.
  local f
  for f in harden.sh nftables.conf push.toml vyajena-egress.service; do
    "$RENDER" server-file "$f" | server "cat > '$staging/$f'"
  done
  # The push keys: owner-only in the staging folder, never printed.
  server "umask 077; mkdir '$staging/push'"
  local key
  while IFS= read -r key; do
    [ -n "$key" ] || continue
    server "umask 077; cat > '$staging/push/${key##*/}'" <"$key"
  done <<<"$push_files"
  "$RENDER" server-review-config | server "cat > '$staging/review.toml'"
  # Rendered from ~/.vyajena-relay/; tunnel.toml holds the token.
  "$RENDER" server-config | server "cat > '$staging/config.toml'"
  "$RENDER" server-tunnel | server "umask 077; cat > '$staging/tunnel.toml'"
  "$RENDER" server-tunnel-unit | server "cat > '$staging/vyajena-tunnel.service'"

  say "host setup"
  server sudo bash "$staging/harden.sh" base
  staged_step "$staging" sshd
  staged_step "$staging" firewall
  staged_step "$staging" address

  if [ "$reset" = yes ]; then
    say "resetting: deleting the group, then a fresh vyajena.service and the tunnel"
  else
    say "installing and restarting vyajena.service and vyajena-tunnel.service"
  fi
  run_remote remote_install "$staging" "$reset" "$reset_review" "${BLOB_DIR:-}" \
    "$SERVER_PORT" "$REVIEW_SERVER_PORT" "$EGRESS_PORT" | divert_invite
  server rm -rf "$staging"
  say "done; check with deploy/server/check.sh"
}

main "$@"
