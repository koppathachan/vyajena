#!/usr/bin/env bash
# One-time host setup on the Pi (pi-deployment design D1, D4), run as root by
# deploy.sh from a staging folder that also holds nftables.conf. Idempotent.
#
#   harden.sh base                  service user, mDNS off, unattended upgrades
#   harden.sh needs <step>          exit 0 if <step> still has to be applied
#   harden.sh stage <step>          arm an automatic undo, then apply <step>
#   harden.sh commit <step>         cancel the undo and keep <step>
#
# Steps that can lock the owner out (sshd, firewall, address) are staged: the
# undo runs by itself after UNDO_AFTER unless deploy.sh, having proved with a
# NEW SSH connection from the Mac that the Pi is still reachable, commits.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly HERE
readonly UNDO_AFTER="120s"
readonly SSHD_DROPIN="/etc/ssh/sshd_config.d/00-vyajena.conf"
# The Pi's static address on eth0 (design D4); invites name it.
readonly IFACE="eth0"
readonly ADDRESS="192.168.1.8"

die() {
  echo "harden.sh: $*" >&2
  exit 1
}

log() {
  echo "harden.sh: $*" >&2
}

base() {
  if ! id vyajena >/dev/null 2>&1; then
    useradd --system --user-group --home-dir /var/lib/vyajena --no-create-home \
      --shell /usr/sbin/nologin vyajena
    log "created the vyajena system user"
  fi
  install -d -m 0755 -o root -g root /etc/vyajena

  # Nothing but SSH and the server listens (pi-deployment: Host hardening).
  local unit
  for unit in avahi-daemon.service avahi-daemon.socket; do
    if [ "$(systemctl is-enabled "$unit" 2>/dev/null || true)" != "masked" ]; then
      systemctl disable --now "$unit" >/dev/null 2>&1 || true
      systemctl mask "$unit" >/dev/null
      log "masked $unit (mDNS listened on UDP 5353)"
    fi
  done

  # Security updates install unattended; never an automatic reboot (owner,
  # 2026-11-13): check.sh reports a pending reboot instead.
  if ! dpkg-query -W -f '${Status}' unattended-upgrades 2>/dev/null | grep -q "ok installed"; then
    apt-get update -qq
    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq unattended-upgrades >/dev/null
  fi
  cat >/etc/apt/apt.conf.d/20auto-upgrades <<'CONF'
APT::Periodic::Update-Package-Lists "1";
APT::Periodic::Unattended-Upgrade "1";
CONF
  cat >/etc/apt/apt.conf.d/51vyajena-unattended-upgrades <<'CONF'
// vyajena2 (deploy/pi/harden.sh): security updates without automatic reboots.
Unattended-Upgrade::Automatic-Reboot "false";
CONF
  systemctl enable --now apt-daily.timer apt-daily-upgrade.timer >/dev/null
}

# --- staged steps ------------------------------------------------------------

eth_connection() {
  nmcli -g GENERAL.CONNECTION device show "$IFACE"
}

needs() {
  case "$1" in
    sshd)
      local t
      t="$(sshd -T)"
      ! { grep -qx "permitrootlogin no" <<<"$t" &&
        grep -qx "passwordauthentication no" <<<"$t" &&
        grep -qx "kbdinteractiveauthentication no" <<<"$t" &&
        grep -qx "pubkeyauthentication yes" <<<"$t"; }
      ;;
    firewall)
      ! { cmp -s "$HERE/nftables.conf" /etc/nftables.conf &&
        systemctl is-enabled --quiet nftables.service &&
        nft list table inet vyajena >/dev/null 2>&1; }
      ;;
    address)
      local conn
      conn="$(eth_connection)"
      ! { [ "$(nmcli -g ipv4.method connection show "$conn")" = "manual" ] &&
        nmcli -g ipv4.addresses connection show "$conn" | grep -q "^$ADDRESS/"; }
      ;;
    *) die "unknown step $1" ;;
  esac
}

