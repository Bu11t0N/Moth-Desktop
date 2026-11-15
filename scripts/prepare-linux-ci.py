#!/usr/bin/env python3
"""Select the supported DNS setup in a disposable Ubuntu Actions guest."""
import os
from pathlib import Path
import subprocess
import uuid


def main():
    if (os.environ.get("GITHUB_ACTIONS") != "true"
            or os.environ.get("RUNNER_OS") != "Linux" or os.geteuid() != 0):
        raise RuntimeError("Root in a disposable hosted Linux CI guest is required")
    release = dict(line.split("=", 1) for line in Path("/etc/os-release").read_text().splitlines() if "=" in line)
    if release.get("ID", "").strip('"') != "ubuntu" or release.get("VERSION_ID", "").strip('"') != "24.04":
        raise RuntimeError("Only the supported Ubuntu 24.04 guest may be prepared")
    subprocess.run(["systemctl", "start", "systemd-resolved.service"], check=True)
    stub = Path("/run/systemd/resolve/stub-resolv.conf")
    if not stub.is_file():
        raise RuntimeError("The real resolved stub did not become available")
    resolver = Path("/etc/resolv.conf")
    if resolver.resolve() != stub:
        temporary = resolver.with_name(".moth-ci-resolver-" + str(uuid.uuid4()))
        try:
            temporary.symlink_to(stub)
            os.replace(temporary, resolver)
        finally:
            temporary.unlink(missing_ok=True)
    subprocess.run(["resolvectl", "query", "github.com"], check=True, stdout=subprocess.DEVNULL, timeout=30)
    print("Disposable Ubuntu guest uses the real systemd-resolved stub; network snapshots follow preparation.")


if __name__ == "__main__":
    main()
