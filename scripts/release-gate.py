#!/usr/bin/env python3
"""Require all native checks for the exact source revision before signing."""
import json
import os
from pathlib import Path
import subprocess
import sys
import urllib.request


def main():
    if os.environ.get("GITHUB_ACTIONS") != "true" or os.environ.get("GITHUB_REF") != "refs/heads/main":
        raise RuntimeError("Manual release from main in hosted Actions is required")
    run = sys.argv[1]
    if not run.isdecimal():
        raise RuntimeError("A numeric accepted workflow run is required")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    base = "https://api.github.com/repos/Bu11t0N/Moth-Desktop"

    def get(endpoint):
        request = urllib.request.Request(base + endpoint, headers={"Authorization": "Bearer " + os.environ["GH_TOKEN"], "Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28"})
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response)

    accepted = get("/actions/runs/" + run)
    if accepted["head_sha"] != revision or accepted["path"] != ".github/workflows/checks.yml" or accepted["status"] != "completed" or accepted["conclusion"] != "success":
        raise RuntimeError("The exact release revision has not passed Desktop checks")
    jobs = get("/actions/runs/" + run + "/jobs?filter=latest&per_page=100")["jobs"]
    required = {"windows", "windows-installer", "linux-core", "signed-updates-windows", "signed-updates-linux"}
    if {job["name"] for job in jobs if job["conclusion"] == "success"} != required:
        raise RuntimeError("All five native acceptance jobs must pass")
    root = Path(__file__).resolve().parent.parent
    version = json.loads((root / "package.json").read_text())["version"]
    if not version or any(part == "" or not part.isdecimal() for part in version.split(".")) or len(version.split(".")) != 3:
        raise RuntimeError("Invalid release version")
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        output.write("version=" + version + "\nrevision=" + revision + "\n")
    print("All native checks passed for the exact source revision.")


if __name__ == "__main__":
    main()
