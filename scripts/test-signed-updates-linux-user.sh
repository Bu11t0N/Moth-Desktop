#!/usr/bin/env bash
set -euo pipefail
[[ "${GITHUB_ACTIONS:-}" == true && "${RUNNER_OS:-}" == Linux && $(id -u) != 0 ]]
home=$1
export XDG_CONFIG_HOME="$home/config"
export XDG_DATA_HOME="$home/keyring-data"
export LIBGL_ALWAYS_SOFTWARE=1
mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME"
dbus-update-activation-environment DISPLAY XAUTHORITY XDG_CONFIG_HOME XDG_DATA_HOME XDG_RUNTIME_DIR
keyring_password=$(openssl rand -base64 32)
eval "$(printf '%s' "$keyring_password" | gnome-keyring-daemon --unlock --components=secrets)"
unset keyring_password
export GNOME_KEYRING_CONTROL
xfwm4 --compositor=off --replace >"$home/window-manager.log" 2>&1 &
window_manager=$!
trap 'kill "$window_manager" 2>/dev/null || true' EXIT
python3 "$home/test-signed-updates-user.py" "$home"
