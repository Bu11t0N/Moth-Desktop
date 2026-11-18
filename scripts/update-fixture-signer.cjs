// Ephemeral CI test key only. The caller discards signer output and never uploads
// this key, its password, test installers or compiled test trust anchors.
const fs=require('node:fs');
const path=require('node:path');
const crypto=require('node:crypto');
process.umask(0o077);
const cli=require(path.join(process.env.MOTH_FIXTURE_SOURCE,'node_modules/@tauri-apps/cli/main.js'));
(async()=>{
  const directory=process.env.MOTH_FIXTURE_DIRECTORY;
  const key=path.join(directory,'fixture.key');
  const password=process.env.MOTH_FIXTURE_PASSWORD;
  if(process.argv[2]==='generate'){
    await cli.run(['signer','generate','--ci','--password',password,'--write-keys',key]);
    return;
  }
  const base=process.env.MOTH_FIXTURE_BASE;
  const version=JSON.parse(fs.readFileSync(path.join(process.env.MOTH_FIXTURE_SOURCE,'package.json'),'utf8')).version;
  if(!/^\d+\.\d+\.\d+$/.test(version))throw Error('Invalid fixture version');
  const suffix=process.platform==='win32'?'windows-x64-setup.exe':'linux-x64.deb';
  for(const variant of ['candidate','fault']){
    const artifact=path.join(directory,variant);
    process.chdir(artifact);
    const file=`Moth-${version}-${suffix}`;
    await cli.run(['signer','sign','--private-key-path',key,'--password',password,'--app-version',version,file]);
    const bytes=fs.readFileSync(file);
    const metadata={schema:1,application:'org.mothvpn.desktop',version,protocol:2,notes:'Isolated platform-specific native acceptance.',platforms:{}};
    for(const [platform,extension] of [['windows-x86_64','windows-x64-setup.exe'],['linux-x86_64','linux-x64.deb']]){
      const name=`Moth-${version}-${extension}`;
      // The unused platform is structural fixture metadata and is never served
      // or accepted as native evidence. Each OS job installs its real package.
      metadata.platforms[platform]={file:name,url:`${base}/v${version}/${name}`,size:bytes.length,sha256:crypto.createHash('sha256').update(bytes).digest('hex'),signature:fs.readFileSync(file+'.sig','utf8').trim()};
    }
    fs.writeFileSync('moth-update.json',JSON.stringify(metadata),{flag:'wx'});
    await cli.run(['signer','sign','--private-key-path',key,'--password',password,'--app-version',version,'moth-update.json']);
  }
})().catch(()=>{process.exitCode=1});
