#!/usr/bin/env bash
set -euo pipefail
if [[ "${GITHUB_ACTIONS:-}" != true || "${RUNNER_OS:-}" != Linux ]]; then
  echo 'Disposable Linux CI is required for Secret Service acceptance.' >&2
  exit 1
fi
# A separate bus and password-protected keyring: never touch the runner's login
# keyring, and never enable an unencrypted/empty-password keyring for this test.
export XDG_DATA_HOME
export XDG_RUNTIME_DIR
XDG_DATA_HOME=$(mktemp -d)
XDG_RUNTIME_DIR=$(mktemp -d)
trap 'rm -rf -- "$XDG_DATA_HOME" "$XDG_RUNTIME_DIR"' EXIT
dbus-run-session -- bash -euo pipefail -c '
  keyring_password=$(openssl rand -base64 32)
  eval "$(printf "%s" "$keyring_password" | gnome-keyring-daemon --unlock --components=secrets)"
  unset keyring_password
  export GNOME_KEYRING_CONTROL
  export MOTH_SECRET_SERVICE_ACCEPTANCE=1
  cargo test -p moth-core -p moth-platform -p moth-engine -p moth-ipc -p moth-service --locked -- --include-ignored
'
