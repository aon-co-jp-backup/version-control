#!/bin/sh
# Runs on the VPS (ExecStartPre of version-control.service): copies open-easy-web's admin token into env.api (mode 600)
# so that the API-exception patrol can use the notice endpoint. Prints nothing; the value never reaches a log.
set -eu
out="$(dirname "$0")/../env.api"
tok="$(systemctl show open-easy-web -p Environment --value | tr ' ' '\n' | sed -n 's/^OPEN_EASYWEB_DIST_SYNC_ADMIN_TOKEN=//p' | head -n1)"
if [ -z "$tok" ]; then
  rm -f "$out"
  exit 0   # the patrol then reports "token not set" and carries on
fi
umask 077
printf 'OPEN_EASYWEB_DIST_SYNC_ADMIN_TOKEN=%s\n' "$tok" > "$out"
