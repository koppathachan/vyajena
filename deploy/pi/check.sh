#!/usr/bin/env bash
# Checks the Pi's hardening over SSH (pi-deployment: Hardening check). Prints
# one line per check, "FAIL <name>: <why>" for each failure, and exits 1 if any
# failed. WARN lines (a pending reboot, an address clash) do not fail it.
#
#   deploy/pi/check.sh          PI=user@host overrides vyajan@192.168.1.8
set -euo pipefail

PI="${PI:-vyajan@192.168.1.8}"
readonly PI
readonly HOST="${PI#*@}"
# shellcheck source=../relay/relay.conf
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../relay/relay.conf"
# The highest `systemd-analyze security` exposure accepted for vyajena.service
# and vyajena-tunnel.service (pi-deployment design D3: the first real run on
# systemd 257 scored 1.0; what is left is the network access they need).
readonly MAX_EXPOSURE="1.5"

# --- run on the Pi as root (sent with declare -f) ----------------------------

remote_checks() {
  local max_exposure="$1" tunnel_port="$2"
  ok() { echo "ok   $1"; }
  fail() { echo "FAIL $1: $2"; }
  warn() { echo "WARN $1: $2"; }
  check() { # check <name> <why> <command...>
    local name="$1" why="$2"
    shift 2
    if "$@" >/dev/null 2>&1; then ok "$name"; else fail "$name" "$why"; fi
  }
  mode_of() { stat -c '%a %U' "$1" 2>/dev/null; }
  # Address lists compare as sets: systemd prints them in its own order.
  words() { tr ' ' '\n' <<<"$1" | sed '/^$/d' | sort | tr '\n' ' ' | sed 's/ $//'; }

  # Service user: a system account with no login shell and no password.
  local entry uid shell
  entry="$(getent passwd vyajena || true)"
  uid="$(cut -d: -f3 <<<"$entry")"
  shell="$(cut -d: -f7 <<<"$entry")"
  if [ -z "$entry" ]; then
    fail service-user "no vyajena user"
  elif [ "$uid" -ge 1000 ] || { [ "$shell" != /usr/sbin/nologin ] && [ "$shell" != /bin/false ]; }; then
    fail service-user "uid $uid, shell $shell (want a system uid and nologin)"
  elif ! getent shadow vyajena | cut -d: -f2 | grep -qE '^[!*]'; then
    fail service-user "vyajena has a usable password"
  else
    ok service-user
  fi

  check service-running "vyajena.service is not active and enabled" \
    sh -c 'systemctl is-active --quiet vyajena.service && systemctl is-enabled --quiet vyajena.service'

  # Sandbox settings, as systemd applies them.
  local want prop got
  for want in User=vyajena NoNewPrivileges=yes ProtectSystem=strict ProtectHome=yes \
    PrivateTmp=yes PrivateDevices=yes CapabilityBoundingSet= AmbientCapabilities= \
    "RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX" Restart=on-failure \
    StateDirectory=vyajena UMask=0077 KillSignal=15; do
    prop="${want%%=*}"
    got="$(systemctl show -p "$prop" --value vyajena.service)"
    if [ "$prop=$got" = "$want" ]; then ok "sandbox $prop"; else fail "sandbox $prop" "is '$got', want '${want#*=}'"; fi
  done
  exposure_of() { # exposure_of <unit> <label>
    local exposure
    exposure="$(systemd-analyze security "$1" --no-pager 2>/dev/null |
      sed -n 's/.*Overall exposure level for [^:]*: \([0-9.]*\).*/\1/p')"
    if [ -n "$exposure" ] && awk -v e="$exposure" -v m="$max_exposure" 'BEGIN {exit !(e <= m)}'; then
      ok "$2 exposure $exposure (max $max_exposure)"
    else
      fail "$2 exposure" "systemd-analyze security says ${exposure:-nothing}, max $max_exposure"
    fi
  }
  exposure_of vyajena.service sandbox
  # Loopback only: devices come through the tunnel (relay design D5).
  got="$(systemctl show -p IPAddressAllow --value vyajena.service)"
  if [ "$(words "$got")" = "$(words "127.0.0.0/8 ::1/128")" ]; then ok "sandbox IPAddressAllow loopback"; else fail "sandbox IPAddressAllow" "is '$got', want loopback only"; fi
  # No writes outside /var/lib/vyajena, tried inside the running service's
  # namespaces as its user. Its private /tmp and /var/tmp (PrivateTmp=yes)
  # are throwaway and seen by nothing else, so they are not probed.
  local pid vuid vgid
  pid="$(systemctl show -p MainPID --value vyajena.service)"
  vuid="$(id -u vyajena 2>/dev/null || echo 65534)"
  vgid="$(id -g vyajena 2>/dev/null || echo 65534)"
  if [ -n "$pid" ] && [ "$pid" != 0 ]; then
    local probe=".vyajena-check-$$"
    if nsenter -t "$pid" -m -U -S "$vuid" -G "$vgid" \
      sh -c "touch /var/lib/vyajena/$probe && rm /var/lib/vyajena/$probe" 2>/dev/null; then
      ok "sandbox writes to /var/lib/vyajena"
    else
      fail "sandbox writes" "the service cannot write /var/lib/vyajena"
    fi
    local path bad=""
    for path in / /etc /usr/local/bin /var/lib /var/log /home /root; do
      if nsenter -t "$pid" -m -U -S "$vuid" -G "$vgid" \
        sh -c "touch $path/$probe" 2>/dev/null; then
        bad="$bad $path"
        rm -f "$path/$probe"
      fi
    done
    if [ -z "$bad" ]; then ok "sandbox no writes elsewhere"; else fail "sandbox writes" "the service can write:$bad"; fi
  else
    fail "sandbox writes" "vyajena.service has no main process"
  fi

  # The tunnel (relay design D5): sandboxed, dynamic user, only loopback and
  # the relay, attached.
  local relay_ip
  relay_ip="$(sed -n 's/^remote_addr = "\([0-9.]*\):[0-9]*"$/\1/p' /etc/vyajena/tunnel.toml 2>/dev/null)"
  check tunnel-running "vyajena-tunnel.service is not active and enabled" \
    sh -c 'systemctl is-active --quiet vyajena-tunnel.service && systemctl is-enabled --quiet vyajena-tunnel.service'
  for want in DynamicUser=yes NoNewPrivileges=yes ProtectSystem=strict ProtectHome=yes \
    PrivateTmp=yes PrivateDevices=yes PrivateUsers=yes CapabilityBoundingSet= AmbientCapabilities= \
    "RestrictAddressFamilies=AF_INET AF_INET6" Restart=always StateDirectory= \
    "IPAddressAllow=127.0.0.0/8 ::1/128 $relay_ip/32" "IPAddressDeny=0.0.0.0/0 ::/0"; do
    prop="${want%%=*}"
    got="$(systemctl show -p "$prop" --value vyajena-tunnel.service)"
    case "$prop" in IPAddress*) got="$(words "$got")"; want="$prop=$(words "${want#*=}")" ;; esac
    if [ "$prop=$got" = "$want" ]; then ok "tunnel sandbox $prop"; else fail "tunnel sandbox $prop" "is '$got', want '${want#*=}'"; fi
  done
  exposure_of vyajena-tunnel.service "tunnel sandbox"
  pid="$(systemctl show -p MainPID --value vyajena-tunnel.service)"
  if [ -n "$pid" ] && [ "$pid" != 0 ]; then
    local tuid tgid tbad="" tprobe=".vyajena-check-$$"
    tuid="$(stat -c %u "/proc/$pid")"
    tgid="$(stat -c %g "/proc/$pid")"
    for path in / /etc /usr/local/bin /var/lib /var/lib/vyajena /var/log /home /root; do
      if nsenter -t "$pid" -m -U -S "$tuid" -G "$tgid" sh -c "touch $path/$tprobe" 2>/dev/null; then
        tbad="$tbad $path"
        rm -f "$path/$tprobe"
      fi
    done
    if [ -z "$tbad" ]; then ok "tunnel sandbox no writes"; else fail "tunnel sandbox writes" "rathole can write:$tbad"; fi
  else
    fail "tunnel sandbox writes" "vyajena-tunnel.service has no main process"
  fi
  if [ -n "$relay_ip" ] && ss -Htnp state established dst "$relay_ip:$tunnel_port" | grep -q '"rathole"'; then
    ok "tunnel attached to $relay_ip:$tunnel_port"
  else
    fail tunnel "no rathole connection to ${relay_ip:-the relay}:$tunnel_port"
  fi

  # File modes: database and keys owner-only.
  local f m
  for f in /var/lib/vyajena /var/lib/vyajena/keys; do
    m="$(mode_of "$f")"
    if [ "$m" = "700 vyajena" ]; then ok "file-mode $f"; else fail "file-mode $f" "is '${m:-missing}', want '700 vyajena'"; fi
  done
  for f in /var/lib/vyajena/pi.sqlite /var/lib/vyajena/pi.sqlite-wal /var/lib/vyajena/pi.sqlite-shm \
    /var/lib/vyajena/keys/tls_key.pem /var/lib/vyajena/keys/mls_external_sender_key.pem; do
    [ -e "$f" ] || { [[ "$f" == *-wal || "$f" == *-shm ]] && continue; }
    m="$(mode_of "$f")"
    if [ "$m" = "600 vyajena" ]; then ok "file-mode $f"; else fail "file-mode $f" "is '${m:-missing}', want '600 vyajena'"; fi
  done
  for f in /usr/local/bin/vyajena-server:"755 root" /etc/vyajena/config.toml:"644 root" \
    /etc/systemd/system/vyajena.service:"644 root" /usr/local/bin/rathole:"755 root" \
    /etc/vyajena/tunnel.toml:"600 root" /etc/systemd/system/vyajena-tunnel.service:"644 root"; do
    m="$(mode_of "${f%%:*}")"
    if [ "$m" = "${f#*:}" ]; then ok "file-mode ${f%%:*}"; else fail "file-mode ${f%%:*}" "is '${m:-missing}', want '${f#*:}'"; fi
  done

  # Listening: SSH, and the server on loopback only. NetworkManager's DHCP
  # client sockets (UDP 68, 546) are clients, not services.
  local listeners extra
  listeners="$(ss -H -tulnp)"
  extra="$(awk '
    $1 == "tcp" && $5 ~ /:22$/ && /"sshd"/ {next}
    $1 == "tcp" && $5 == "127.0.0.1:8443" && /"vyajena-server"/ {next}
    $1 == "udp" && $5 ~ /:(68|546)$/ && /"NetworkManager"/ {next}
    {print $1, $5, $7}' <<<"$listeners")"
  if [ -z "$extra" ]; then ok listening; else fail listening "unexpected listeners: $(tr '\n' ';' <<<"$extra")"; fi
  check "listening 127.0.0.1:8443" "vyajena-server is not listening on 127.0.0.1:8443" \
    grep -q '127.0.0.1:8443.*"vyajena-server"' <<<"$listeners"

  # nftables: loaded, default drop, SSH from the home subnet the only port.
  local rules
  rules="$(nft list chain inet vyajena input 2>/dev/null || true)"
  check "nftables loaded" "no inet vyajena input chain" test -n "$rules"
  check "nftables policy" "input policy is not drop" grep -q "policy drop;" <<<"$rules"
  check "nftables ssh" "SSH is not limited to 192.168.1.0/24" \
    grep -q "ip saddr 192.168.1.0/24 tcp dport 22 accept" <<<"$rules"
  if grep -q "dport 8443" <<<"$rules"; then
    fail "nftables 8443" "a rule still accepts 8443; the server is loopback-only"
  else
    ok "nftables no 8443"
  fi
  if grep -E "tcp dport" <<<"$rules" | grep -vq "ip saddr 192.168.1.0/24 tcp dport 22 accept"; then
    fail "nftables open port" "a port other than SSH from the home subnet is open"
  else
    ok "nftables open port"
  fi
  check "nftables boot" "nftables.service is not enabled, or /etc/nftables.conf is not vyajena's" \
    sh -c 'systemctl is-enabled --quiet nftables.service && grep -q "table inet vyajena" /etc/nftables.conf'

  # SSH: keys only, no root.
  local sshd
  sshd="$(sshd -T 2>/dev/null)"
  check "ssh root-login" "PermitRootLogin is not 'no'" grep -qx "permitrootlogin no" <<<"$sshd"
  check "ssh passwords" "PasswordAuthentication is not 'no'" grep -qx "passwordauthentication no" <<<"$sshd"
  check "ssh keyboard-interactive" "KbdInteractiveAuthentication is not 'no'" \
    grep -qx "kbdinteractiveauthentication no" <<<"$sshd"

  # Unattended security upgrades, no automatic reboot.
  local apt
  apt="$(apt-config dump 2>/dev/null)"
  check "unattended-upgrades installed" "package missing" \
    sh -c "dpkg-query -W -f '\${Status}' unattended-upgrades | grep -q 'ok installed'"
  check "unattended-upgrades enabled" "APT::Periodic::Unattended-Upgrade is not 1" \
    grep -q '^APT::Periodic::Unattended-Upgrade "1";' <<<"$apt"
  check "unattended-upgrades no-reboot" "Automatic-Reboot is not false" \
    grep -q '^Unattended-Upgrade::Automatic-Reboot "false";' <<<"$apt"
  check "unattended-upgrades timer" "apt-daily-upgrade.timer is not active" \
    systemctl is-active --quiet apt-daily-upgrade.timer

  # No toolchain on the Pi (pi-deployment: Build for the Pi).
  if command -v cargo >/dev/null || command -v rustc >/dev/null; then
    fail no-toolchain "cargo or rustc is installed"
  else
    ok no-toolchain
  fi

  # The static address invites name (design D4).
  local conn
  conn="$(nmcli -g GENERAL.CONNECTION device show eth0 2>/dev/null)"
  if [ "$(nmcli -g ipv4.method connection show "$conn" 2>/dev/null)" = manual ] &&
    nmcli -g ipv4.addresses connection show "$conn" | grep -q '^192.168.1.8/'; then
    ok "address static 192.168.1.8"
  else
    fail address "eth0 ($conn) is not a static 192.168.1.8"
  fi
  if arping -D -q -I eth0 -c 2 -w 3 192.168.1.8; then
    ok "address no clash"
  else
    warn address-clash "another host answers ARP for 192.168.1.8; reserve it on the router"
  fi

  if [ -e /run/reboot-required ]; then
    warn reboot-required "updates are waiting for a reboot ($(tr '\n' ' ' </run/reboot-required.pkgs 2>/dev/null))"
  else
    ok "no reboot pending"
  fi

  # Raspberry Pi kernel packages make no /run/reboot-required (qa-fixes D7):
  # a newer kernel of the running flavour in /lib/modules means one waits.
  local running newer
  running="$(uname -r)"
  newer="$(newer_kernel "$running" /lib/modules)"
  if [ -n "$newer" ]; then
    warn kernel-reboot "kernel $newer is installed, $running is running; reboot the Pi"
  else
    ok "kernel $running is the newest installed"
  fi
}

