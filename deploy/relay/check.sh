#!/usr/bin/env bash
# Checks the relay (relay: Relay hardening check; design D10) through gcloud,
# over SSH via IAP, and from the Mac. Prints one line per check,
# "FAIL <name>: <why>" for each failure, and exits 1 if any failed. WARN lines
# (a pending reboot, the Pi's tunnel not attached) do not fail it.
#
#   deploy/relay/check.sh       needs a valid `gcloud auth login`
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly HERE
# shellcheck source=lib.sh
. "$HERE/lib.sh"
# The highest `systemd-analyze security` exposure accepted for
# vyajena-relay.service (design D8: the first run on Debian 13 scored 1.4;
# what is left is the network and the one capability it needs).
readonly MAX_EXPOSURE="2.0"
readonly FREE_REGIONS="us-central1 us-east1 us-west1"

ok() { echo "ok   $1"; }
fail() { echo "FAIL $1: $2"; }
warn() { echo "WARN $1: $2"; }
g() { gcloud "$@" --project="$PROJECT" --format=json 2>/dev/null; }

# --- through the GCP API -------------------------------------------------------

gcp_checks() {
  local ip="$1" inst disk sub rules
  inst="$(g compute instances describe "$NAME" --zone="$ZONE")" || {
    fail vm "no instance $NAME in $ZONE (or gcloud is not logged in)"
    return
  }
  local machine region status
  machine="$(jq -r '.machineType | split("/") | last' <<<"$inst")"
  status="$(jq -r '.status' <<<"$inst")"
  region="${ZONE%-*}"
  if [ "$machine" = e2-micro ]; then ok "vm type e2-micro"; else fail "vm type" "is $machine, want e2-micro (free tier)"; fi
  if [[ " $FREE_REGIONS " == *" $region "* ]]; then ok "vm region $region"; else fail "vm region" "$region is not a free-tier region"; fi
  if [ "$status" = RUNNING ]; then ok "vm running"; else fail "vm running" "status $status"; fi
  if [ "$(jq '.serviceAccounts // [] | length' <<<"$inst")" = 0 ]; then
    ok "vm no service account"
  else
    fail "vm service account" "the VM has a service account; it must call no Google API"
  fi
  if [ "$(jq -r '.shieldedInstanceConfig.enableSecureBoot' <<<"$inst")" = true ]; then
    ok "vm secure boot"
  else
    fail "vm secure boot" "Shielded VM secure boot is off"
  fi

  local meta
  meta="$(jq -r '.metadata.items // [] | .[] | "\(.key)=\(.value)"' <<<"$inst")"
  local want
  for want in enable-oslogin=FALSE block-project-ssh-keys=TRUE; do
    if grep -qx "$want" <<<"$meta"; then ok "vm metadata $want"; else fail "vm metadata" "want $want"; fi
  done
  if grep -qix "serial-port-enable=true" <<<"$meta"; then
    fail "vm metadata" "the serial console is enabled"
  else
    ok "vm serial console off"
  fi
  local keys pub
  keys="$(jq -r '.metadata.items // [] | .[] | select(.key == "ssh-keys") | .value' <<<"$inst" | sed '/^$/d')"
  pub="$(awk '{print $1, $2}' "$SECRETS/ssh_ed25519.pub" 2>/dev/null || true)"
  if [ "$(wc -l <<<"$keys" | tr -d ' ')" = 1 ] && [ -n "$pub" ] && [[ "$keys" == "$SSH_USER:$pub"* ]]; then
    ok "vm ssh-keys (only the relay key)"
  else
    fail "vm ssh-keys" "instance metadata holds other keys than $SSH_USER's relay key"
  fi

  local nics nat net
  nics="$(jq '.networkInterfaces | length' <<<"$inst")"
  nat="$(jq -r '.networkInterfaces[0].accessConfigs[0].natIP // ""' <<<"$inst")"
  net="$(jq -r '.networkInterfaces[0].network | split("/") | last' <<<"$inst")"
  if [ "$nics" = 1 ] && [ "$net" = "$NAME" ]; then ok "vm network $NAME"; else fail "vm network" "$nics interfaces, network $net"; fi
  if [ -n "$ip" ] && [ "$nat" = "$ip" ]; then
    ok "static address attached"
  else
    fail "static address" "VM has '$nat', relay.env says '$ip'"
  fi
  local addr
  addr="$(g compute addresses describe "$NAME" --region="$region" | jq -r '"\(.address) \(.status) \(.addressType)"')"
  if [ "$addr" = "$ip IN_USE EXTERNAL" ]; then ok "static address reserved"; else fail "static address" "reservation is '$addr'"; fi

  disk="$(g compute disks describe "$NAME" --zone="$ZONE")"
  local dtype dsize
  dtype="$(jq -r '.type | split("/") | last' <<<"$disk")"
  dsize="$(jq -r '.sizeGb' <<<"$disk")"
  if [ "$dtype" = pd-standard ] && [ "$dsize" -le 30 ]; then
    ok "disk $dtype ${dsize} GB"
  else
    fail disk "$dtype ${dsize} GB (free tier: pd-standard, at most 30 GB)"
  fi

  sub="$(g compute networks subnets describe "$NAME" --region="$region")"
  if [ "$(jq -r '(.enableFlowLogs // false) or (.logConfig.enable // false)' <<<"$sub")" = false ]; then
    ok "flow logs off"
  else
    fail "flow logs" "VPC flow logs are on for subnet $NAME"
  fi

  # Exactly the three rules, as created; nothing else in this VPC.
  rules="$(gcloud compute firewall-rules list --project="$PROJECT" --format=json \
    --filter="network~/$NAME\$" 2>/dev/null)"
  local got expect
  got="$(jq -r '.[] | [.name, .direction, (.disabled|tostring), ((.logConfig.enable // false)|tostring),
      ([.allowed[]? | "\(.IPProtocol):\(.ports // [] | join(","))"] | join(";")),
      ((.denied // []) | length | tostring),
      ((.sourceRanges // []) | join(",")), ((.targetTags // []) | join(","))] | join(" ")' <<<"$rules" | sort)"
  expect="$(printf '%s\n' \
    "$NAME-iap-ssh INGRESS false false tcp:22 0 $IAP_RANGE $NAME" \
    "$NAME-public INGRESS false false tcp:$PUBLIC_PORT 0 0.0.0.0/0 $NAME" \
    "$NAME-tunnel INGRESS false false tcp:$TUNNEL_PORT 0 0.0.0.0/0 $NAME" | sort)"
  if [ "$got" = "$expect" ]; then
    ok "firewall rules (443, $TUNNEL_PORT public; 22 from IAP; logging off)"
  else
    fail "firewall rule" "the VPC's rules differ from the three expected: $(diff <(echo "$expect") <(echo "$got") | grep '^[<>]' | tr '\n' ';')"
  fi

  local others
  others="$(gcloud compute instances list --project="$PROJECT" --format='value(name)' \
    --filter="networkInterfaces.network~/$NAME\$" 2>/dev/null | grep -vx "$NAME" || true)"
  if [ -z "$others" ]; then ok "vpc holds only $NAME"; else fail "vpc shared" "other VMs: $(tr '\n' ' ' <<<"$others")"; fi
}

# --- on the relay as root (sent with declare -f) -----------------------------

remote_checks() {
  local max_exposure="$1" tunnel_port="$2" want_sha="$3" want_version="$4"
  ok() { echo "ok   $1"; }
  fail() { echo "FAIL $1: $2"; }
  warn() { echo "WARN $1: $2"; }
  check() { # check <name> <why> <command...>
    local name="$1" why="$2"
    shift 2
    if "$@" >/dev/null 2>&1; then ok "$name"; else fail "$name" "$why"; fi
  }
  mode_of() { stat -c '%a %U' "$1" 2>/dev/null; }
  local unit=vyajena-relay.service

  check rathole-running "$unit is not active and enabled" \
    sh -c "systemctl is-active --quiet $unit && systemctl is-enabled --quiet $unit"

  local want prop got
  for want in DynamicUser=yes NoNewPrivileges=yes ProtectSystem=strict ProtectHome=yes \
    PrivateTmp=yes PrivateDevices=yes CapabilityBoundingSet=cap_net_bind_service \
    AmbientCapabilities=cap_net_bind_service "RestrictAddressFamilies=AF_INET AF_INET6" \
    Restart=always StateDirectory= LogsDirectory= CacheDirectory= Environment=RUST_LOG=warn; do
    prop="${want%%=*}"
    got="$(systemctl show -p "$prop" --value "$unit")"
    if [ "$prop=$got" = "$want" ]; then ok "sandbox $prop"; else fail "sandbox $prop" "is '$got', want '${want#*=}'"; fi
  done
  local exposure
  exposure="$(systemd-analyze security "$unit" --no-pager 2>/dev/null |
    sed -n 's/.*Overall exposure level for [^:]*: \([0-9.]*\).*/\1/p')"
  if [ -n "$exposure" ] && awk -v e="$exposure" -v m="$max_exposure" 'BEGIN {exit !(e <= m)}'; then
    ok "sandbox exposure $exposure (max $max_exposure)"
  else
    fail "sandbox exposure" "systemd-analyze security says ${exposure:-nothing}, max $max_exposure"
  fi

  # No writes anywhere, tried inside the running service's namespaces as its
  # dynamic user (its private /tmp is thrown away at stop).
  local pid uid gid
  pid="$(systemctl show -p MainPID --value "$unit")"
  if [ -n "$pid" ] && [ "$pid" != 0 ]; then
    uid="$(stat -c %u "/proc/$pid")"
    gid="$(stat -c %g "/proc/$pid")"
    local path bad="" probe=".vyajena-check-$$"
    for path in / /etc /usr/local/bin /var/lib /var/log /var/cache /home /root; do
      if nsenter -t "$pid" -m -S "$uid" -G "$gid" sh -c "touch $path/$probe" 2>/dev/null; then
        bad="$bad $path"
        rm -f "$path/$probe"
      fi
    done
    if [ -z "$bad" ]; then ok "sandbox no writes (no state)"; else fail "sandbox writes" "rathole can write:$bad"; fi
  else
    fail "sandbox writes" "$unit has no main process"
  fi

  local version sha
  version="$(/usr/local/bin/rathole --version 2>/dev/null | sed -n 's/^Build Version: *//p')"
  if [ "$version" = "$want_version" ]; then ok "rathole version $want_version"; else fail "rathole version" "is '$version'"; fi
  sha="$(sha256sum /usr/local/bin/rathole | cut -d' ' -f1)"
  if [ -z "$want_sha" ]; then
    warn rathole-build "no local target/relay/rathole to compare with"
  elif [ "$sha" = "$want_sha" ]; then
    ok "rathole binary is the local build"
  else
    fail "rathole binary" "sha256 $sha differs from the local build"
  fi

  local f m
  for f in /etc/vyajena-relay:"700 root" /etc/vyajena-relay/server.toml:"600 root" \
    /usr/local/bin/rathole:"755 root" /etc/systemd/system/$unit:"644 root"; do
    m="$(mode_of "${f%%:*}")"
    if [ "$m" = "${f#*:}" ]; then ok "file-mode ${f%%:*}"; else fail "file-mode ${f%%:*}" "is '${m:-missing}', want '${f#*:}'"; fi
  done

  # Listening: SSH and rathole on public interfaces; anything bound to
  # loopback only (the DNS stub, the local MTA, chrony) and DHCP clients are
  # not reachable from outside.
  local listeners extra
  listeners="$(ss -H -tulnp)"
  extra="$(awk -v t=":$tunnel_port\$" '
    $1 == "tcp" && $5 ~ /:22$/ && /"sshd"/ {next}
    $1 == "tcp" && ($5 ~ /:443$/ || $5 ~ t) && /"rathole"/ {next}
    $5 ~ /^(127\.[0-9.]+|\[::1\])(%lo)?:[0-9]+$/ {next}
    $1 == "udp" && $5 ~ /:(68|546)$/ {next}
    {print $1, $5, $7}' <<<"$listeners")"
  if [ -z "$extra" ]; then ok listening; else fail listening "unexpected listeners: $(tr '\n' ';' <<<"$extra")"; fi
  check "listening $tunnel_port" "rathole is not listening on the tunnel port" \
    grep -q ":$tunnel_port .*\"rathole\"" <<<"$listeners"

  # The Pi's control channel.
  if ss -Htn state established "( sport = :$tunnel_port )" | grep -q .; then
    ok "tunnel attached"
  else
    warn tunnel "no tunnel connection from the Pi on $tunnel_port"
  fi

  local sshd
  sshd="$(sshd -T 2>/dev/null)"
  check "ssh root-login" "PermitRootLogin is not 'no'" grep -qx "permitrootlogin no" <<<"$sshd"
  check "ssh passwords" "PasswordAuthentication is not 'no'" grep -qx "passwordauthentication no" <<<"$sshd"
  check "ssh keyboard-interactive" "KbdInteractiveAuthentication is not 'no'" \
    grep -qx "kbdinteractiveauthentication no" <<<"$sshd"

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

  check "journal volatile" "journald does not keep logs in RAM only" \
    sh -c 'systemd-analyze cat-config systemd/journald.conf | grep -qx "Storage=volatile" && [ ! -e /var/log/journal ]'
  check "no syslog" "a syslog daemon writes persistent logs" \
    sh -c '! systemctl is-active --quiet rsyslog.service && ! systemctl is-active --quiet syslog-ng.service'

  if [ -e /run/reboot-required ]; then
    warn reboot-required "updates are waiting for a reboot ($(tr '\n' ' ' </run/reboot-required.pkgs 2>/dev/null))"
  else
    ok "no reboot pending"
  fi
}

# --- from the Mac --------------------------------------------------------------

mac_checks() {
  local ip="$1" port
  [ -n "$ip" ] || { fail reachable "no RELAY_IP in $SECRETS/relay.env"; return; }
  if nc -z -G 3 "$ip" "$TUNNEL_PORT" 2>/dev/null; then ok "reachable $ip:$TUNNEL_PORT"; else fail reachable "$ip:$TUNNEL_PORT does not answer"; fi
  if nc -z -G 3 "$ip" "$PUBLIC_PORT" 2>/dev/null; then
    ok "reachable $ip:$PUBLIC_PORT"
  else
    warn reachable "$ip:$PUBLIC_PORT does not answer (it listens only while the Pi's tunnel is attached)"
  fi
  local open=""
  for port in 22 80 8443; do
    if nc -z -G 3 "$ip" "$port" 2>/dev/null; then open="$open $port"; fi
  done
  if [ -z "$open" ]; then ok "closed 22, 80, 8443 (public SSH too)"; else fail "firewall open" "$ip answers on:$open"; fi
}

ip="$(relay_ip)"
want_sha=""
[ -x "$HERE/../../target/relay/rathole" ] &&
  want_sha="$(shasum -a 256 "$HERE/../../target/relay/rathole" | cut -d' ' -f1)"
report="$(
  gcp_checks "$ip"
  if relay_open 2>/dev/null; then
    { declare -f remote_checks
      printf 'remote_checks %q %q %q %q\n' "$MAX_EXPOSURE" "$TUNNEL_PORT" "$want_sha" "$RATHOLE_VERSION"
    } | relay_ssh sudo bash -s
  else
    fail ssh "cannot reach $NAME over IAP with the relay key"
  fi
  mac_checks "$ip"
)"
echo "$report"
failures="$(grep -c '^FAIL' <<<"$report" || true)"
warnings="$(grep -c '^WARN' <<<"$report" || true)"
echo "check.sh: $failures failed, $warnings warnings"
[ "$failures" -eq 0 ]
