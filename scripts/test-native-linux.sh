#!/usr/bin/env bash
set -euo pipefail
if [[ "${GITHUB_ACTIONS:-}" != true || "${RUNNER_OS:-}" != Linux || $(id -u) == 0 ]]; then
  echo 'An ordinary user inside disposable Linux CI is required.' >&2
  exit 1
fi
profile_file=$1
evidence=$2
mode=$3
export XDG_CONFIG_HOME="$evidence/config"
export XDG_DATA_HOME="$evidence/data"
export MOTH_DEV_DATA_DIR="$evidence"
export MOTH_NATIVE_TRAY_TEST=1
export LIBGL_ALWAYS_SOFTWARE=1
export XDG_CURRENT_DESKTOP=XFCE
export XDG_SESSION_TYPE=x11
mkdir -p "$XDG_CONFIG_HOME/xfce4/xfconf/xfce-perchannel-xml" "$XDG_DATA_HOME"
# The private bus was started before xvfb-run and these isolated XDG paths.
# Activated XFCE daemons must receive this session's display/configuration.
dbus-update-activation-environment DISPLAY XAUTHORITY XDG_CONFIG_HOME XDG_DATA_HOME XDG_RUNTIME_DIR XDG_CURRENT_DESKTOP XDG_SESSION_TYPE
keyring_password=$(openssl rand -base64 32)
eval "$(printf '%s' "$keyring_password" | gnome-keyring-daemon --unlock --components=secrets)"
unset keyring_password
export GNOME_KEYRING_CONTROL
xfwm4 --compositor=off --replace >"$evidence/window-manager.log" 2>&1 &
wm_pid=$!
panel_pid=''
trap '[[ -z "$panel_pid" ]] || kill "$panel_pid" 2>/dev/null || true; kill "$wm_pid" 2>/dev/null || true' EXIT
if [[ "$mode" == tray ]]; then
  cat >"$XDG_CONFIG_HOME/xfce4/xfconf/xfce-perchannel-xml/xfce4-panel.xml" <<'XML'
<?xml version="1.0" encoding="UTF-8"?>
<channel name="xfce4-panel" version="1.0">
  <property name="configver" type="int" value="2"/>
  <property name="panels" type="array"><value type="int" value="1"/>
    <property name="panel-1" type="empty">
      <property name="position" type="string" value="p=6;x=0;y=0"/>
      <property name="length" type="uint" value="100"/>
      <property name="plugin-ids" type="array"><value type="int" value="1"/></property>
    </property>
  </property>
  <property name="plugins" type="empty"><property name="plugin-1" type="string" value="systray"/></property>
</channel>
XML
  xfce4-panel --disable-wm-check >"$evidence/panel.log" 2>&1 &
  panel_pid=$!
  # Confirm the actual watcher before launching the application; an absent
  # desktop host must fail the fixture rather than masquerade as an app bug.
  ready=false
  for attempt in $(seq 1 100); do
    if gdbus call --session --dest org.kde.StatusNotifierWatcher --object-path /StatusNotifierWatcher --method org.freedesktop.DBus.Properties.Get org.kde.StatusNotifierWatcher IsStatusNotifierHostRegistered 2>/dev/null | grep -q true; then ready=true; break; fi
    sleep 0.1
  done
  if [[ "$ready" != true ]]; then
    xfconf-query --channel xfce4-panel --list >&2 || true
    gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus --method org.freedesktop.DBus.ListNames >&2 || true
    head -c 2000 "$evidence/panel.log" >&2 || true
    echo 'The actual XFCE StatusNotifier host did not register.' >&2
    exit 1
  fi
else
  export MOTH_NATIVE_NO_TRAY=1
fi
/usr/lib/moth-desktop/moth-desktop <"$profile_file" >"$evidence/gui.log" 2>&1
python3 - "$evidence/native-tray-result.json" "$mode" <<'PY'
import json, sys
with open(sys.argv[1]) as f:
    result = json.load(f)
assert result.get('closeKeptVpn') and result.get('secondLaunchRestored') and result.get('exitDisconnected'), 'Native lifecycle failed'
assert result.get('trayFallback') == (sys.argv[2] != 'tray'), 'Wrong native close mode'
print('Ordinary-user native Linux close, second instance and confirmed Exit passed: ' + sys.argv[2])
PY
