#!/bin/sh
# Run only after the exact native launch command has seeded its private tmpfs.
set -eu
umask 077
test "$#" -eq 5
billing_context=$1
billing_uid=$2
billing_gid=$3
billing_root=$4
billing_assets=$5
test -f /.dockerenv
test "$(id -u)" -eq 0
case "$billing_uid:$billing_gid" in *[!0-9:]*|0:*) exit 71;; esac
if ! command -v python3 >/dev/null 2>&1 || ! command -v openssl >/dev/null 2>&1 || ! command -v gh >/dev/null 2>&1 || ! command -v setpriv >/dev/null 2>&1 || ! command -v findmnt >/dev/null 2>&1; then
    command -v apt-get >/dev/null 2>&1
    apt-get update
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends python3 openssl gh util-linux
fi
billing_store="$billing_root/.decapod/data"
test "$(findmnt -n -o FSTYPE --mountpoint "$billing_store")" = tmpfs
test -d "$billing_store"
test ! -L "$billing_store"
test -f "$billing_store/decapod.db"
test ! -L "$billing_store/decapod.db"
# These two paths are the new private tmpfs and native backup copy, never host state.
chown -- "$billing_uid:$billing_gid" "$billing_store" "$billing_store/decapod.db"
exec setpriv --reuid "$billing_uid" --regid "$billing_gid" --clear-groups --no-new-privs \
    python3 "$billing_assets/container_billing.py" inside --context "$billing_context"
