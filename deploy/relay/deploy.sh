#!/usr/bin/env bash
# Hardens the relay VM and installs or upgrades rathole on it over Google IAP
# (relay: Provision and deploy the relay; design D8). Run on the Mac after
# deploy/relay/create.sh, with a valid `gcloud auth login`:
#
#   deploy/relay/deploy.sh               build, harden, install, restart
#   deploy/relay/deploy.sh --no-build    install target/relay/rathole as is
#   deploy/relay/deploy.sh --rollback    swap back the previous rathole
#
# The config (with the token and the Noise private key) is rendered from
# ~/.vyajena-relay/ and piped over SSH into a root-only file; nothing secret
# is written on the Mac. Restarting rathole drops the tunnel for a few seconds;
# the server reattaches by itself.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly HERE
# shellcheck source=lib.sh
. "$HERE/lib.sh"
BINARY="$(cd "$HERE/../.." && pwd)/target/relay/rathole"
readonly BINARY

die() {
  echo "deploy.sh: $*" >&2
  exit 1
}

say() {
  echo "==> $*" >&2
}

# --- run on the relay as root (sent with declare -f) -------------------------

remote_harden() {
  set -euo pipefail
  # SSH: keys only, no root (IAP already limits who reaches port 22).
  local dropin=/etc/ssh/sshd_config.d/00-vyajena.conf
  cat >"$dropin.new" <<'CONF'
# vyajena2 (deploy/relay/deploy.sh): keys only, no root (relay: Relay host
# hardening). Read before the other drop-ins, so these values win.
PermitRootLogin no
PasswordAuthentication no
KbdInteractiveAuthentication no
PubkeyAuthentication yes
CONF
  chmod 0644 "$dropin.new"
  if ! cmp -s "$dropin.new" "$dropin"; then
    mv -f "$dropin.new" "$dropin"
    sshd -t || { rm -f "$dropin"; echo "sshd -t refused the drop-in" >&2; return 1; }
    systemctl reload ssh
  else
    rm -f "$dropin.new"
  fi

  # Security updates unattended, never an automatic reboot.
  if ! dpkg-query -W -f '${Status}' unattended-upgrades 2>/dev/null | grep -q "ok installed"; then
    apt-get update -qq
    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq unattended-upgrades >/dev/null
  fi
  cat >/etc/apt/apt.conf.d/20auto-upgrades <<'CONF'
APT::Periodic::Update-Package-Lists "1";
APT::Periodic::Unattended-Upgrade "1";
CONF
  cat >/etc/apt/apt.conf.d/51vyajena-unattended-upgrades <<'CONF'
// vyajena2 (deploy/relay/deploy.sh): security updates without automatic reboots.
Unattended-Upgrade::Automatic-Reboot "false";
CONF
  systemctl enable --now apt-daily.timer apt-daily-upgrade.timer >/dev/null

  # Logs in RAM only: the relay keeps no state (design D8).
  install -d -m 0755 /etc/systemd/journald.conf.d
  cat >/etc/systemd/journald.conf.d/50-vyajena-relay.conf <<'CONF'
# vyajena2 (deploy/relay/deploy.sh): the relay keeps no logs across a restart.
[Journal]
Storage=volatile
RuntimeMaxUse=32M
CONF
  if [ -d /var/log/journal ]; then
    systemctl restart systemd-journald
    rm -rf /var/log/journal
  fi
  # Nothing but SSH and rathole on a public interface: resolved's LLMNR and
  # mDNS listen on every interface (UDP/TCP 5355) by default.
  install -d -m 0755 /etc/systemd/resolved.conf.d
  cat >/etc/systemd/resolved.conf.d/50-vyajena-relay.conf <<'CONF'
# vyajena2 (deploy/relay/deploy.sh): no LLMNR or mDNS listeners.
[Resolve]
LLMNR=no
MulticastDNS=no
CONF
  if systemctl is-active --quiet systemd-resolved.service; then
    systemctl restart systemd-resolved.service
  fi

  # A syslog daemon would write a second, persistent copy.
  if systemctl is-enabled --quiet rsyslog.service 2>/dev/null; then
    systemctl disable --now rsyslog.service >/dev/null 2>&1 || true
  fi
}

