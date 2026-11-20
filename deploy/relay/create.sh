#!/usr/bin/env bash
# Creates the relay's GCP resources (relay: Provision and deploy the relay;
# design D2, D7). Idempotent: what exists is kept. Run on the Mac after
# deploy/relay/secrets.sh, with a valid `gcloud auth login`:
#
#   deploy/relay/create.sh --dry-run    print every gcloud command, change nothing
#   deploy/relay/create.sh              create what is missing
#   deploy/relay/create.sh --update-ports   also bring the tunnel and review firewall
#                                       rules to the ports of network.conf (the
#                                       cut-over; they are VPC-wide)
#
# Writes ~/.vyajena-relay/relay.env (RELAY_IP=) and known_hosts (the VM's host
# keys, read from guest attributes through the API). Names and sizes:
# deploy/relay/relay.conf. Cost in asia-south1 (Mumbai, not in the free tier),
# on-demand list prices: e2-micro about $7.3/month, static IPv4 about
# $3.65/month, 10 GB standard disk about $0.4/month; egress is small (chat
# ciphertext). About $11/month, roughly Rs 1,120 with 18% GST.
#
# It overwrites relay.env and known_hosts with the new VM's. Moving regions:
# copy both to relay.env.old / known_hosts.old first (design D3, change
# relay-mumbai), so the old relay can be put back.
set -euo pipefail

# shellcheck source=lib.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

DRY=no
UPDATE_PORTS=no

die() {
  echo "create.sh: $*" >&2
  exit 1
}

say() {
  echo "==> $*" >&2
}

# run <command...>: prints it, then runs it unless --dry-run.
run() {
  printf '%q ' "$@" | sed 's/ $//'
  echo
  [ "$DRY" = yes ] || "$@"
}

# exists <gcloud describe args...>: in a dry run, nothing is assumed to exist.
exists() {
  [ "$DRY" = no ] && gcloud "$@" --project="$PROJECT" --format='value(name)' >/dev/null 2>&1
}

g() {
  gcloud "$@" --project="$PROJECT"
}

# port_rule <tunnel|review> <port>: the rule for a port from network.conf. A
# rule that exists keeps its port, because rules are VPC-wide and the old
# relay uses them too; --update-ports brings it to the new port, at the
# cut-over (design D3 of change relay-mumbai). A missing rule is created.
port_rule() {
  local rule="$NAME-$1" port="$2"
  if [ "$UPDATE_PORTS" = yes ] && { [ "$DRY" = yes ] || exists compute firewall-rules describe "$rule"; }; then
    run gcloud compute firewall-rules update "$rule" --project="$PROJECT" --rules="tcp:$port"
  elif [ "$DRY" = no ] && exists compute firewall-rules describe "$rule"; then
    say "$rule exists; its port changes only with --update-ports"
  else
    run gcloud compute firewall-rules create "$rule" --project="$PROJECT" \
      --network="$NAME" --direction=INGRESS --action=ALLOW --rules="tcp:$port" \
      --source-ranges=0.0.0.0/0 --target-tags="$NAME" --no-enable-logging
  fi
}

preflight() {
  [ -s "$SECRETS/ssh_ed25519.pub" ] || die "no $SECRETS/ssh_ed25519.pub; run deploy/relay/secrets.sh"
  [ "$DRY" = yes ] && return 0
  g projects describe "$PROJECT" --format='value(projectId)' >/dev/null 2>&1 ||
    die "gcloud cannot read $PROJECT: run 'gcloud auth login' yourself, then retry"
  local billing
  billing="$(gcloud billing projects describe "$PROJECT" --format='value(billingEnabled)' 2>/dev/null || true)"
  case "$billing" in
    True) ;;
    False) die "billing is off on $PROJECT: link an open billing account first" ;;
    *) say "could not read billing for $PROJECT; continuing (Compute refuses without it)" ;;
  esac
}