# newer_kernel <running> <modules dir>: prints the newest kernel in the
# folder with the running kernel's flavour (the part after the first "+",
# e.g. "+rpt-rpi-v8", or after the last "-" without one) if it sorts above
# the running one; else nothing. Other flavours (the Pi 5's "-2712") don't
# count.
newer_kernel() {
  local running="$1" dir="$2" flavour name newest=""
  if [[ "$running" == *+* ]]; then flavour="+${running#*+}"; else flavour="-${running##*-}"; fi
  for name in "$dir"/*; do
    name="${name##*/}"
    [[ "$name" == ?*"$flavour" ]] || continue
    [[ "${name%"$flavour"}" =~ ^[0-9][0-9.]*(-[0-9]+)?$ ]] || continue
    if [ -z "$newest" ] || [ "$(printf '%s\n' "$newest" "$name" | sort -V | tail -n 1)" = "$name" ]; then
      newest="$name"
    fi
  done
  if [ -n "$newest" ] && [ "$newest" != "$running" ] &&
    [ "$(printf '%s\n' "$running" "$newest" | sort -V | tail -n 1)" = "$newest" ]; then
    echo "$newest"
  fi
}

# --- from the Mac --------------------------------------------------------------

# spki <host:port>: SHA-256 of the TLS key the server presents (the pin).
spki() {
  openssl s_client -connect "$1" -alpn h2 </dev/null 2>/dev/null |
    openssl x509 -pubkey -noout 2>/dev/null |
    openssl pkey -pubin -outform der 2>/dev/null | openssl dgst -sha256 -r | cut -d' ' -f1
}

