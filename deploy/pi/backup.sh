#!/usr/bin/env bash
# Copies a consistent snapshot of the Pi's database and its keys to the Mac
# (pi-deployment: Backup to the Mac; design D6), while the service runs:
#
#   deploy/pi/backup.sh     -> ~/vyajena-pi-backups/<UTC time>/{pi.sqlite,keys/}
#
# BACKUP_ROOT overrides ~/vyajena-pi-backups; PI=user@host overrides the Pi.
# Everything is owner-only (0700 folders, 0600 files). Never into the repo:
# the keys are the server's private keys.
#
# Image blobs are not backed up (pi-deployment: Backup to the Mac): they are
# encrypted, live 7 days and can be sent again. They sit outside
# /var/lib/vyajena, the only folder this copies (deploy refuses a BLOB_DIR
# inside it).
set -euo pipefail

PI="${PI:-vyajan@192.168.1.8}"
readonly PI
readonly BACKUP_ROOT="${BACKUP_ROOT:-$HOME/vyajena-pi-backups}"

die() {
  echo "backup.sh: $*" >&2
  exit 1
}

# --- run on the Pi as root (sent with declare -f) ----------------------------

# Writes the snapshot with the server binary as the service user (SQLite
# online backup, no sqlite3 tool on the Pi), streams it and the keys as a tar
# to stdout, and removes the snapshot.
remote_backup() {
  set -euo pipefail
  local dir=/var/lib/vyajena/backup name
  name="pi-$(date -u +%Y%m%dT%H%M%SZ).sqlite"
  install -d -m 0700 -o vyajena -g vyajena "$dir"
  rm -f "${dir:?}"/pi-*.sqlite
  runuser -u vyajena -- /usr/local/bin/vyajena-server backup \
    --config /etc/vyajena/config.toml --to "$dir/$name" >&2
  tar -C /var/lib/vyajena -cf - "backup/$name" keys
  rm -f "${dir:?}/$name"
}

# -----------------------------------------------------------------------------

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
umask 077
mkdir -p "$BACKUP_ROOT"
chmod 0700 "$BACKUP_ROOT"
dest="$(cd "$BACKUP_ROOT" && pwd)/$(date -u +%Y%m%dT%H%M%SZ)"
case "$dest/" in
  "$repo"/*) die "$dest is inside the repository; set BACKUP_ROOT elsewhere" ;;
esac
mkdir "$dest"

{ declare -f remote_backup; echo remote_backup; } |
  ssh -o BatchMode=yes -o ConnectTimeout=10 "$PI" sudo bash -s |
  tar -C "$dest" -xf -

snapshot="$(find "$dest/backup" -name 'pi-*.sqlite' | head -1)"
[ -n "$snapshot" ] || die "no snapshot arrived"
mv "$snapshot" "$dest/pi.sqlite"
rmdir "$dest/backup"
chmod -R go-rwx "$dest"
find "$dest" -type d -exec chmod 0700 {} +
find "$dest" -type f -exec chmod 0600 {} +

check="$(sqlite3 -readonly "$dest/pi.sqlite" 'PRAGMA integrity_check;')"
[ "$check" = ok ] || die "the snapshot in $dest failed its integrity check: $check"
users="$(sqlite3 -readonly "$dest/pi.sqlite" 'SELECT count(*) FROM users;')"
[ -s "$dest/keys/tls_key.pem" ] || die "no TLS key in $dest/keys"
echo "backup.sh: $dest (database ok, $users users, keys)"
