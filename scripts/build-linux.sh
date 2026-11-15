#!/usr/bin/env bash
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
cd "$repo"
if [[ $(uname -s) != Linux || $(uname -m) != x86_64 ]]; then
  echo 'Linux x86_64 is required.' >&2
  exit 1
fi
if [[ -n ${RUSTFLAGS:-} || -n ${CARGO_ENCODED_RUSTFLAGS:-} || -n ${CARGO_TARGET_DIR:-} ]]; then
  echo 'Use a clean Rust flags/target environment for a sanitized release.' >&2
  exit 1
fi
flags=("--remap-path-prefix=$HOME=/build/home" "--remap-path-prefix=$repo=/moth"
       "--remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo" "--remap-path-prefix=${RUSTUP_HOME:-$HOME/.rustup}=/rustup")
export CARGO_ENCODED_RUSTFLAGS
CARGO_ENCODED_RUSTFLAGS=$(IFS=$'\x1f'; echo "${flags[*]}")
cargo build -p moth-service -p moth-maintenance --release --locked
npm run desktop:build -- --bundles deb
python3 - "$repo" <<'PY'
from pathlib import Path
import os, sys
root=Path(sys.argv[1])
for name in ('moth-service','moth-desktop','moth-maintenance'):
    data=(root/'target/release'/name).read_bytes()
    assert data[:4] == b'\x7fELF', 'Expected an ELF binary'
    for private in (str(root),os.environ['HOME'],os.environ.get('CARGO_HOME',''),os.environ.get('RUSTUP_HOME','')):
        if private and private.encode() in data:
            raise SystemExit('Release binary contains an unremapped local path')
PY
version=$(python3 -c 'import json; print(json.load(open("package.json"))["version"])')
destination=${1:?Specify a new artifact output directory}
mkdir -p "$destination"
package="$destination/Moth-$version-linux-x64.deb"
if [[ -e "$package" ]]; then
  echo 'Output exists; preserve published artifacts.' >&2
  exit 1
fi
mapfile -t packages < <(find target/release/bundle/deb -maxdepth 1 -type f -name "*_${version}_amd64.deb")
[[ ${#packages[@]} == 1 ]]
cp -- "${packages[0]}" "$package"
python3 scripts/check-linux-package.py "$package"
(cd "$destination" && sha256sum "Moth-$version-linux-x64.deb" >"Moth-$version-linux-x64.deb.sha256")
echo "Linux package: $package"
