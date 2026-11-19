#!/usr/bin/env python3
"""Real root daemon/TUN acceptance, exclusively in a disposable hosted Ubuntu VM."""
import http.server
import ipaddress
import json
import os
import pwd
from pathlib import Path
import secrets
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
import uuid


def sink():
    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            self.send_response(204)
            self.send_header("Connection", "close")
            self.end_headers()

        def log_message(self, *_):
            pass

    class Server6(http.server.ThreadingHTTPServer):
        address_family = socket.AF_INET6

    for family, address in [(socket.AF_INET, "10.231.90.2"),
                            (socket.AF_INET6, "fd24:4d4f:5448::2")]:
        server = (Server6 if family == socket.AF_INET6 else http.server.ThreadingHTTPServer)((address, 18080), Handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        udp = socket.socket(family, socket.SOCK_DGRAM)
        udp.bind((address, 18081))

        def echo(sock):
            while True:
                data, peer = sock.recvfrom(2048)
                sock.sendto(data, peer)

        threading.Thread(target=echo, args=(udp,), daemon=True).start()
    while True:
        time.sleep(1)


def run(*args, data=None, check=True, timeout=40):
    # Some account/service tools start descendants inheriting output handles.
    # Wait for the actual command, not EOF on a pipe held by a daemon.
    with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
        result = subprocess.run(args, input=data, stdout=stdout, stderr=stderr, timeout=timeout)
        stdout.seek(0)
        stderr.seek(0)
        result.stdout = stdout.read(1024 * 1024)
        result.stderr = stderr.read(1024 * 1024)
    if check and result.returncode:
        # Request/config contents and stderr are deliberately not included.
        raise RuntimeError(f"Command failed: {args[0]} ({result.returncode})")
    return result


def cli(user, action, data=None):
    result = run("runuser", "-u", user, "--", "/usr/lib/moth-desktop/moth-service", action, data=data)
    return json.loads(result.stdout)


def wait(test, reason, seconds=20):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            if test():
                return
        except (RuntimeError, json.JSONDecodeError, FileNotFoundError):
            pass
        time.sleep(0.1)
    raise RuntimeError(reason)


def snapshot():
    result = {}
    for family in ("-4", "-6"):
        result[family] = run("ip", family, "-N", "-j", "rule", "show").stdout
        routes = json.loads(run("ip", family, "-N", "-j", "route", "show", "table", "main").stdout)
        for route in routes:
            route.pop("expires", None)
        result[family + "routes"] = json.dumps(routes, sort_keys=True)
    for kind in ("dns", "domain", "default-route"):
        result[kind] = run("resolvectl", kind).stdout
    result["resolv.conf"] = Path("/etc/resolv.conf").read_bytes()
    return result


def main():
    if os.environ.get("GITHUB_ACTIONS") != "true" or os.environ.get("RUNNER_OS") != "Linux" or os.geteuid() != 0:
        raise RuntimeError("Root inside a disposable hosted Linux CI VM is required")
    root = Path("/usr/lib/moth-desktop")
    unit = Path("/etc/systemd/system/moth-desktop.service")
    guardian = Path("/etc/systemd/system/moth-egress-guard.service")
    if guardian.exists():
        raise RuntimeError("An existing guardian unit must not be replaced")
    if root.exists() or unit.exists() or Path("/sys/class/net/moth0").exists():
        raise RuntimeError("Existing installation/interface must never be replaced by acceptance")
    if Path("/etc/resolv.conf").resolve() != Path("/run/systemd/resolve/stub-resolv.conf"):
        raise RuntimeError("The supported resolved stub configuration is required")
    repo = Path(__file__).resolve().parent.parent
    users = ("moth-ci-owner", "moth-ci-other")
    children = []
    created_users = []
    namespace = False
    veth = False
    os.umask(0o077)
    with tempfile.TemporaryDirectory(prefix="moth-linux-acceptance-") as temporary:
        work = Path(temporary)
        try:
            for user, uid in zip(users, (23101, 23102)):
                if run("id", user, check=False).returncode == 0 or run("getent", "passwd", str(uid), check=False).returncode == 0:
                    raise RuntimeError("Acceptance account already exists")
                home = Path("/home") / user
                if home.exists():
                    raise RuntimeError("Acceptance home already exists")
                run("useradd", "--uid", str(uid), "--user-group", "--no-create-home", "--home-dir", str(home), "--shell", "/usr/sbin/nologin", user)
                created_users.append((user, uid))
                account = pwd.getpwnam(user)
                if account.pw_uid != uid:
                    raise RuntimeError("Acceptance account has the wrong UID")
                home.mkdir(mode=0o700)
                os.chown(home, uid, account.pw_gid)
                # Keep a real user manager across short runuser/PAM sessions,
                # like the persistent runtime of a logged-in desktop account.
                run("loginctl", "enable-linger", user)
                run("systemctl", "start", f"user@{uid}.service")
                wait(lambda: Path(f"/run/user/{uid}").is_dir(), "User manager runtime was not created")
            run("ip", "netns", "add", "moth-ci-sink")
            namespace = True
            run("ip", "link", "add", "moth-ci-host", "type", "veth", "peer", "name", "moth-ci-peer")
            veth = True
            run("ip", "link", "set", "moth-ci-peer", "netns", "moth-ci-sink")
            run("ip", "addr", "add", "10.231.90.1/30", "dev", "moth-ci-host")
            run("ip", "-6", "addr", "add", "fd24:4d4f:5448::1/126", "dev", "moth-ci-host", "nodad")
            run("ip", "link", "set", "moth-ci-host", "up")
            for args in [("addr", "add", "10.231.90.2/30", "dev", "moth-ci-peer"),
                         ("-6", "addr", "add", "fd24:4d4f:5448::2/126", "dev", "moth-ci-peer", "nodad"),
                         ("link", "set", "moth-ci-peer", "up"), ("link", "set", "lo", "up")]:
                run("ip", "-n", "moth-ci-sink", *args)
            # Copy the non-secret fixture program into the root-only work directory.
            fixture = work / "fixture.py"
            shutil.copyfile(__file__, fixture)
            children.append(subprocess.Popen(["ip", "netns", "exec", "moth-ci-sink", sys.executable, str(fixture), "sink"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
            # Establish that both isolated servers really started before testing
            # VPN routing. A tunnel timeout must not conceal a dead fixture.
            for destination in ("10.231.90.2", "fd24:4d4f:5448::2"):
                host = f"[{destination}]" if ":" in destination else destination
                wait(lambda: run("curl", "--fail", "--silent", "--noproxy", "*", "--max-time", "2", f"http://{host}:18080/ready", check=False).returncode == 0,
                     "Isolated TCP fixture did not become reachable")
            root.mkdir(mode=0o755)
            root.chmod(0o755)  # Explicitly override the fixture's private umask.
            (root / "engines").mkdir(mode=0o755)
            (root / "engines").chmod(0o755)
            settings_example=root/'desktop-settings-acceptance'
            shutil.copyfile(repo/'target/debug/examples/desktop_settings_acceptance',settings_example)
            settings_example.chmod(0o755)
            settings_result=run('runuser','-u',users[0],'--','env','GITHUB_ACTIONS=true','RUNNER_OS=Linux',
                f'HOME=/home/{users[0]}',f'XDG_CONFIG_HOME=/home/{users[0]}/.config',
                'XDG_CURRENT_DESKTOP=GNOME',f'XDG_RUNTIME_DIR=/run/user/23101',
                'dbus-run-session','--',str(settings_example),check=False)
            if settings_result.returncode:
                # This fresh-account fixture emits fixed stage labels and
                # static failures only; it never prints the ownership journal.
                raise RuntimeError('Ordinary-user settings acceptance failed:\n'+
                    settings_result.stdout.decode(errors='replace')[-2000:]+
                    settings_result.stderr.decode(errors='replace')[-2000:])
            for source, destination in [(repo / "target/debug/moth-service", root / "moth-service"),
                                        (repo / ".local/engines/linux-x64/xray", root / "engines/xray"),
                                        (repo / ".local/engines/linux-x64/sing-box", root / "engines/sing-box"),
                                        *((repo / ".local/engines/linux-x64" / asset, root / "engines" / asset)
                                          for asset in ("mihomo", "GeoIP.dat", "GeoSite.dat"))]:
                shutil.copyfile(source, destination)
                destination.chmod(0o755)
            shutil.copyfile(repo / "packaging/linux/moth-desktop.service", unit)
            unit.chmod(0o644)
            shutil.copyfile(repo / "packaging/linux/moth-egress-guard.service", guardian)
            guardian.chmod(0o644)
            run(str(root / "moth-service"), "--prepare-install")
            run("systemctl", "daemon-reload")
            server_id = str(uuid.uuid4())
            listener = socket.socket()
            listener.bind(("127.0.0.1", 0))
            port = listener.getsockname()[1]
            listener.close()
            config = {"log": {"loglevel": "none"},
                      "inbounds": [{"listen": "127.0.0.1", "port": port, "protocol": "vless", "settings": {"decryption": "none", "clients": [{"id": server_id}]}}],
                      # Xray's built-in VLESS egress policy blocks private IPs.
                      # Permit only this isolated synthetic TCP/UDP destination.
                      "outbounds": [{"protocol": "freedom", "settings": {"finalRules": [{"action": "allow", "ip": ["10.231.90.2/32", "fd24:4d4f:5448::2/128"], "port": "18080-18081"}]}, "streamSettings": {"sockopt": {"mark": 0x4D4F5448}}}]}
            config_path = work / "server.json"
            config_path.write_text(json.dumps(config))
            children.append(subprocess.Popen([str(root / "engines/xray"), "run", "-config", str(config_path)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
            profile = f"vless://{server_id}@127.0.0.1:{port}?encryption=none&security=none&type=tcp".encode()
            baseline = snapshot()
            run("systemctl", "start", "moth-desktop.service")
            wait(lambda: cli(users[0], "--status")["systemTunnel"], "Ordinary-user authenticated daemon health failed")
            lease_example=str(repo / "target/debug/examples/maintenance_acceptance")
            run(lease_example,"available")
            private_input = Path(run(lease_example, "private-create").stdout.decode().strip())
            private_reader = root / "maintenance-acceptance"
            shutil.copyfile(lease_example, private_reader)
            private_reader.chmod(0o755)
            try:
                result = run("runuser", "-u", users[0], "--", "env", "GITHUB_ACTIONS=true", str(private_reader), "private-denied", str(private_input), check=False)
                if result.returncode:
                    raise RuntimeError("Private input read-denial fixture failed: " + result.stderr.decode(errors="replace")[-1000:])
            finally:
                private_input.unlink()
                private_reader.unlink()
            print("Actual ordinary-user private maintenance-input denial passed", flush=True)

            def connected():
                result = cli(users[0], "--connect", profile)
                if result.get("phase") != "connected" or not result.get("ownedByCaller"):
                    raise RuntimeError("Real service HTTPS/TUN verification failed")

            def restored():
                wait(lambda: not Path("/sys/class/net/moth0").exists(), "Private TUN survived teardown")
                # resolved consumes link removal asynchronously; require exact
                # baseline restoration after it has had time to settle.
                try:
                    wait(lambda: not Path("/run/moth-desktop/network-v1.json").exists() and snapshot() == baseline,
                         "Network rules, routes or physical DNS were not preserved", seconds=8)
                except RuntimeError:
                    current=snapshot()
                    print("Network journal remains:",Path("/run/moth-desktop/network-v1.json").exists(),file=sys.stderr)
                    for key in baseline:
                        if baseline[key]!=current[key]:
                            print(f"Acceptance baseline changed ({key}): before={baseline[key]!r} after={current[key]!r}",file=sys.stderr)
                    raise

            # Cancel a genuine in-flight TUN startup with a rejected synthetic
            # VLESS credential; the production HTTPS verifier cannot complete.
            rejected_profile=f"vless://{uuid.uuid4()}@127.0.0.1:{port}?encryption=none&security=none&type=tcp".encode()
            pending=subprocess.Popen(["runuser","-u",users[0],"--",str(root/"moth-service"),"--connect"],stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            children.append(pending)
            pending.stdin.write(rejected_profile)
            pending.stdin.close()
            wait(lambda: Path("/sys/class/net/moth0").exists() and cli(users[0],"--tunnel-status").get("phase")=="connecting",
                 "Real in-flight TUN startup was not observed")
            if cli(users[1],"--disconnect").get("code")!="notOwner":
                raise RuntimeError("Another UID cancelled startup")
            if cli(users[0],"--disconnect").get("phase")!="disconnected":
                raise RuntimeError("Owner cancellation was not confirmed")
            if pending.wait(timeout=10)!=0:
                raise RuntimeError("Cancelled connection client failed")
            restored()
            run(lease_example,"available")
            connected()
            run(lease_example,"blocked")
            if cli(users[1], "--disconnect").get("code") != "notOwner":
                raise RuntimeError("Another UID disconnected the owner's tunnel")
            for source, destination in [("172.19.0.1", "10.231.90.2"), ("fdfe:dcba:9876::1", "fd24:4d4f:5448::2")]:
                print(f"Testing actual TUN TCP/UDP IPv{ipaddress.ip_address(source).version}", flush=True)
                host = f"[{destination}]" if ":" in destination else destination
                response = run("curl", "--fail", "--silent", "--noproxy", "*", "--interface", source, "--max-time", "10", "--output", "/dev/null", "--write-out", "%{http_code}", f"http://{host}:18080/actual-tun")
                if response.stdout != b"204":
                    raise RuntimeError("Real TUN TCP fixture failed")
                family = socket.AF_INET6 if ipaddress.ip_address(source).version == 6 else socket.AF_INET
                with socket.socket(family, socket.SOCK_DGRAM) as udp:
                    udp.settimeout(10)
                    udp.bind((source, 0))
                    payload = secrets.token_bytes(32)
                    udp.sendto(payload, (destination, 18081))
                    if udp.recv(2048) != payload:
                        raise RuntimeError("Real TUN UDP fixture failed")
            answers = run("dig", "@172.19.0.2", "example.com", "+short", "+tries=1", "+time=8").stdout.splitlines()
            if not answers or not any(ipaddress.ip_address(answer.decode()).version == 4 for answer in answers):
                raise RuntimeError("Private TUN DNS returned no address")
            run("resolvectl", "query", "example.com")
            cli(users[0], "--disconnect")
            restored()
            run(lease_example,"available")
            connected()
            main_pid = run("systemctl", "show", "moth-desktop.service", "--property=MainPID", "--value").stdout.strip()
            engine_pid = run("pgrep", "-P", main_pid.decode(), "-x", "xray").stdout.strip()
            run("kill", "-KILL", engine_pid.decode())
            wait(lambda: cli(users[0], "--tunnel-status").get("phase") == "failed", "Engine death was not reported")
            restored()
            connected()
            run("systemctl", "kill", "--kill-whom=main", "--signal=SIGKILL", "moth-desktop.service")
            wait(lambda: run("systemctl", "show", "moth-desktop.service", "--property=MainPID", "--value").stdout.strip() not in (b"0", main_pid) and cli(users[0], "--status")["systemTunnel"], "Daemon did not restart after SIGKILL")
            restored()
            connected()
            cli(users[0], "--disconnect")
            restored()
            # Persistent nftables acceptance against previously reachable real
            # Authenticated LAN clients from an actual isolated peer namespace.
            access_example=root/'proxy-access-acceptance'
            shutil.copyfile(repo/'target/debug/examples/proxy_access_acceptance',access_example)
            access_example.chmod(0o755)
            run('ip','addr','add','10.231.91.1/30','dev','moth-ci-host')
            run('ip','-n','moth-ci-sink','addr','add','10.231.91.2/30','dev','moth-ci-peer')
            listener={'listen':'10.231.90.1','port':10808,'allowedClients':['10.231.90.2/32']}
            result=cli(users[0],'--connect-proxy',json.dumps({'link':profile.decode(),'mode':'proxy','listener':listener}).encode())
            if result.get('phase')!='connected' or result.get('mode')!='proxy':
                raise RuntimeError('Real proxy-only startup failed')
            if cli(users[0],'--tunnel-status').get('phase')=='connected' or Path('/sys/class/net/moth0').exists():
                raise RuntimeError('Proxy-only was misreported or created a TUN')
            # Private credentials stay in a captured pipe, never logs or argv.
            access=json.loads(run('runuser','-u',users[0],'--','env','GITHUB_ACTIONS=true',str(access_example),'export').stdout)
            for source,expected in [('10.231.90.2',True),('10.231.91.2',False)]:
                payload=json.dumps({'access':access,'source':source,'expected':expected}).encode()
                run('ip','netns','exec','moth-ci-sink','env','GITHUB_ACTIONS=true',str(access_example),'client',data=payload)
            access.clear()
            cli(users[0],'--disconnect')
            run('ip','addr','del','10.231.91.1/30','dev','moth-ci-host')
            run('ip','-n','moth-ci-sink','addr','del','10.231.91.2/30','dev','moth-ci-peer')
            restored()
            clash_example=root/'clash-policy-acceptance'
            shutil.copyfile(repo/'target/debug/examples/clash_policy_acceptance',clash_example)
            clash_example.chmod(0o755)
            def clash_check(*arguments):
                outcome=run('runuser','-u',users[0],'--','env','GITHUB_ACTIONS=true',str(clash_example),*arguments,check=False)
                if outcome.returncode:
                    raise RuntimeError("Clash fixture failed: "+outcome.stderr.decode(errors='replace')[-1600:])
                print(outcome.stdout.decode(errors='replace').strip(),flush=True)
            clash_check()
            restored()
            # Persistent nftables acceptance against previously reachable real
            # IPv4/IPv6 peers. No missing-route timeout is counted as a pass.
            connected()
            traffic=cli(users[0],"--traffic")
            if traffic.get("kind")!="traffic" or traffic.get("uploadedBytes",0)<=0:
                raise RuntimeError("Actual service traffic counters missing")
            if cli(users[1],"--traffic").get("code")!="notOwner":
                raise RuntimeError("Another user obtained traffic summaries")
            armed=cli(users[0],"--guard-on")
            if not armed.get("enabled") or not armed.get("filtering"):
                raise RuntimeError("Actual nftables guard was not verified")
            if cli(users[1],"--guard-off").get("code")!="notOwner":
                raise RuntimeError("Another user released the guard")
            cli(users[0],"--disconnect")
            clash_check("guard")
            def blocked():
                for destination in ("10.231.90.2","fd24:4d4f:5448::2"):
                    host=f"[{destination}]" if ":" in destination else destination
                    if run("curl","--fail","--silent","--noproxy","*","--interface","moth-ci-host","--max-time","2",f"http://{host}:18080/blocked",check=False).returncode==0:
                        raise RuntimeError("Direct IPv4/IPv6 escaped the persistent guard")
                if not cli(users[0],"--guard-status").get("filtering"):
                    raise RuntimeError("Persistent rules disappeared")
            blocked()
            old_pid=run("systemctl","show","moth-desktop.service","--property=MainPID","--value").stdout.strip()
            run("systemctl","kill","--kill-whom=main","--signal=SIGKILL","moth-desktop.service")
            wait(lambda:run("systemctl","show","moth-desktop.service","--property=MainPID","--value").stdout.strip() not in (b"0",old_pid) and cli(users[0],"--guard-status").get("filtering"),"Guard did not survive service death/restart")
            blocked()
            run("systemctl","stop","moth-desktop.service")
            run(str(root/"moth-service"),"--restore-guard")
            run(str(root/"moth-service"),"--release-guard")
            run("systemctl","start","moth-desktop.service")
            for destination in ("10.231.90.2","fd24:4d4f:5448::2"):
                host=f"[{destination}]" if ":" in destination else destination
                if run("curl","--fail","--silent","--noproxy","*","--interface","moth-ci-host","--max-time","3",f"http://{host}:18080/released").returncode:
                    raise RuntimeError("Administrator release did not restore direct peers")
            restored()
            # Actual GTK/WebKit windows under an ordinary user, with a real XFCE
            # tray host and separately without any tray host. No fake IPC state.
            gui = root / "moth-desktop"
            shutil.copyfile(repo / "target/debug/moth-desktop", gui)
            gui.chmod(0o755)
            native_script = root / "test-native-linux.sh"
            shutil.copyfile(repo / "scripts/test-native-linux.sh", native_script)
            native_script.chmod(0o755)
            for mode in ("tray", "taskbar"):
                evidence = Path("/home") / users[0] / ("native-" + mode)
                evidence.mkdir(mode=0o700)
                os.chown(evidence, 23101, 23101)
                profile_file = evidence / "fixture-profile"
                profile_file.write_bytes(profile)
                profile_file.chmod(0o600)
                os.chown(profile_file, 23101, 23101)
                try:
                    run("runuser", "-u", users[0], "--", "env", "GITHUB_ACTIONS=true", "RUNNER_OS=Linux", "XDG_RUNTIME_DIR=/run/user/23101", "dbus-run-session", "--", "xvfb-run", "-a", "bash", str(native_script), str(profile_file), str(evidence), mode, timeout=120)
                except Exception:
                    for name in ("gui.log", "panel.log", "window-manager.log", "native-tray-result.json"):
                        path = evidence / name
                        if path.exists():
                            contents=path.read_text(errors="replace")
                            sys.stderr.write(contents[:1500]+("\n...\n" if len(contents)>3000 else "")+contents[-1500:])
                    raise
                restored()
            run("systemctl", "stop", "moth-desktop.service")
            restored()
            print("Linux real UID IPC, HTTPS/DNS, TCP/UDP IPv4/IPv6 TUN, engine death, SIGKILL/restart, reconnect and network restoration passed.")
        except Exception:
            # These commands describe only the synthetic acceptance network;
            # never dump engine configuration or imported profile data.
            for args in [("ip", "-s", "link", "show", "moth-ci-host"),
                         ("ip", "-s", "link", "show", "moth0"),
                         ("ip", "-4", "route", "get", "10.231.90.2", "from", "172.19.0.1"),
                         ("ip", "-4", "route", "get", "10.231.90.2", "mark", "0x4d4f5448"),
                         ("sysctl", "net.ipv4.conf.all.rp_filter", "net.ipv4.conf.moth-ci-host.rp_filter"),
                         ("ip", "netns", "exec", "moth-ci-sink", "ss", "-ln", "sport", "=", ":18080")]:
                diagnostic = run(*args, check=False)
                sys.stderr.write(diagnostic.stdout.decode(errors="replace")[:3000])
            for path in ("/usr/lib/moth-desktop", "/usr/lib/moth-desktop/moth-service", "/run", "/run/user", "/run/user/23101", "/run/moth-desktop", "/run/moth-desktop/control.sock"):
                try:
                    metadata = os.lstat(path)
                    print(f"Acceptance access metadata: {path} uid={metadata.st_uid} mode={oct(metadata.st_mode & 0o7777)}", file=sys.stderr)
                except FileNotFoundError:
                    print(f"Acceptance path absent: {path}", file=sys.stderr)
            for user in ("root", users[0]):
                health = run("runuser", "-u", user, "--", "/usr/lib/moth-desktop/moth-service", "--status", check=False)
                # Health/errors are typed and credential-free; never print inputs.
                print(f"Acceptance health ({user}): exit={health.returncode} " + health.stdout.decode(errors="replace")[:1000] + health.stderr.decode(errors="replace")[:500], file=sys.stderr)
            # The daemon logs only typed public failures, never profile/config input.
            diagnostic = run("journalctl", "-u", "moth-desktop.service", "--no-pager", "-n", "50", check=False)
            sys.stderr.write(diagnostic.stdout.decode(errors="replace"))
            raise
        finally:
            run("systemctl", "stop", "moth-desktop.service", check=False)
            if (root/"moth-service").exists():
                run(str(root/"moth-service"),"--release-guard",check=False)
            run("systemctl","stop","moth-egress-guard.service",check=False)
            if guardian.exists():
                guardian.unlink()
            for child in children:
                child.kill()
                child.wait(timeout=10)
            if unit.exists():
                unit.unlink()
                run("systemctl", "daemon-reload", check=False)
            # The directory was created by this test after an explicit absence check.
            if root.exists():
                shutil.rmtree(root)
            if veth:
                run("ip", "link", "delete", "moth-ci-host", check=False)
            if namespace:
                run("ip", "netns", "delete", "moth-ci-sink", check=False)
            for user, uid in created_users:
                run("loginctl", "disable-linger", user, check=False)
                run("systemctl", "stop", f"user@{uid}.service", check=False)
                run("systemctl", "stop", f"user-runtime-dir@{uid}.service", check=False)
                run("pkill", "-u", str(uid), check=False)
                run("userdel", "--remove", user, check=False)


if __name__ == "__main__":
    if len(sys.argv) == 2 and sys.argv[1] == "sink":
        sink()
    else:
        main()
