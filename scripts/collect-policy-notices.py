"""Collect notices from Go modules recorded inside both pinned Mihomo builds."""
import argparse
import concurrent.futures
import gzip
import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import tarfile
import urllib.request

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument("--go", required=True)
args = parser.parse_args()
lock = json.loads((root / "policy-assets.lock.json").read_text(encoding="utf-8"))
notices = {}
modules = {}
index = {"upstream": lock["source"], "goBinaries": {}, "modules": {}}

def digest(data):
    return hashlib.sha256(data).hexdigest()

def get(url):
    with urllib.request.urlopen(url, timeout=30) as response:
        return response.read()

for platform, pin in lock["platforms"].items():
    binary = root / ".local/engines" / platform / pin["file"]
    if digest(binary.read_bytes()) != pin["sha256"]:
        raise RuntimeError("Mihomo executable pin mismatch")
    metadata = json.loads(subprocess.check_output([args.go, "version", "-m", "-json", binary]))
    index["goBinaries"][platform] = metadata
    go = metadata["GoVersion"]
    go_license = f"Go/{go}/LICENSE"
    if go_license not in notices:
        notices[go_license] = get(f"https://raw.githubusercontent.com/golang/go/{go}/LICENSE")
    for entry in metadata["Deps"]:
        dep = entry.get("Replace", entry)
        key = dep["Path"] + "@" + dep["Version"]
        if key in modules and modules[key] != dep["Sum"]:
            raise RuntimeError("Conflicting dependency module checksum")
        modules[key] = dep["Sum"]

def collect(item):
    module, checksum = item
    metadata = json.loads(subprocess.check_output([args.go, "mod", "download", "-json", module]))
    if metadata.get("Sum") != checksum:
        raise RuntimeError("Go module checksum mismatch: " + module)
    folder = Path(metadata["Dir"])
    files = {}
    for p in folder.rglob("*"):
        if p.is_file() and re.match(r"(?i)^(?:licen[cs]e|copying|notice|copyright|authors)(?:[._-].*)?$", p.name):
            files[p.relative_to(folder).as_posix()] = p.read_bytes()
    if not files:
        raise RuntimeError("Review missing license: " + module)
    return module, checksum, files

with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
    for module, checksum, files in pool.map(collect, sorted(modules.items())):
        index["modules"][module] = {"sum": checksum, "files": sorted(files)}
        for file, data in files.items():
            notices[f"modules/{module}/{file}"] = data
for filename in ("LICENSE", "go.mod", "go.sum"):
    notices[f"Mihomo/v{lock['version']}/{filename}"] = get(
        f"https://raw.githubusercontent.com/MetaCubeX/mihomo/v{lock['version']}/{filename}"
    )
for project, revision in (("geoip", "202611080027"), ("domain-list-community", "20261118141523")):
    notices[f"v2fly/{project}/LICENSE"] = get(f"https://raw.githubusercontent.com/v2fly/{project}/{revision}/LICENSE")
# The Go tool's leading executable path is local to whoever ran this collector.
for data in index["goBinaries"].values():
    data.pop("Dir", None)
notices["SOURCE-INDEX.json"] = (json.dumps(index, indent=2) + "\n").encode()
destination = root / "licenses/Policy-engine-notices.tar.gz"
with destination.open("wb") as file, gzip.GzipFile(filename="", fileobj=file, mode="wb", mtime=0) as zipped:
    with tarfile.open(fileobj=zipped, mode="w") as archive:
        for path, data in sorted(notices.items()):
            member = tarfile.TarInfo(path)
            member.size = len(data)
            member.mode = 0o644
            archive.addfile(member, io.BytesIO(data))
record = {
    "lockSha256": digest((root / "policy-assets.lock.json").read_text(encoding="utf-8").encode("utf-8")),
    "archiveSha256": digest(destination.read_bytes()),
    "modules": len(modules),
    "files": len(notices),
}
(root / "licenses/policy-engine-inventory.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
print(f"Checked {len(modules)} Go modules and bundled {len(notices)} policy engine notices")