remote_wait_ready() {
  local port="$1" _
  for _ in $(seq 1 40); do
    if systemctl is-active --quiet vyajena-relay.service &&
      ss -tlnH "sport = :$port" | grep -q .; then
      return 0
    fi
    sleep 0.5
  done
  journalctl -u vyajena-relay.service -n 30 --no-pager >&2
  echo "vyajena-relay.service is not active and listening on $port" >&2
  return 1
}

remote_install() {
  set -euo pipefail
  local staging="$1" port="$2" bin=/usr/local/bin/rathole
  install -d -m 0700 -o root -g root /etc/vyajena-relay
  install -m 0600 -o root -g root "$staging/server.toml" /etc/vyajena-relay/server.toml
  install -m 0644 -o root -g root "$staging/vyajena-relay.service" /etc/systemd/system/vyajena-relay.service
  install -m 0755 -o root -g root "$staging/rathole" "$bin.new"
  if [ ! -e "$bin" ]; then
    mv "$bin.new" "$bin"
  elif cmp -s "$bin" "$bin.new"; then
    rm -f "$bin.new"
  else
    mv -f "$bin" "$bin.prev"
    mv "$bin.new" "$bin"
  fi
  systemctl daemon-reload
  systemctl enable --quiet vyajena-relay.service
  systemctl restart vyajena-relay.service
  remote_wait_ready "$port"
  "$bin" --version
}

remote_rollback() {
  set -euo pipefail
  local port="$1" bin=/usr/local/bin/rathole
  [ -e "$bin.prev" ] || { echo "no previous binary ($bin.prev)" >&2; return 1; }
  mv -f "$bin" "$bin.rollback"
  mv -f "$bin.prev" "$bin"
  mv -f "$bin.rollback" "$bin.prev"
  systemctl restart vyajena-relay.service
  remote_wait_ready "$port"
  "$bin" --version
}

# -----------------------------------------------------------------------------

run_remote() {
  local fn="$1"
  shift
  { declare -f remote_wait_ready "$fn"; printf '%s' "$fn"; printf ' %q' "$@"; echo; } |
    relay_ssh sudo bash -s
}

main() {
  local build=yes rollback=no
  while [ $# -gt 0 ]; do
    case "$1" in
      --no-build) build=no ;;
      --rollback) rollback=yes ;;
      -h | --help) sed -n '2,14p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
      *) die "unknown argument $1" ;;
    esac
    shift
  done

  relay_open || die "cannot reach $NAME over IAP (gcloud auth login? create.sh run?)"
  if [ "$rollback" = yes ]; then
    say "rolling back rathole on $NAME"
    run_remote remote_rollback "$TUNNEL_PORT"
    return
  fi

  if [ "$build" = yes ]; then
    say "building rathole $RATHOLE_VERSION (amd64)"
    "$HERE/build.sh" amd64
  fi
  [ -x "$BINARY" ] || die "no $BINARY; run deploy/relay/build.sh"
  file -b "$BINARY" | grep -q "x86-64" || die "$BINARY is not an x86-64 binary"

  local staging
  staging="$(relay_ssh mktemp -d /tmp/vyajena-relay.XXXXXX)"
  relay_scp "$BINARY" "$HERE/vyajena-relay.service" "$staging/"
  "$HERE/render.sh" relay-server | relay_ssh "umask 077; cat > '$staging/server.toml'"

  say "hardening $NAME"
  run_remote remote_harden
  # A new connection (not the open master) proves SSH still lets us in.
  ssh "${RELAY_SSH_OPTS[@]}" -o BatchMode=yes -o ControlPath=none "$SSH_USER@$NAME" true ||
    die "a new SSH connection failed after hardening; recreate the VM (it holds no state)"

  say "installing rathole and restarting vyajena-relay.service"
  run_remote remote_install "$staging" "$TUNNEL_PORT"
  relay_ssh rm -rf "$staging"
  say "done; check with deploy/relay/check.sh"
}

main "$@"