mac_checks() {
  local empty
  empty="$(printf '' | openssl dgst -sha256 -r | cut -d' ' -f1)"
  if nc -z -G 3 "$HOST" 8443 2>/dev/null; then
    echo "FAIL lan-closed: $HOST:8443 answers on the LAN; the server must be loopback-only"
  else
    echo "ok   lan-closed $HOST:8443"
  fi
  # The relay reaches this Pi: its 443 presents the key the Pi's loopback port
  # presents (both ends through openssl, relay design D10).
  local relay pi_pin relay_pin
  relay="$(ssh -o BatchMode=yes "$PI" "sed -n 's/^public_address = \"\(.*\)\"\$/\1/p' /etc/vyajena/config.toml")"
  pi_pin="$(ssh -o BatchMode=yes "$PI" "openssl s_client -connect 127.0.0.1:8443 -alpn h2 </dev/null 2>/dev/null |
    openssl x509 -pubkey -noout | openssl pkey -pubin -outform der | openssl dgst -sha256 -r | cut -d' ' -f1")"
  relay_pin="$(spki "$relay")"
  if [ -z "$relay" ]; then
    echo "FAIL relay-pin: no public_address in the Pi's config"
  elif [ -z "$pi_pin" ] || [ "$pi_pin" = "$empty" ]; then
    echo "FAIL relay-pin: no TLS key on the Pi's 127.0.0.1:8443"
  elif [ "$relay_pin" = "$pi_pin" ]; then
    echo "ok   relay-pin $relay presents the Pi's key ${pi_pin:0:16}…"
  else
    echo "FAIL relay-pin: $relay presents '${relay_pin:0:16}', the Pi '${pi_pin:0:16}'"
  fi
  # Root login with a valid key is refused.
  if ssh -o BatchMode=yes -o ConnectTimeout=5 -o ControlPath=none "root@$HOST" true 2>/dev/null; then
    echo "FAIL ssh root-login: root@$HOST logged in"
  else
    echo "ok   ssh root-login refused"
  fi
  # The ISP's global IPv6 addresses expose nothing.
  local v6 addr port open=""
  v6="$(ssh -o BatchMode=yes "$PI" "ip -6 -o addr show scope global | awk '{print \$4}' | cut -d/ -f1")"
  for addr in $v6; do
    for port in 22 8443; do
      if nc -z -G 3 "$addr" "$port" 2>/dev/null; then open="$open [$addr]:$port"; fi
    done
  done
  if [ -z "$v6" ]; then
    echo "ok   ipv6 (no global addresses)"
  elif [ -z "$open" ]; then
    echo "ok   ipv6 global addresses drop 22 and 8443"
  else
    echo "FAIL ipv6: open over IPv6:$open"
  fi
}

report="$(
  { declare -f remote_checks newer_kernel; printf 'remote_checks %q %q\n' "$MAX_EXPOSURE" "$TUNNEL_PORT"; } |
    ssh -o BatchMode=yes -o ConnectTimeout=10 "$PI" sudo bash -s
  mac_checks
)"
echo "$report"
failures="$(grep -c '^FAIL' <<<"$report" || true)"
warnings="$(grep -c '^WARN' <<<"$report" || true)"
echo "check.sh: $failures failed, $warnings warnings"
[ "$failures" -eq 0 ]
