#!/usr/bin/env python3
"""Ordinary-user half, one real desktop/keyring session across all restarts."""
import os
from pathlib import Path
import subprocess
import sys
import time

if os.geteuid()==0 or os.environ.get('GITHUB_ACTIONS')!='true':
    raise RuntimeError('Ordinary user in disposable hosted Linux CI is required')
home=Path(sys.argv[1])
for case in ('metadata-rejection','download-rejection','rollback','success'):
    evidence=home/'evidence'/case;evidence.mkdir(mode=0o700)
    (home/'current-case').write_text(case)
    deadline=time.monotonic()+720
    while not (home/('allowed-'+case)).exists():
        if time.monotonic()>deadline: raise RuntimeError('Root fixture did not authorize case')
        time.sleep(0.1)
    environment=os.environ.copy()
    environment.update(MOTH_NATIVE_UPDATE_TEST='1',MOTH_NATIVE_UPDATE_EXPECT=case,MOTH_NATIVE_UPDATE_EVIDENCE=str(evidence),MOTH_DEV_DATA_DIR=str(home/'data'))
    with (home/'profile').open('rb') as profile,(evidence/'gui.log').open('wb') as log:
        process=subprocess.Popen(['/usr/bin/moth-desktop'],stdin=profile,stdout=log,stderr=log,env=environment)
        while not (evidence/'result.json').exists():
            if (evidence/'error.txt').exists() or time.monotonic()>deadline:
                raise RuntimeError('Native update failed or timed out')
            time.sleep(0.1)
        if process.wait(timeout=20)!=0: raise RuntimeError('Original native GUI did not exit cleanly')
    deadline=time.monotonic()+20
    while subprocess.run(['pgrep','-u',str(os.getuid()),'-x','moth-desktop'],stdout=subprocess.DEVNULL).returncode==0:
        if time.monotonic()>deadline: raise RuntimeError('Restarted native GUI did not exit')
        time.sleep(0.1)