main() {
  local arg
  for arg in "$@"; do
    case "$arg" in
      --dry-run) DRY=yes ;;
      --update-ports) UPDATE_PORTS=yes ;;
      -h | --help) sed -n '2,16p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
      *) die "unknown argument $arg" ;;
    esac
  done
  preflight

  local pubkey
  pubkey="$(cat "$SECRETS/ssh_ed25519.pub")"

  say "APIs (Compute, IAP for SSH)"
  run gcloud services enable compute.googleapis.com iap.googleapis.com --project="$PROJECT"

  say "VPC $NAME (own network, flow logs off)"
  exists compute networks describe "$NAME" ||
    run gcloud compute networks create "$NAME" --project="$PROJECT" --subnet-mode=custom
  exists compute networks subnets describe "$NAME" --region="$REGION" ||
    run gcloud compute networks subnets create "$NAME" --project="$PROJECT" \
      --network="$NAME" --region="$REGION" --range="$SUBNET_RANGE" --no-enable-flow-logs

  say "firewall: the public, review and tunnel ports from anywhere, 22 from IAP only, logging off"
  exists compute firewall-rules describe "$NAME-public" ||
    run gcloud compute firewall-rules create "$NAME-public" --project="$PROJECT" \
      --network="$NAME" --direction=INGRESS --action=ALLOW --rules="tcp:$PUBLIC_PORT" \
      --source-ranges=0.0.0.0/0 --target-tags="$NAME" --no-enable-logging
  port_rule tunnel "$TUNNEL_PORT"
  exists compute firewall-rules describe "$NAME-iap-ssh" ||
    run gcloud compute firewall-rules create "$NAME-iap-ssh" --project="$PROJECT" \
      --network="$NAME" --direction=INGRESS --action=ALLOW --rules=tcp:22 \
      --source-ranges="$IAP_RANGE" --target-tags="$NAME" --no-enable-logging

  port_rule review "$REVIEW_PORT"

  say "static external IPv4 $NAME in $REGION (about \$3.65/month)"
  exists compute addresses describe "$NAME" --region="$REGION" ||
    run gcloud compute addresses create "$NAME" --project="$PROJECT" \
      --region="$REGION" --network-tier=PREMIUM
  local ip=RELAY_IP
  if [ "$DRY" = no ]; then
    ip="$(g compute addresses describe "$NAME" --region="$REGION" --format='value(address)')"
    [[ "$ip" =~ ^[0-9]{1,3}(\.[0-9]{1,3}){3}$ ]] || die "no address for $NAME"
    printf 'RELAY_IP=%s\n' "$ip" >"$SECRETS/relay.env"
    chmod 0600 "$SECRETS/relay.env"
    say "relay address $ip (in $SECRETS/relay.env)"
  fi

  say "VM $NAME ($MACHINE_TYPE, $ZONE, no service account, SSH key for $SSH_USER only)"
  exists compute instances describe "$NAME" --zone="$ZONE" ||
    run gcloud compute instances create "$NAME" --project="$PROJECT" --zone="$ZONE" \
      --machine-type="$MACHINE_TYPE" \
      --network-interface="subnet=$NAME,address=$ip,network-tier=PREMIUM" \
      --image-family="$IMAGE_FAMILY" --image-project="$IMAGE_PROJECT" \
      --boot-disk-size="${DISK_GB}GB" --boot-disk-type="$DISK_TYPE" \
      --tags="$NAME" --no-service-account --no-scopes \
      --shielded-secure-boot --shielded-vtpm --shielded-integrity-monitoring \
      --metadata="enable-oslogin=FALSE,block-project-ssh-keys=TRUE,enable-guest-attributes=TRUE,serial-port-enable=FALSE,ssh-keys=$SSH_USER:$pubkey"

  say "host key from guest attributes -> $SECRETS/known_hosts"
  if [ "$DRY" = yes ]; then
    run gcloud compute instances get-guest-attributes "$NAME" --project="$PROJECT" \
      --zone="$ZONE" --query-path=hostkeys/ --format='value(key,value)'
    return 0
  fi
  local keys="" _
  for _ in $(seq 1 36); do
    keys="$(g compute instances get-guest-attributes "$NAME" --zone="$ZONE" \
      --query-path=hostkeys/ --format='value(key,value)' 2>/dev/null || true)"
    [ -n "$keys" ] && break
    sleep 5
  done
  [ -n "$keys" ] || die "the VM published no host keys in 3 minutes; rerun create.sh"
  awk -v host="$NAME" 'NF == 2 {print host, $1, $2}' <<<"$keys" >"$SECRETS/known_hosts"
  chmod 0600 "$SECRETS/known_hosts"
  say "done; next: deploy/relay/deploy.sh"
}

main "$@"
