#!/usr/bin/env python3
"""Inspect the actual Debian archive before any installation is attempted."""
from pathlib import Path
import hashlib
import json
import subprocess
import sys
import tarfile

package=Path(sys.argv[1]).resolve(strict=True)
root="usr/lib/moth-desktop/"
required={"usr/bin/moth-desktop",root+"moth-service",root+"moth-maintenance",root+"engines/xray",root+"engines/sing-box","usr/lib/systemd/system/moth-desktop.service","usr/share/applications/moth-desktop.desktop"}
found=set()
pin=json.loads((Path(__file__).resolve().parent.parent/'policy-assets.lock.json').read_text(encoding='utf-8'))
policy_files={root+'engines/'+p['file']:p['sha256'] for p in [pin['platforms']['linux-x64'], *pin['geo']]}
required.update(policy_files)
required.update({'usr/lib/systemd/system/moth-egress-guard.service',
    'usr/lib/systemd/system/NetworkManager.service.d/50-moth-egress-guard.conf',
    'usr/lib/systemd/system/systemd-networkd.service.d/50-moth-egress-guard.conf'})
process=subprocess.Popen(["dpkg-deb","--fsys-tarfile",str(package)],stdout=subprocess.PIPE)
with tarfile.open(fileobj=process.stdout,mode="r|*") as archive:
    for entry in archive:
        name=entry.name.removeprefix("./").rstrip("/")
        if name in found:
            raise RuntimeError("Duplicate package entry")
        found.add(name)
        if name in required or name.startswith(root):
            if entry.uid!=0 or entry.gid!=0 or entry.mode & 0o022 or not(entry.isfile() or entry.isdir()):
                raise RuntimeError("Unsafe package ownership, mode or file type")
            if name in required and not entry.isfile():
                raise RuntimeError("Required package file is not regular")
            if name in policy_files:
                with archive.extractfile(entry) as content:
                    if hashlib.file_digest(content,'sha256').hexdigest()!=policy_files[name]:
                        raise RuntimeError("Packaged policy asset checksum mismatch")
if process.wait(timeout=10)!=0:
    raise RuntimeError("Could not inspect Debian payload")
if required-found:
    raise RuntimeError("Missing fixed-layout package files: "+", ".join(sorted(required-found)))
print("Debian GUI, daemon, updater, pinned-core, desktop-entry and unit paths/ownership verified.")
