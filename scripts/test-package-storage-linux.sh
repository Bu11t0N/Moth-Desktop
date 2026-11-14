#!/usr/bin/env bash
set -euo pipefail
work=$1
export XDG_DATA_HOME="$work/data"
export XDG_CONFIG_HOME="$work/config"
export MOTH_SECRET_SERVICE_ACCEPTANCE=1
mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME"
keyring_password=$(openssl rand -base64 32)
eval "$(printf '%s' "$keyring_password" | gnome-keyring-daemon --unlock --components=secrets)"
unset keyring_password
export GNOME_KEYRING_CONTROL
"$work/storage-acceptance" write
touch "$work/ready"
for step in 1 2 3 4; do
  deadline=$((SECONDS + 180))
  until [[ -f "$work/step-$step" ]]; do
    [[ $SECONDS -lt $deadline ]] || exit 1
    sleep 0.1
  done
  "$work/storage-acceptance" verify
  touch "$work/verified-$step"
done
