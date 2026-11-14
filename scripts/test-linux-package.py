#!/usr/bin/env python3
"""Actual apt/dpkg installation/removal with an ordinary user's live keyring."""
import os
from pathlib import Path
import pwd
import shutil
import subprocess
import sys
import tempfile
import time


def run(*args, check=True):
    with tempfile.TemporaryFile() as output:
        result = subprocess.run(args, stdout=output, stderr=subprocess.STDOUT, timeout=180)
        output.seek(0)
        result.stdout = output.read(1024 * 1024)
    if check and result.returncode:
        raise RuntimeError(f"Package acceptance command failed: {args[0]}\n" + result.stdout.decode(errors="replace")[-2000:])
    return result


def main():
    if os.environ.get("GITHUB_ACTIONS") != "true" or os.environ.get("RUNNER_OS") != "Linux" or os.geteuid() != 0:
        raise RuntimeError("Root inside disposable hosted Linux CI is required")
    package = Path(sys.argv[1]).resolve(strict=True)
    repo = Path(__file__).resolve().parent.parent
    user = "moth-ci-package"
    home = Path("/home") / user
    if home.exists() or Path("/usr/lib/moth-desktop").exists() or run("id", user, check=False).returncode == 0 or run("dpkg-query", "-W", "-f=${Status}", "moth-desktop", check=False).returncode == 0:
        raise RuntimeError("A clean disposable package test system is required")
    created = False
    child = None
    try:
        run("useradd", "--user-group", "--no-create-home", "--home-dir", str(home), "--shell", "/usr/sbin/nologin", user)
        created = True
        account = pwd.getpwnam(user)
        home.mkdir(mode=0o700)
        os.chown(home, account.pw_uid, account.pw_gid)
        run("systemctl", "start", f"user-runtime-dir@{account.pw_uid}.service")
        fixture = home / "storage-acceptance"
        shutil.copyfile(repo / "target/debug/examples/storage_acceptance", fixture)
        fixture.chmod(0o755)
        script = home / "test-storage.sh"
        shutil.copyfile(repo / "scripts/test-package-storage-linux.sh", script)
        script.chmod(0o755)
        with (home / "acceptance.log").open("wb") as log:
            child = subprocess.Popen(["runuser", "-u", user, "--", "env", "GITHUB_ACTIONS=true", "MOTH_SECRET_SERVICE_ACCEPTANCE=1", f"XDG_RUNTIME_DIR=/run/user/{account.pw_uid}", "dbus-run-session", "--", "bash", str(script), str(home)], stdout=log, stderr=log)

            def wait(name):
                deadline = time.monotonic() + 30
                while not (home / name).exists():
                    if child.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError("Real keyring fixture did not confirm package data retention")
                    time.sleep(0.1)

            wait("ready")
            # A readable local package is needed by apt's _apt acquisition user;
            # the source artifact contains only public program files.
            package.chmod(0o644)
            for step, action in enumerate(("install", "remove", "install", "purge"), start=1):
                if action == "install":
                    run("apt-get", "install", "--yes", "--no-install-recommends", str(package))
                    version = run("dpkg-query", "-W", "-f=${Version}", "moth-desktop").stdout.decode().strip()
                    if run("/usr/bin/moth-desktop", "--version").stdout.decode().strip() != version:
                        raise RuntimeError("Installed GUI version mismatch")
                    run("/usr/lib/moth-desktop/moth-service", "--check-installed")
                    health = run("runuser", "-u", user, "--", "/usr/lib/moth-desktop/moth-service", "--status")
                    import json
                    if not json.loads(health.stdout)["systemTunnel"]:
                        raise RuntimeError("Installed package daemon unavailable to an ordinary user")
                    if not Path("/usr/share/applications/Moth Desktop.desktop").exists() and not Path("/usr/share/applications/moth-desktop.desktop").exists():
                        raise RuntimeError("Application menu entry missing")
                else:
                    run("apt-get", action, "--yes", "moth-desktop")
                    if Path("/usr/lib/moth-desktop/moth-service").exists() or Path("/usr/bin/moth-desktop").exists() or Path("/sys/class/net/moth0").exists():
                        raise RuntimeError("Uninstall left active application/tunnel files")
                (home / f"step-{step}").touch(mode=0o644)
                wait(f"verified-{step}")
            child.wait(timeout=20)
            if child.returncode:
                raise RuntimeError("Profile retention fixture failed")
        print("Actual .deb clean install, ordinary-user service access, remove/reinstall/purge and unchanged Secret Service profiles passed.")
    except Exception:
        if home.exists() and (home / "acceptance.log").exists():
            sys.stderr.write((home / "acceptance.log").read_text(errors="replace")[-3000:])
        sys.stderr.write(run("journalctl", "-u", "moth-desktop.service", "--no-pager", "-n", "30", check=False).stdout.decode(errors="replace"))
        raise
    finally:
        if child is not None and child.poll() is None:
            child.kill()
            child.wait(timeout=10)
        run("systemctl", "stop", "moth-desktop.service", check=False)
        if created:
            account = pwd.getpwnam(user)
            run("systemctl", "stop", f"user-runtime-dir@{account.pw_uid}.service", check=False)
            run("pkill", "-u", str(account.pw_uid), check=False)
            run("userdel", "--remove", user, check=False)


if __name__ == "__main__":
    main()
