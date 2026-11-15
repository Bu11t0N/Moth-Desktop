#!/usr/bin/env python3
"""Generate or check path-free Rust and bundled JavaScript license notices."""
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
INPUTS = ("Cargo.lock", "package-lock.json", "about.toml", "packaging/licenses.hbs")
OUTPUTS = ("licenses/Rust-dependencies.html", "licenses/tauri-api-MIT.txt")


def digest(name):
    # Git may check text out with CRLF on Windows and LF on Linux.
    return hashlib.sha256((ROOT / name).read_text(encoding="utf-8").encode("utf-8")).hexdigest()


def main():
    record = ROOT / "licenses/dependency-inventory.json"
    if len(sys.argv) == 3 and sys.argv[1] == "generate":
        tool = sys.argv[2]
        version = subprocess.check_output([tool, "--version"], text=True).strip()
        if version != "cargo-about 0.9.3":
            raise RuntimeError("cargo-about 0.9.3 is required")
        subprocess.run([tool, "generate", "--workspace", "--locked", "--fail", "-o", OUTPUTS[0], "packaging/licenses.hbs"], cwd=ROOT, check=True)
        report = ROOT / OUTPUTS[0]
        report.write_text("\n".join(line.rstrip(" \t") for line in report.read_text(encoding="utf-8").splitlines()) + "\n", encoding="utf-8")
        api = ROOT / "node_modules/@tauri-apps/api"
        package = json.loads((api / "package.json").read_text())
        lock = json.loads((ROOT / "package-lock.json").read_text())
        runtime = {name: item for name, item in lock["packages"].items() if name and not item.get("dev")}
        if set(runtime) != {"node_modules/@tauri-apps/api"} or runtime["node_modules/@tauri-apps/api"]["version"] != package["version"]:
            raise RuntimeError("Review notices for changed JavaScript runtime dependencies")
        shutil.copyfile(api / "LICENSE-MIT", ROOT / OUTPUTS[1])
        record.write_text(json.dumps({"generator": version, "inputs": {name: digest(name) for name in INPUTS}, "outputs": {name: digest(name) for name in OUTPUTS}, "javascript": {"@tauri-apps/api": package["version"]}}, indent=2) + "\n", encoding="utf-8")
    elif len(sys.argv) != 1:
        raise RuntimeError("Usage: license-notices.py [generate PATH_TO_CARGO_ABOUT]")
    inventory = json.loads(record.read_text())
    for group in ("inputs", "outputs"):
        for name, expected in inventory[group].items():
            if digest(name) != expected:
                raise RuntimeError("Dependency notices need regeneration: " + name)
    report = (ROOT / OUTPUTS[0]).read_text(encoding="utf-8")
    if re.search(r"(?:file:///|\b[A-Za-z]:[\\/]|/home/runner/|/Users/)", report):
        raise RuntimeError("Local path in generated license report")
    engines = json.loads((ROOT / "licenses/engine-inventory.json").read_text())
    archive = ROOT / engines["archive"]
    if (digest("engines.lock.json") != engines["engineLockSha256"]
            or hashlib.sha256(archive.read_bytes()).hexdigest() != engines["archiveSha256"]):
        raise RuntimeError("Pinned engine notices need regeneration")
    print("Locked Rust, JavaScript and pinned engine notices match their reviewed inputs.")


if __name__ == "__main__":
    main()
