#!/usr/bin/env python3
"""Two actual signed native builds, isolated HTTPS and ordinary-user update CI.

Test source/key/endpoints stay in a separate temporary tree. Production artifacts
and production trust anchors are never overwritten, signed or uploaded here.
"""
import hashlib
import http.server
import io
import json
import os
import re
from pathlib import Path
import secrets
import shutil
import ssl
import subprocess
import sys
import tarfile
import tempfile
import threading
import uuid


def run(*args,cwd=None,env=None,quiet=False):
    subprocess.run(args,cwd=cwd,env=env,check=True,stdout=subprocess.DEVNULL if quiet else None,stderr=subprocess.DEVNULL if quiet else None)


def main():
    if os.environ.get('GITHUB_ACTIONS')!='true' or os.environ.get('RUNNER_OS') not in ('Windows','Linux'):
        raise RuntimeError('Disposable hosted CI is required for signed update acceptance')
    windows=os.name=='nt'
    repo=Path(__file__).resolve().parent.parent
    # Preserve private failures for this disposable job; no test binaries/keys
    # enter an upload-artifact step. The VM is destroyed after the job.
    directory=Path(tempfile.mkdtemp(prefix='moth-signed-update-',dir=os.environ['RUNNER_TEMP']))
    source=directory/'source'
    source.mkdir()
    archive=subprocess.run(['git','archive','HEAD'],cwd=repo,check=True,stdout=subprocess.PIPE).stdout
    with tarfile.open(fileobj=io.BytesIO(archive)) as files:
        files.extractall(source,filter='data')
    # Restore dependencies from the normal checked workspace cache, but never
    # save this job's test-key binaries back into a shared production cache.
    target=repo/'target';target.mkdir(exist_ok=True)
    if windows:
        run('pwsh','-NoProfile','-File',str(repo/'scripts/link-fixture-target.ps1'),'-Link',str(source/'target'),'-Target',str(target))
    else:
        (source/'target').symlink_to(target,target_is_directory=True)
    npm='npm.cmd' if windows else 'npm'
    run(npm,'ci',cwd=source)
    environment=os.environ.copy()
    environment['MOTH_FIXTURE_SOURCE']=str(source)
    environment['MOTH_FIXTURE_DIRECTORY']=str(directory)
    environment['MOTH_FIXTURE_PASSWORD']='Moth-test-'+secrets.token_urlsafe(48)
    run('node',str(source/'scripts/update-fixture-signer.cjs'),'generate',env=environment,quiet=True)
    (source/'packaging/updater.pub').write_bytes((directory/'fixture.key.pub').read_bytes())
    openssl=shutil.which('openssl')
    if not openssl and windows:
        openssl=str(Path(os.environ['ProgramFiles'])/'Git/usr/bin/openssl.exe')
    if not openssl:
        raise RuntimeError('OpenSSL fixture tool is required')
    run(openssl,'req','-x509','-newkey','rsa:2048','-nodes','-keyout',str(directory/'ca.key'),'-out',str(directory/'ca.crt'),'-days','1','-subj','/CN=Moth native update CI CA','-addext','basicConstraints=critical,CA:TRUE','-addext','keyUsage=critical,keyCertSign,cRLSign',quiet=True)
    run(openssl,'req','-newkey','rsa:2048','-nodes','-keyout',str(directory/'server.key'),'-out',str(directory/'server.csr'),'-subj','/CN=localhost',quiet=True)
    (directory/'server.ext').write_text('subjectAltName=DNS:localhost,IP:127.0.0.1\nbasicConstraints=CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n')
    run(openssl,'x509','-req','-in',str(directory/'server.csr'),'-CA',str(directory/'ca.crt'),'-CAkey',str(directory/'ca.key'),'-CAcreateserial','-out',str(directory/'server.crt'),'-days','1','-extfile',str(directory/'server.ext'),quiet=True)

    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self,*_): pass
        def do_GET(self):
            mode=(directory/'mode').read_text().strip() if (directory/'mode').exists() else 'success'
            variant='fault' if mode=='rollback' else 'candidate'
            artifact=directory/variant
            prefix='/releases/download/v0.3.3/'
            if self.path.startswith('/releases?') or self.path=='/releases':
                assets=[{'name':name,'browser_download_url':base+'/v0.3.3/'+name} for name in ('moth-update.json','moth-update.json.sig')]
                data=json.dumps([{'tag_name':'v0.3.3','draft':False,'assets':assets}]).encode()
            elif self.path.startswith(prefix):
                name=self.path[len(prefix):]
                if name not in ('moth-update.json','moth-update.json.sig',package_name):
                    self.send_error(404);return
                data=(artifact/name).read_bytes()
                if mode=='metadata-rejection' and name=='moth-update.json':
                    data=bytes([data[0]^1])+data[1:]
                if mode=='download-rejection' and name==package_name:
                    data=bytes([data[0]^1])+data[1:]
            else:
                self.send_error(404);return
            self.send_response(200);self.send_header('Content-Length',str(len(data)));self.end_headers()
            try: self.wfile.write(data)
            except (BrokenPipeError,ConnectionResetError): pass

    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
    context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(directory/'server.crt',directory/'server.key')
    server.socket=context.wrap_socket(server.socket,server_side=True)
    base=f'https://localhost:{server.server_port}/releases/download'
    environment['MOTH_FIXTURE_BASE']=base
    code=source/'crates/moth-update/src/lib.rs'
    text=code.read_text(encoding='utf-8').replace('https://api.github.com/repos/Bu11t0N/Moth-Desktop/releases?per_page=20',f'https://localhost:{server.server_port}/releases?per_page=20').replace('https://github.com/Bu11t0N/Moth-Desktop/releases/download',base)
    code.write_text(text,encoding='utf-8')
    package_name='Moth-0.3.3-'+('windows-x64-setup.exe' if windows else 'linux-x64.deb')
    threading.Thread(target=server.serve_forever,daemon=True).start()
    run('node','scripts/fetch-engines.mjs',cwd=source)
    configurations={name:(source/name).read_text(encoding='utf-8') for name in ('Cargo.toml','package.json','src-tauri/tauri.conf.json','scripts/Service.ps1')}
    original_lock=(source/'Cargo.lock').read_text()
    if not windows:
        linux=source/'src-tauri/tauri.linux.conf.json'
        linux.write_text(linux.read_text().replace('../target/release/','../target/debug/'))
    try:
        for version,variant in [('0.3.2','baseline'),('0.3.3','candidate')]:
            for name,original in configurations.items():
                (source/name).write_text(original.replace('0.3.3',version),encoding='utf-8')
            lock=re.sub(r'(\[\[package\]\]\nname = "moth-[^"]+"\nversion = ")0\.3\.3(")',lambda match:match[1]+version+match[2],original_lock)
            (source/'Cargo.lock').write_text(lock)
            run('cargo','build','-p','moth-service','-p','moth-maintenance','--locked',cwd=source)
            arguments=['desktop:build','--','--debug']+(['--no-bundle'] if windows else ['--bundles','deb'])
            run(npm,'run',*arguments,cwd=source)
            artifact=directory/variant;artifact.mkdir()
            if windows:
                run('pwsh','-NoProfile','-File',str(source/'scripts/package-windows.ps1'),'-BuildDirectory',str(source/'target/debug'),'-OutputDirectory',str(artifact/'portable'),cwd=source)
                package=artifact/'portable'/f'Moth-{version}-windows-x64'
                run('pwsh','-NoProfile','-File',str(source/'scripts/build-installer-windows.ps1'),'-PackageDirectory',str(package),'-OutputDirectory',str(artifact),cwd=source)
            else:
                packages=list((source/'target/debug/bundle/deb').glob(f'*_{version}_amd64.deb'))
                if len(packages)!=1: raise RuntimeError('Exactly one native Debian test package is required')
                shutil.copyfile(packages[0],artifact/f'Moth-{version}-linux-x64.deb')
                run(sys.executable,str(source/'scripts/check-linux-package.py'),str(artifact/f'Moth-{version}-linux-x64.deb'))
        fault=directory/'fault';fault.mkdir()
        if windows:
            script=source/'scripts/Service.ps1'
            marker="$fixtureMarker=Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'Moth Desktop/acceptance-poststart-fixture'; [IO.File]::Open($fixtureMarker,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None).Dispose(); if ($true)"
            script.write_text(script.read_text(encoding='utf-8').replace('if ($health.version -ne $expectedVersion)',marker),encoding='utf-8')
            run('pwsh','-NoProfile','-File',str(source/'scripts/build-installer-windows.ps1'),'-PackageDirectory',str(directory/'candidate/portable/Moth-0.3.3-windows-x64'),'-OutputDirectory',str(fault),cwd=source)
        else:
            package=directory/'candidate'/package_name
            unpacked=fault/'unpacked'
            run('dpkg-deb','--raw-extract',str(package),str(unpacked))
            script=unpacked/'DEBIAN/postinst'
            original=script.read_text()
            if not original.endswith('exit 0\n'): raise RuntimeError('Post-start fault injection point changed')
            script.write_text(original[:-len('exit 0\n')]+'(umask 077; set -C; : >/run/moth-update-acceptance-poststart)\necho "Isolated post-start failure fixture" >&2\nexit 1\n')
            run('dpkg-deb','--build','--root-owner-group',str(unpacked),str(fault/package_name))
        run('node',str(source/'scripts/update-fixture-signer.cjs'),'sign',env=environment,quiet=True)
        # The caller's source trust anchor/endpoints remain byte-identical.
        run('git','diff','--exit-code','--','packaging/updater.pub','crates/moth-update/src/lib.rs',cwd=repo)
        if windows:
            der=ssl.PEM_cert_to_DER_cert((directory/'ca.crt').read_text())
            thumbprint=hashlib.sha1(der).hexdigest()
            run('certutil','-addstore','-f','Root',str(directory/'ca.crt'),quiet=True)
            try: run('pwsh','-NoProfile','-File',str(repo/'scripts/test-signed-updates-windows.ps1'),'-FixtureDirectory',str(directory))
            finally: run('certutil','-delstore','Root',thumbprint,quiet=True)
        else:
            run('sudo','--non-interactive','--preserve-env=GITHUB_ACTIONS,RUNNER_OS',sys.executable,str(repo/'scripts/test-signed-updates-linux.py'),str(directory))
    finally:
        environment.pop('MOTH_FIXTURE_PASSWORD',None)
        server.shutdown();server.server_close()


if __name__=='__main__': main()
