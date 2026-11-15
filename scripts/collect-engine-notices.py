import concurrent.futures
import hashlib
import io
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile
import urllib.request

import argparse

parser=argparse.ArgumentParser(description='Collect notices for the exact pinned binaries without rebuilding engines')
parser.add_argument('--go',required=True,help='Go 1.27+ executable')
parser.add_argument('--sources',required=True,help='Directory with the two exact upstream revisions and four go-licenses save outputs')
args=parser.parse_args()
repo=Path(__file__).resolve().parent.parent
sources=Path(args.sources).resolve(strict=True)
go=Path(args.go).resolve(strict=True)
environment=os.environ.copy()
pins=json.loads((repo/'engines.lock.json').read_text())
modules={}
index={'collector':'github.com/google/go-licenses/v2 v2.0.1','binaries':{},'modules':[], 'manualDeclarations':{'github.com/jackpal/go-nat-pmp':'Apache-2.0 notice; full text supplied separately','github.com/anytls/sing-anytls':'GPL-3.0-or-later','github.com/sagernet/fswatch':'GPL-3.0-or-later','github.com/sagernet/sing':'GPL-3.0-or-later','github.com/sagernet/sing-vmess':'GPL-3.0-or-later','github.com/sagernet/sing-box':'GPL-3.0-or-later; retain additional upstream naming notice'}}
entries={}
def add(name,data):
    if name in entries and entries[name]!=data: raise RuntimeError('Conflicting notice '+name)
    if str(Path.home()).encode().lower() in data.lower(): raise RuntimeError('Local home path in collected text')
    entries[name]=data
for name,engine in pins['engines'].items():
    for platform,asset in engine['platforms'].items():
        binary=repo/'.local/engines'/platform/asset['file']
        if hashlib.sha256(binary.read_bytes()).hexdigest()!=asset['binarySha256']: raise RuntimeError('Binary pin mismatch')
        info=json.loads(subprocess.check_output([str(go),'version','-m','-json',str(binary)],env=environment))
        index['binaries'][name+'/'+platform]={'sha256':asset['binarySha256'],'goVersion':info['GoVersion'],'main':info['Main'],'dependencies':info['Deps'],'buildSettings':info['Settings']}
        for dep in info['Deps']:
            dep=dep.get('Replace',dep)
            key=(dep['Path'],dep['Version'])
            if key in modules and modules[key]['Sum']!=dep['Sum']: raise RuntimeError('Conflicting module sum')
            modules[key]=dep
        gv=info['GoVersion']
        add('go-runtime/'+gv+'/LICENSE',urllib.request.urlopen('https://raw.githubusercontent.com/golang/go/'+gv+'/LICENSE',timeout=30).read())
def collect(item):
    (path,version),dep=item
    data=json.loads(subprocess.check_output([str(go),'mod','download','-json',path+'@'+version],env=environment))
    if data.get('Sum')!=dep['Sum']: raise RuntimeError('Module sum differs from binary: '+path)
    root=Path(data['Dir'])
    files=[]
    for file in root.rglob('*'):
        if file.is_file() and re.match(r'(?i)^(?:licen[cs]e|copying|notice|copyright|authors)(?:[._-].*)?$',file.name):
            files.append((file.relative_to(root).as_posix(),file.read_bytes()))
    if not files: raise RuntimeError('No license notices for '+path)
    return path,version,dep['Sum'],files
with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
    for path,version,total,files in pool.map(collect,sorted(modules.items())):
        index['modules'].append({'path':path,'version':version,'sum':total,'noticeFiles':[name for name,_ in files]})
        for name,data in files: add('modules/'+path+'@'+version+'/'+name,data)
for directory in ['xray-windows-notices','xray-linux-notices','sing-windows-notices','sing-linux-notices']:
    root=sources/directory
    for file in root.rglob('*'):
        if file.is_file(): add('go-licenses/'+directory+'/'+file.relative_to(root).as_posix(),file.read_bytes())
for label,revision in [('Xray-core','1b1647cd9ddf0abc34b981d4611fc03bcac1ca74'),('sing-box','73bfb99ebce7923c485435e4faf8571b412065a9')]:
    root=sources/(label+'-'+revision)
    for name in ['LICENSE','go.mod','go.sum']:
        add('upstream/'+label+'/'+revision+'/'+name,(root/name).read_bytes())
add('full-license-texts/GPL-3.0.txt',(repo/'LICENSE').read_bytes())
add('full-license-texts/Apache-2.0.txt',(repo/'licenses/single-instance-Apache-2.0.txt').read_bytes())
add('SOURCE-INDEX.json',(json.dumps(index,indent=2)+'\n').encode())
output=repo/'licenses/Engine-dependencies.tar.gz'
with tarfile.open(output,'w:gz',format=tarfile.PAX_FORMAT) as archive:
    for name,data in sorted(entries.items()):
        member=tarfile.TarInfo(name);member.size=len(data);member.mode=0o644;member.mtime=0
        archive.addfile(member,io.BytesIO(data))
record={'collector':index['collector'],'engineLockSha256':hashlib.sha256((repo/'engines.lock.json').read_text().encode()).hexdigest(),'archive':'licenses/Engine-dependencies.tar.gz','archiveSha256':hashlib.sha256(output.read_bytes()).hexdigest(),'modules':len(modules),'files':len(entries),'manualDeclarations':index['manualDeclarations']}
(repo/'licenses/engine-inventory.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'modules':len(modules),'files':len(entries),'archiveBytes':output.stat().st_size}))
