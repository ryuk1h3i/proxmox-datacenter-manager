#!/bin/sh
set -eu

PRIVILEGED_API=/usr/libexec/proxmox/proxmox-datacenter-privileged-api
PUBLIC_API=/usr/libexec/proxmox/proxmox-datacenter-api
SOCKET=/run/proxmox-datacenter-manager/priv.sock

API_GROUP=www-data
API_USER=www-data

# Volumes and bind mounts show up as root:root 0755, but the services abort
# unless the mount points already carry the exact ownership and mode they
# would get from the Debian packaging.
prepare_dir() {
    mkdir -p "$1"
    chown "$2" "$1"
    chmod "$3" "$1"
}

prepare_dir /etc/proxmox-datacenter-manager "$API_USER:$API_GROUP" 1770
prepare_dir /var/lib/proxmox-datacenter-manager "$API_USER:$API_GROUP" 0755
prepare_dir /var/cache/proxmox-datacenter-manager "$API_USER:$API_GROUP" 0755
prepare_dir /var/log/proxmox-datacenter-manager "root:$API_GROUP" 0755
prepare_dir /run/proxmox-datacenter-manager "root:$API_GROUP" 1770

"$PRIVILEGED_API" setup
"$PRIVILEGED_API" &
privileged_pid=$!

shutdown() {
    if [ -n "${api_pid:-}" ]; then
        kill -TERM "$api_pid" 2>/dev/null || true
        wait "$api_pid" 2>/dev/null || true
    fi
    if [ -n "${privileged_pid:-}" ]; then
        kill -TERM "$privileged_pid" 2>/dev/null || true
        wait "$privileged_pid" 2>/dev/null || true
    fi
}

api_pid=
trap shutdown INT TERM EXIT

attempt=0
while [ ! -S "$SOCKET" ]; do
    if ! kill -0 "$privileged_pid" 2>/dev/null; then
        echo "privileged API exited before creating $SOCKET" >&2
        exit 1
    fi
    attempt=$((attempt + 1))
    if [ "$attempt" -ge 300 ]; then
        echo "timed out waiting for privileged API socket $SOCKET" >&2
        exit 1
    fi
    sleep 0.1
done

setpriv --reuid=www-data --regid=www-data --init-groups "$PUBLIC_API" &
api_pid=$!

while kill -0 "$privileged_pid" 2>/dev/null && kill -0 "$api_pid" 2>/dev/null; do
    sleep 1
done

exit 1
