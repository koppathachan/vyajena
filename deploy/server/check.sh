#!/usr/bin/env bash
# shellcheck disable=SC2153 # the ports are set by network.sh
# Checks the server's hardening over SSH (server-deployment: Hardening check). Prints
# one line per check, "FAIL <name>: <why>" for each failure, and exits 1 if any
# failed. WARN lines (a pending reboot, an address clash) do not fail it.
#
#   deploy/server/check.sh          SERVER=user@host overrides the SERVER of network.conf
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../network.sh
. "$HERE/../network.sh"
readonly SERVER
readonly HOST="$SERVER_ADDRESS"
# shellcheck source=../relay/relay.conf
. "$HERE/../relay/relay.conf"
# The highest `systemd-analyze security` exposure accepted for vyajena.service,
# vyajena-tunnel.service and vyajena-egress.service (server-deployment design D3:
# the first real run on systemd 257 scored 1.0; what is left is the network
# access they need).
readonly MAX_EXPOSURE="1.5"

# --- run on the server as root (sent with declare -f) ----------------------------

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
  # server_checks <unit> <user> <state dir> <other server's dir> <name prefix>:
  # the real server and the review server get the same checks
  # (review-server design D3); neither may write the other's directory.
  server_checks() {
    local unit="$1" user="$2" dir="$3" other="$4" pfx="$5"
    # Service user: a system account with no login shell and no password.
    local entry uid shell
    entry="$(getent passwd "$user" || true)"
    uid="$(cut -d: -f3 <<<"$entry")"
    shell="$(cut -d: -f7 <<<"$entry")"
    if [ -z "$entry" ]; then
      fail "${pfx}service-user" "no $user user"
    elif [ "$uid" -ge 1000 ] || { [ "$shell" != /usr/sbin/nologin ] && [ "$shell" != /bin/false ]; }; then
      fail "${pfx}service-user" "uid $uid, shell $shell (want a system uid and nologin)"
    elif ! getent shadow "$user" | cut -d: -f2 | grep -qE '^[!*]'; then
      fail "${pfx}service-user" "$user has a usable password"
    else
      ok "${pfx}service-user"
    fi

    check "${pfx}service-running" "$unit is not active and enabled" \
      sh -c "systemctl is-active --quiet $unit && systemctl is-enabled --quiet $unit"

    # Sandbox settings, as systemd applies them.
    local want prop got
    for want in "User=$user" NoNewPrivileges=yes ProtectSystem=strict ProtectHome=yes \
      PrivateTmp=yes PrivateDevices=yes CapabilityBoundingSet= AmbientCapabilities= \
      "RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX" Restart=on-failure \
      "StateDirectory=${dir#/var/lib/}" UMask=0077 KillSignal=15 \
      MemoryHigh=402653184 MemoryMax=536870912 TasksMax=256; do
      prop="${want%%=*}"
      got="$(systemctl show -p "$prop" --value "$unit")"
      if [ "$prop=$got" = "$want" ]; then ok "${pfx}sandbox $prop"; else fail "${pfx}sandbox $prop" "is '$got', want '${want#*=}'"; fi
    done
    exposure_of "$unit" "${pfx}sandbox"
    # Loopback only: devices come through the tunnel (relay design D5).
    got="$(systemctl show -p IPAddressAllow --value "$unit")"
    if [ "$(words "$got")" = "$(words "127.0.0.0/8 ::1/128")" ]; then ok "${pfx}sandbox IPAddressAllow loopback"; else fail "${pfx}sandbox IPAddressAllow" "is '$got', want loopback only"; fi
    # No writes outside /var/lib/vyajena, tried inside the running service's
    # namespaces as its user. Its private /tmp and /var/tmp (PrivateTmp=yes)
    # are throwaway and seen by nothing else, so they are not probed.
    local pid vuid vgid
    pid="$(systemctl show -p MainPID --value "$unit")"
    vuid="$(id -u "$user" 2>/dev/null || echo 65534)"
    vgid="$(id -g "$user" 2>/dev/null || echo 65534)"
    if [ -n "$pid" ] && [ "$pid" != 0 ]; then
      local probe=".vyajena-check-$$"
      if nsenter -t "$pid" -m -U -S "$vuid" -G "$vgid" \
        sh -c "touch $dir/$probe && rm $dir/$probe" 2>/dev/null; then
        ok "${pfx}sandbox writes to $dir"
      else
        fail "${pfx}sandbox writes" "the service cannot write $dir"
      fi
      local path bad=""
      for path in / /etc /usr/local/bin /var/lib /var/log /home /root $other; do
        if nsenter -t "$pid" -m -U -S "$vuid" -G "$vgid" \
          sh -c "touch $path/$probe" 2>/dev/null; then
          bad="$bad $path"
          rm -f "$path/$probe"
        fi
      done
      if [ -z "$bad" ]; then ok "${pfx}sandbox no writes elsewhere"; else fail "${pfx}sandbox writes" "the service can write:$bad"; fi
    else
      fail "${pfx}sandbox writes" "$unit has no main process"
    fi

  }
  server_checks vyajena.service vyajena /var/lib/vyajena /var/lib/vyajena-review ""
  server_checks vyajena-review.service vyajena-review /var/lib/vyajena-review /var/lib/vyajena "review "

  # Push (push design D5-D7): only the real server runs with --push; the
  # review server only logs pings.
  local exec_main exec_review
  exec_main="$(systemctl show -p ExecStart --value vyajena.service)"
  exec_review="$(systemctl show -p ExecStart --value vyajena-review.service)"
  if grep -q -- "--push /etc/vyajena/push.toml" <<<"$exec_main"; then ok "push vyajena.service --push"; else fail push "vyajena.service runs without --push /etc/vyajena/push.toml"; fi
  if grep -q -- "--push" <<<"$exec_review"; then fail "review push" "vyajena-review.service runs with --push"; else ok "review no push"; fi

  # The reusable admin invite (change review-admin-invite) is for the store
  # reviewers' server only: on the real server it would let anyone holding it
  # take over the admin seat.
  if grep -q '^\[review\]' /etc/vyajena/config.toml; then fail "real review flag" "/etc/vyajena/config.toml has a [review] table"; else ok "real server has no review flag"; fi

  # The egress proxy (server-deployment: Sandboxed service): dynamic user, no
  # writable path, loopback + the router's DNS + public addresses only.
  local dns
  dns="$(sed -n 's/^nameserver \([0-9.]*\)$/\1/p' /etc/resolv.conf | head -n 1)"
  check egress-running "vyajena-egress.service is not active and enabled" \
    sh -c 'systemctl is-active --quiet vyajena-egress.service && systemctl is-enabled --quiet vyajena-egress.service'
  for want in DynamicUser=yes NoNewPrivileges=yes ProtectSystem=strict ProtectHome=yes \
    PrivateTmp=yes PrivateDevices=yes PrivateUsers=yes CapabilityBoundingSet= AmbientCapabilities= \
    "RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX" Restart=always StateDirectory= \
    "IPAddressAllow=127.0.0.0/8 ::1/128 ${dns:-$ROUTER}/32" \
    "IPAddressDeny=0.0.0.0/8 10.0.0.0/8 100.64.0.0/10 169.254.0.0/16 172.16.0.0/12 192.168.0.0/16 fc00::/7 fe80::/10"; do
    prop="${want%%=*}"
    got="$(systemctl show -p "$prop" --value vyajena-egress.service)"
    case "$prop" in IPAddress*) got="$(words "$got")"; want="$prop=$(words "${want#*=}")" ;; esac
    if [ "$prop=$got" = "$want" ]; then ok "egress sandbox $prop"; else fail "egress sandbox $prop" "is '$got', want '${want#*=}'"; fi
  done
  exposure_of vyajena-egress.service "egress sandbox"
  pid="$(systemctl show -p MainPID --value vyajena-egress.service)"
  if [ -n "$pid" ] && [ "$pid" != 0 ]; then
    local euid egid ebad="" eprobe=".vyajena-check-$$"
    euid="$(stat -c %u "/proc/$pid")"
    egid="$(stat -c %g "/proc/$pid")"
    for path in / /etc /usr/local/bin /var/lib /var/lib/vyajena /var/log /home /root; do
      if nsenter -t "$pid" -m -U -S "$euid" -G "$egid" sh -c "touch $path/$eprobe" 2>/dev/null; then
        ebad="$ebad $path"
        rm -f "$path/$eprobe"
      fi
    done
    if [ -z "$ebad" ]; then ok "egress sandbox no writes"; else fail "egress sandbox writes" "the proxy can write:$ebad"; fi
  else
    fail "egress sandbox writes" "vyajena-egress.service has no main process"
  fi
  # The allowlist, asked on loopback: a host outside it gets 403; the push
  # hosts get a tunnel (the server reaches Google and Apple).
  connect_status() { # connect_status <host:port>: the proxy's status code
    # shellcheck disable=SC2016 # $1 expands in the inner bash, on purpose
    timeout 10 bash -c 'exec 3<>/dev/tcp/127.0.0.1/$2 &&
      printf "CONNECT %s HTTP/1.1\r\nHost: %s\r\n\r\n" "$1" "$1" >&3 &&
      head -n 1 <&3' _ "$1" "$EGRESS_PORT" 2>/dev/null | cut -d" " -f2
  }
  local code target
  code="$(connect_status example.com:443)"
  if [ "$code" = 403 ]; then ok "egress refuses example.com:443"; else fail "egress allowlist" "CONNECT example.com:443 got '${code:-nothing}', want 403"; fi
  code="$(connect_status fcm.googleapis.com:22)"
  if [ "$code" = 403 ]; then ok "egress refuses fcm.googleapis.com:22"; else fail "egress allowlist" "CONNECT fcm.googleapis.com:22 got '${code:-nothing}', want 403"; fi
  for target in fcm.googleapis.com:443 oauth2.googleapis.com:443 api.push.apple.com:443; do
    code="$(connect_status "$target")"
    if [ "$code" = 200 ]; then ok "egress reaches $target"; else fail "egress reach" "CONNECT $target got '${code:-nothing}', want 200"; fi
  done

  # Push keys (server-deployment: Push credentials): owner-only, the real server's
  # folder only; a platform without a key is a warning (it only logs pings).
  local pdir=/var/lib/vyajena/push
  m="$(stat -c '%a %U' "$pdir" 2>/dev/null)"
  if [ "$m" = "700 vyajena" ]; then ok "file-mode $pdir"; else fail "file-mode $pdir" "is '${m:-missing}', want '700 vyajena'"; fi
  for f in "$pdir"/*; do
    [ -e "$f" ] || continue
    m="$(stat -c '%a %U' "$f")"
    if [ "$m" = "600 vyajena" ]; then ok "file-mode $f"; else fail "file-mode $f" "is '$m', want '600 vyajena'"; fi
  done
  [ -e "$pdir/fcm-service-account.json" ] || warn push-fcm "no FCM service account key on the server: Android pings are only logged"
  compgen -G "$pdir/AuthKey_*.p8" >/dev/null || warn push-apns "no APNs key on the server: iPhone pings are only logged"
  if [ -e /var/lib/vyajena-review/push ]; then fail "review push keys" "/var/lib/vyajena-review/push exists"; else ok "review has no push keys"; fi

  # The blob store (server-deployment: Blob store deployment): off unless the
  # server's config names a path. Then the path must be on a mount of its own
  # (never the SD card), the service user's with mode 700, and writable by the
  # running service. A drive that is absent is a warning: the server boots and
  # serves text without it.
  local blob_dir blob_mount bpid buid bgid
  blob_dir="$(sed -n '/^\[blob_store\]/,/^\[/s/^path = "\(.*\)"$/\1/p' /etc/vyajena/config.toml)"
  if [ -z "$blob_dir" ]; then
    ok "blob-store off (no [blob_store] in the config)"
  elif [ ! -d "$blob_dir" ]; then
    warn blob-store "$blob_dir is not there (drive not mounted?): images are unavailable, text works"
  else
    blob_mount="$(findmnt -n -o TARGET -T "$blob_dir" 2>/dev/null || true)"
    if [ -z "$blob_mount" ] || [ "$blob_mount" = / ]; then
      fail "blob-store mount" "$blob_dir is on the main storage; blobs must be on a mount of their own"
    else
      ok "blob-store mount $blob_mount"
    fi
    m="$(mode_of "$blob_dir")"
    if [ "$m" = "700 vyajena" ]; then ok "file-mode $blob_dir"; else fail "file-mode $blob_dir" "is '$m', want '700 vyajena'"; fi
    if systemctl show -p ReadWritePaths --value vyajena.service | tr ' ' '\n' | grep -qx -- "-$blob_dir\|$blob_dir"; then
      ok "blob-store sandbox ReadWritePaths"
    else
      fail "blob-store sandbox ReadWritePaths" "vyajena.service does not list $blob_dir (run deploy.sh)"
    fi
    bpid="$(systemctl show -p MainPID --value vyajena.service)"
    buid="$(id -u vyajena 2>/dev/null || echo 65534)"
    bgid="$(id -g vyajena 2>/dev/null || echo 65534)"
    if [ -n "$bpid" ] && [ "$bpid" != 0 ] &&
      nsenter -t "$bpid" -m -U -S "$buid" -G "$bgid" \
        sh -c "touch $blob_dir/.vyajena-check-$$ && rm $blob_dir/.vyajena-check-$$" 2>/dev/null; then
      ok "blob-store service can write"
    else
      fail "blob-store service writes" "the running service cannot write $blob_dir (drive mounted after it started? restart vyajena.service)"
    fi
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
  local owner d
  for d in vyajena vyajena-review; do
    owner="$d"
    for f in "/var/lib/$d" "/var/lib/$d/keys"; do
      m="$(mode_of "$f")"
      if [ "$m" = "700 $owner" ]; then ok "file-mode $f"; else fail "file-mode $f" "is '${m:-missing}', want '700 $owner'"; fi
    done
    for f in "/var/lib/$d/server.sqlite" "/var/lib/$d/server.sqlite-wal" "/var/lib/$d/server.sqlite-shm" \
      "/var/lib/$d/keys/tls_key.pem" "/var/lib/$d/keys/mls_external_sender_key.pem"; do
      [ -e "$f" ] || { [[ "$f" == *-wal || "$f" == *-shm ]] && continue; }
      m="$(mode_of "$f")"
      if [ "$m" = "600 $owner" ]; then ok "file-mode $f"; else fail "file-mode $f" "is '${m:-missing}', want '600 $owner'"; fi
    done
  done
  for f in /usr/local/bin/vyajena-server:"755 root" /etc/vyajena/config.toml:"644 root" \
    /etc/systemd/system/vyajena.service:"644 root" /usr/local/bin/rathole:"755 root" \
    /etc/vyajena/tunnel.toml:"600 root" /etc/systemd/system/vyajena-tunnel.service:"644 root" \
    /etc/vyajena/review.toml:"644 root" /etc/systemd/system/vyajena-review.service:"644 root" \
    /etc/vyajena/push.toml:"644 root" /etc/systemd/system/vyajena-egress.service:"644 root"; do
    m="$(mode_of "${f%%:*}")"
    if [ "$m" = "${f#*:}" ]; then ok "file-mode ${f%%:*}"; else fail "file-mode ${f%%:*}" "is '${m:-missing}', want '${f#*:}'"; fi
  done

  # Listening: SSH, and the two servers and the egress proxy on loopback only. NetworkManager's DHCP
  # client sockets (UDP 68, 546) are clients, not services.
  local listeners extra
  listeners="$(ss -H -tulnp)"
  extra="$(awk -v a="127.0.0.1:$SERVER_PORT" -v b="127.0.0.1:$REVIEW_SERVER_PORT" -v c="127.0.0.1:$EGRESS_PORT" '
    $1 == "tcp" && $5 ~ /:22$/ && /"sshd"/ {next}
    $1 == "tcp" && ($5 == a || $5 == b || $5 == c) && /"vyajena-server"/ {next}
    $1 == "udp" && $5 ~ /:(68|546)$/ && /"NetworkManager"/ {next}
    {print $1, $5, $7}' <<<"$listeners")"
  if [ -z "$extra" ]; then ok listening; else fail listening "unexpected listeners: $(tr '\n' ';' <<<"$extra")"; fi
  check "review listening on loopback" "the review server is not listening on its loopback port" \
    grep -q "127.0.0.1:$REVIEW_SERVER_PORT.*\"vyajena-server\"" <<<"$listeners"
  check "listening on loopback" "vyajena-server is not listening on its loopback port" \
    grep -q "127.0.0.1:$SERVER_PORT.*\"vyajena-server\"" <<<"$listeners"
  check "egress listening on loopback" "the egress proxy is not listening on its loopback port" \
    grep -q "127.0.0.1:$EGRESS_PORT.*\"vyajena-server\"" <<<"$listeners"

  # nftables: loaded, default drop, SSH from the home subnet the only port.
  local rules
  rules="$(nft list chain inet vyajena input 2>/dev/null || true)"
  check "nftables loaded" "no inet vyajena input chain" test -n "$rules"
  check "nftables policy" "input policy is not drop" grep -q "policy drop;" <<<"$rules"
  check "nftables ssh" "SSH is not limited to the home network" \
    grep -q "ip saddr $HOME_LAN tcp dport 22 accept" <<<"$rules"
  if grep -q "dport $SERVER_PORT" <<<"$rules"; then
    fail "nftables server port" "a rule accepts the server's port; the server is loopback-only"
  else
    ok "nftables no server port"
  fi
  if grep -E "tcp dport" <<<"$rules" | grep -vq "ip saddr $HOME_LAN tcp dport 22 accept"; then
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

  # No toolchain on the server (server-deployment: Build for the server).
  if command -v cargo >/dev/null || command -v rustc >/dev/null; then
    fail no-toolchain "cargo or rustc is installed"
  else
    ok no-toolchain
  fi

  # The static address invites name (design D4).
  local conn
  conn="$(nmcli -g GENERAL.CONNECTION device show eth0 2>/dev/null)"
  if [ "$(nmcli -g ipv4.method connection show "$conn" 2>/dev/null)" = manual ] &&
    nmcli -g ipv4.addresses connection show "$conn" | grep -q "^$SERVER_ADDRESS/"; then
    ok "address static"
  else
    fail address "eth0 ($conn) is not the static address of network.conf"
  fi
  if arping -D -q -I eth0 -c 2 -w 3 "$SERVER_ADDRESS"; then
    ok "address no clash"
  else
    warn address-clash "another host answers ARP for the server's address; reserve it on the router"
  fi

  if [ -e /run/reboot-required ]; then
    warn reboot-required "updates are waiting for a reboot ($(tr '\n' ' ' </run/reboot-required.pkgs 2>/dev/null))"
  else
    ok "no reboot pending"
  fi

  # the host's kernel packages make no /run/reboot-required (qa-fixes D7):
  # a newer kernel of the running flavour in /lib/modules means one waits.
  local running newer
  running="$(uname -r)"
  newer="$(newer_kernel "$running" /lib/modules)"
  if [ -n "$newer" ]; then
    warn kernel-reboot "kernel $newer is installed, $running is running; reboot the server"
  else
    ok "kernel $running is the newest installed"
  fi
}

# newer_kernel <running> <modules dir>: prints the newest kernel in the
# folder with the running kernel's flavour (the part after the first "+",
# e.g. "+rpt-rpi-v8", or after the last "-" without one) if it sorts above
# the running one; else nothing. Other flavours (the server 5's "-2712") don't
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

# pin_check <name> <config on the server> <loopback port>: the public_address in
# that config presents the same key as the server's loopback port; sets PIN.
pin_check() {
  local name="$1" config="$2" port="$3" empty relay server_pin relay_pin
  PIN=""
  empty="$(printf '' | openssl dgst -sha256 -r | cut -d' ' -f1)"
  relay="$(ssh -o BatchMode=yes "$SERVER" "sed -n 's/^public_address = \"\(.*\)\"\$/\1/p' $config")"
  server_pin="$(ssh -o BatchMode=yes "$SERVER" "openssl s_client -connect 127.0.0.1:$port -alpn h2 </dev/null 2>/dev/null |
    openssl x509 -pubkey -noout | openssl pkey -pubin -outform der | openssl dgst -sha256 -r | cut -d' ' -f1")"
  relay_pin="$(spki "$relay")"
  if [ -z "$relay" ]; then
    echo "FAIL $name: no public_address in the server's $config"
  elif [ -z "$server_pin" ] || [ "$server_pin" = "$empty" ]; then
    echo "FAIL $name: no TLS key on the server's 127.0.0.1:$port"
  elif [ "$relay_pin" = "$server_pin" ]; then
    echo "ok   $name $relay presents the server's key ${server_pin:0:16}…"
    PIN="$server_pin"
  else
    echo "FAIL $name: $relay presents '${relay_pin:0:16}', the server '${server_pin:0:16}'"
  fi
}

mac_checks() {
  local port
  for port in "$SERVER_PORT" "$REVIEW_SERVER_PORT" "$EGRESS_PORT"; do
    if nc -z -G 3 "$HOST" "$port" 2>/dev/null; then
      echo "FAIL lan-closed: $HOST:$port answers on the LAN; the servers must be loopback-only"
    else
      echo "ok   lan-closed $HOST:$port"
    fi
  done
  # The relay reaches this server: each public port presents the key its loopback
  # port presents (both ends through openssl, relay design D10, review-server
  # D3), and the two servers never share a key.
  pin_check relay-pin /etc/vyajena/config.toml "$SERVER_PORT"
  local real_pin="$PIN"
  pin_check review-pin /etc/vyajena/review.toml "$REVIEW_SERVER_PORT"
  if [ -n "$real_pin" ] && [ -n "$PIN" ] && [ "$real_pin" != "$PIN" ]; then
    echo "ok   pins differ (real ${real_pin:0:16}…, review ${PIN:0:16}…)"
  else
    echo "FAIL pins differ: the real and the review server present the same or no key"
  fi
  # Root login with a valid key is refused.
  if ssh -o BatchMode=yes -o ConnectTimeout=5 -o ControlPath=none "root@$HOST" true 2>/dev/null; then
    echo "FAIL ssh root-login: root@$HOST logged in"
  else
    echo "ok   ssh root-login refused"
  fi
  # The ISP's global IPv6 addresses expose nothing.
  local v6 addr port open=""
  v6="$(ssh -o BatchMode=yes "$SERVER" "ip -6 -o addr show scope global | awk '{print \$4}' | cut -d/ -f1")"
  for addr in $v6; do
    for port in 22 "$SERVER_PORT" "$REVIEW_SERVER_PORT" "$EGRESS_PORT"; do
      if nc -z -G 3 "$addr" "$port" 2>/dev/null; then open="$open [$addr]:$port"; fi
    done
  done
  if [ -z "$v6" ]; then
    echo "ok   ipv6 (no global addresses)"
  elif [ -z "$open" ]; then
    echo "ok   ipv6 global addresses drop SSH and the servers' ports"
  else
    echo "FAIL ipv6: open over IPv6:$open"
  fi
}

report="$(
  { printf 'HOME_LAN=%q ROUTER=%q SERVER_ADDRESS=%q SERVER_PORT=%q REVIEW_SERVER_PORT=%q EGRESS_PORT=%q\n' \
      "$HOME_LAN" "$ROUTER" "$SERVER_ADDRESS" "$SERVER_PORT" "$REVIEW_SERVER_PORT" "$EGRESS_PORT"
    declare -f remote_checks newer_kernel; printf 'remote_checks %q %q\n' "$MAX_EXPOSURE" "$TUNNEL_PORT"; } |
    ssh -o BatchMode=yes -o ConnectTimeout=10 "$SERVER" sudo bash -s
  mac_checks
)"
echo "$report"
failures="$(grep -c '^FAIL' <<<"$report" || true)"
warnings="$(grep -c '^WARN' <<<"$report" || true)"
echo "check.sh: $failures failed, $warnings warnings"
[ "$failures" -eq 0 ]
