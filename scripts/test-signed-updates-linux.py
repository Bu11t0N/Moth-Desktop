#!/usr/bin/env python3
"""Privileged half of native signed-update acceptance, disposable Ubuntu only."""
import json
import os
from pathlib import Path
import pwd
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import uuid


def run(*args,check=True):
    with tempfile.TemporaryFile() as output:
        result=subprocess.run(args,stdout=output,stderr=subprocess.STDOUT,timeout=180)
        output.seek(0);result.stdout=output.read(1024*1024)
    if check and result.returncode:
        raise RuntimeError('Signed update fixture command failed: '+args[0]+'\n'+result.stdout.decode(errors='replace')[-2500:])
    return result


def main():
    if os.environ.get('GITHUB_ACTIONS')!='true' or os.environ.get('RUNNER_OS')!='Linux' or os.geteuid()!=0:
        raise RuntimeError('Root in disposable hosted Linux CI is required')
    fixture=Path(sys.argv[1]).resolve(strict=True)
    user='moth-ci-update';uid=23150
    home=Path('/home')/user
    if home.exists() or Path('/usr/lib/moth-desktop').exists() or run('id',user,check=False).returncode==0 or run('getent','passwd',str(uid),check=False).returncode==0:
        raise RuntimeError('A fresh acceptance machine/account is required')
    certificate=Path('/usr/local/share/ca-certificates')/('moth-update-ci-'+str(uuid.uuid4())+'.crt')
    created=False;installed=False;session=None;helper=None;server=None
    try:
        with certificate.open('xb') as target: target.write((fixture/'ca.crt').read_bytes())
        certificate.chmod(0o644)
        run('update-ca-certificates')
        run('apt-get','install','--yes','--no-install-recommends',str(fixture/'baseline/Moth-0.3.2-linux-x64.deb'))
        installed=True
        with socket.socket() as listener:
            listener.bind(('127.0.0.1',0));port=listener.getsockname()[1]
        identifier=str(uuid.uuid4())
        link=f'vless://{identifier}@127.0.0.1:{port}?encryption=none&security=none&type=tcp#Updater-retention'
        configuration={'log':{'loglevel':'none'},'inbounds':[{'listen':'127.0.0.1','port':port,'protocol':'vless','settings':{'decryption':'none','clients':[{'id':identifier}]}}],'outbounds':[{'protocol':'freedom','settings':{},'streamSettings':{'sockopt':{'mark':0x4d4f5448}}}]}
        (fixture/'server.json').write_text(json.dumps(configuration))
        shutil.copyfile('/usr/lib/moth-desktop/engines/xray',fixture/'xray-fixture')
        (fixture/'xray-fixture').chmod(0o755)
        server=subprocess.Popen([str(fixture/'xray-fixture'),'run','-config',str(fixture/'server.json')],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        run('useradd','--uid',str(uid),'--user-group','--no-create-home','--home-dir',str(home),'--shell','/usr/sbin/nologin',user)
        created=True
        account=pwd.getpwnam(user)
        if account.pw_uid!=uid: raise RuntimeError('Acceptance UID mismatch')
        home.mkdir(mode=0o700);os.chown(home,uid,account.pw_gid)
        for name in ('data','evidence'):
            path=home/name;path.mkdir(mode=0o700);os.chown(path,uid,account.pw_gid)
        profile=home/'profile'
        profile.write_text(link)
        profile.chmod(0o600);os.chown(profile,uid,account.pw_gid)
        repo=Path(__file__).resolve().parent.parent
        for name in ('test-signed-updates-linux-user.sh','test-signed-updates-user.py'):
            target=home/name;shutil.copyfile(repo/'scripts'/name,target);target.chmod(0o755)
        run('loginctl','enable-linger',user)
        run('systemctl','start',f'user@{uid}.service')
        log=home/'session.log'
        with log.open('wb') as output:
            session=subprocess.Popen(['runuser','-u',user,'--','env','GITHUB_ACTIONS=true','RUNNER_OS=Linux',f'XDG_RUNTIME_DIR=/run/user/{uid}','dbus-run-session','--','xvfb-run','-a','bash',str(home/'test-signed-updates-linux-user.sh'),str(home)],stdout=output,stderr=output)
            for case in ('metadata-rejection','download-rejection','rollback','success'):
                evidence=home/'evidence'/case
                deadline=time.monotonic()+720
                allowed=False;authorized=False
                while not (evidence/'result.json').exists():
                    if (evidence/'error.txt').exists():
                        raise RuntimeError((evidence/'error.txt').read_text()[:1000])
                    current=(home/'current-case').read_text().strip() if (home/'current-case').exists() else ''
                    if current==case and not allowed:
                        (fixture/'mode').write_text(case)
                        (home/('allowed-'+case)).touch(mode=0o644)
                        allowed=True
                    request=evidence/'privileged-request'
                    if request.exists() and not authorized:
                        identifier=request.read_text().strip()
                        if str(uuid.UUID(identifier))!=identifier: raise RuntimeError('Invalid opaque acceptance request')
                        directory=home/'data/updates'/identifier
                        state=json.loads(run('/usr/lib/moth-desktop/moth-service','--tunnel-status').stdout)
                        if state.get('phase')!='disconnected':
                            raise RuntimeError('GUI did not confirm VPN stop before privileged update')
                        helper=subprocess.Popen(['/usr/lib/moth-desktop/moth-maintenance','--apply',str(directory)],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
                        authorized=True
                    if session.poll() is not None or time.monotonic()>deadline:
                        raise RuntimeError('Native Linux update session exited or timed out')
                    time.sleep(0.1)
                result=json.loads((evidence/'result.json').read_bytes())
                if not result.get('ordinaryUser') or not result.get('profilesUnchanged'):
                    raise RuntimeError('Ordinary-user Secret Service retention failed')
                if case in ('metadata-rejection','download-rejection'):
                    if authorized or not result.get('rejected'): raise RuntimeError('Changed content reached installation')
                else:
                    if not authorized or not result.get('restarted') or not result.get('componentsHealthy'):
                        raise RuntimeError('Actual native coordinated update/restart failed')
                    if (helper.wait(timeout=15)==0)!=(case=='success'):
                        raise RuntimeError('Privileged updater outcome mismatched native evidence')
                    helper=None
                    monitor=json.loads((evidence/'monitor-result.json').read_bytes())
                    if not monitor.get('ordinaryUser') or monitor.get('phase')!=('complete' if case=='success' else 'failed'):
                        raise RuntimeError('Native Linux monitor did not confirm transaction')
                expected='0.3.3' if case=='success' else '0.3.2'
                health=json.loads(run('/usr/lib/moth-desktop/moth-service','--status').stdout)
                if health['version']!=expected or not all(engine['available'] for engine in health['engines']):
                    raise RuntimeError('Installed coordinated component versions are not healthy')
                print('Actual ordinary-user signed Linux update acceptance passed: '+case,flush=True)
            if session.wait(timeout=30)!=0: raise RuntimeError('Native Linux update user session failed')
    except Exception:
        if home.exists() and (home/'session.log').exists():
            sys.stderr.write((home/'session.log').read_text(errors='replace')[-2000:])
        raise
    finally:
        if helper and helper.poll() is None:
            # Do not race a still-running package manager with teardown.
            try: helper.wait(timeout=650)
            except subprocess.TimeoutExpired: helper.kill();helper.wait(timeout=15)
        if session and session.poll() is None: session.kill();session.wait(timeout=15)
        if server and server.poll() is None: server.kill();server.wait(timeout=15)
        if installed: run('apt-get','remove','--yes','moth-desktop',check=False)
        if created:
            run('loginctl','disable-linger',user,check=False)
            run('systemctl','stop',f'user@{uid}.service',check=False)
            run('systemctl','stop',f'user-runtime-dir@{uid}.service',check=False)
            run('pkill','-u',str(uid),check=False)
            run('userdel','--remove',user,check=False)
        if certificate.exists(): certificate.unlink();run('update-ca-certificates',check=False)


if __name__=='__main__': main()
