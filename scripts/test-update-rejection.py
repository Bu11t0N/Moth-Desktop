#!/usr/bin/env python3
"""Run the installed privileged verifier against an unsigned update in CI only."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import uuid

if os.environ.get("GITHUB_ACTIONS") != "true" or os.environ.get("RUNNER_OS") not in ("Windows", "Linux"):
    raise RuntimeError("Disposable hosted CI is required")
helper=Path(sys.argv[1]).resolve(strict=True)
state=Path(sys.argv[2]).resolve(strict=True)
with tempfile.TemporaryDirectory(prefix="moth-unsigned-update-") as temporary:
    identifier=str(uuid.uuid4())
    request=Path(temporary)/identifier
    request.mkdir(mode=0o700)
    (request/"manifest.json").write_bytes(b"{}")
    (request/"manifest.sig").write_bytes(b"not-a-signature")
    (request/"package.bin").write_bytes(b"Unsigned fixture must never execute")
    result=subprocess.run([str(helper),"--apply",str(request)],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=40)
    if result.returncode==0:
        raise RuntimeError("Privileged updater accepted unsigned input")
    report=json.loads((state/"updates"/identifier/"status.json").read_bytes())
    if report["phase"]!="failed":
        raise RuntimeError("Privileged updater did not report unsigned-input refusal")
print("Installed privileged updater rejected unsigned input before installation.")
