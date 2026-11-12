#!/bin/sh
# Run only after the exact native launch command has seeded its private tmpfs.
set -eu
umask 077
test "$#" -eq 6
billing_context=$1
billing_uid=$2
billing_gid=$3
billing_root=$4
billing_assets=$5
billing_engine=$6
case "$billing_engine" in
    docker) test -f /.dockerenv;;
    podman) test -f /run/.containerenv;;
    *) exit 71;;
esac
test "$(id -u)" -eq 0
case "$billing_uid:$billing_gid" in *[!0-9:]*|0:*) exit 71;; esac
if ! command -v python3 >/dev/null 2>&1 || ! command -v openssl >/dev/null 2>&1 || ! command -v setpriv >/dev/null 2>&1 || ! command -v findmnt >/dev/null 2>&1; then
    command -v apt-get >/dev/null 2>&1
    apt-get update
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends python3 openssl util-linux
fi
test -x /usr/local/bin/gh
billing_store="$billing_root/.decapod/data"
test "$(findmnt -n -o FSTYPE --mountpoint "$billing_store")" = tmpfs
test -d "$billing_store"
test ! -L "$billing_store"
test -f "$billing_store/decapod.db"
test ! -L "$billing_store/decapod.db"
# These two paths are the new private tmpfs and native backup copy, never host state.
if [ "$billing_engine" = docker ]; then
    chown -- "$billing_uid:$billing_gid" "$billing_store" "$billing_store/decapod.db"
    exec setpriv --reuid "$billing_uid" --regid "$billing_gid" --clear-groups --no-new-privs \
        python3 "$billing_assets/container_billing.py" inside --context "$billing_context"
fi
# Default rootless Podman namespace0 already maps to the unprivileged host owner.
# The child verifies both kernel maps and the engine-issued ID before importing custody.
exec setpriv --no-new-privs python3 "$billing_assets/container_billing.py" inside --context "$billing_context"