arm_undo() {
  local step="$1" command="$2"
  systemctl stop "vyajena-undo-$step.timer" >/dev/null 2>&1 || true
  systemctl reset-failed "vyajena-undo-$step.service" >/dev/null 2>&1 || true
  systemd-run --quiet --collect --unit="vyajena-undo-$step" \
    --on-active="$UNDO_AFTER" /bin/sh -c "$command"
  log "armed: $step is undone in $UNDO_AFTER unless committed"
}

stage() {
  case "$1" in
    sshd)
      cat >"$SSHD_DROPIN.new" <<'CONF'
# vyajena2 (deploy/pi/harden.sh): keys only, no root (pi-deployment: Host
# hardening). Read before the other drop-ins, so these values win.
PermitRootLogin no
PasswordAuthentication no
KbdInteractiveAuthentication no
PubkeyAuthentication yes
CONF
      chmod 0644 "$SSHD_DROPIN.new"
      mv -f "$SSHD_DROPIN.new" "$SSHD_DROPIN"
      if ! sshd -t; then
        rm -f "$SSHD_DROPIN"
        die "sshd -t refused the new settings; removed them"
      fi
      arm_undo sshd "rm -f $SSHD_DROPIN; systemctl reload ssh"
      systemctl reload ssh
      ;;
    firewall)
      nft -c -f "$HERE/nftables.conf" || die "nft refused nftables.conf"
      # The undo leaves the Pi as it was: no rules (nftables.service is only
      # enabled, and the file only installed, at commit).
      arm_undo firewall "nft flush ruleset"
      nft -f "$HERE/nftables.conf"
      ;;
    address)
      local conn current prefix gateway dns
      conn="$(eth_connection)"
      [ -n "$conn" ] || die "no NetworkManager connection on $IFACE"
      current="$(ip -4 -o addr show dev "$IFACE" | awk '{print $4; exit}')"
      [ "${current%/*}" = "$ADDRESS" ] ||
        die "$IFACE has ${current:-no IPv4 address}, not $ADDRESS; refusing to move the Pi"
      prefix="${current#*/}"
      gateway="$(ip -4 route show default dev "$IFACE" | awk '{print $3; exit}')"
      dns="$(nmcli -g IP4.DNS device show "$IFACE" | tr -d ' ' | tr '|' ',')"
      [ -n "$gateway" ] || die "no default gateway on $IFACE"
      [ -n "$dns" ] || dns="$gateway"
      arm_undo address "nmcli connection modify '$conn' ipv4.method auto ipv4.addresses '' ipv4.gateway '' ipv4.dns '' && nmcli device reapply $IFACE"
      # Runs as its own unit, so it finishes even if this SSH session drops.
      systemd-run --quiet --collect --unit=vyajena-apply-address /bin/sh -c \
        "nmcli connection modify '$conn' ipv4.method manual ipv4.addresses $ADDRESS/$prefix ipv4.gateway $gateway ipv4.dns $dns && nmcli device reapply $IFACE"
      log "$IFACE ($conn): static $ADDRESS/$prefix, gateway $gateway, DNS $dns"
      ;;
    *) die "unknown step $1" ;;
  esac
}

commit() {
  systemctl stop "vyajena-undo-$1.timer" >/dev/null 2>&1 || true
  case "$1" in
    sshd | address) ;;
    firewall)
      install -m 0644 -o root -g root "$HERE/nftables.conf" /etc/nftables.conf
      systemctl enable nftables.service >/dev/null 2>&1
      ;;
    *) die "unknown step $1" ;;
  esac
  log "committed $1"
}

[ "$(id -u)" -eq 0 ] || die "run as root"
case "${1:-}" in
  base) base ;;
  needs) needs "${2:?step}" ;;
  stage) stage "${2:?step}" ;;
  commit) commit "${2:?step}" ;;
  *) die "usage: harden.sh base | needs|stage|commit <sshd|firewall|address>" ;;
esac
