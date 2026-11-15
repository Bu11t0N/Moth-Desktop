// Run only in the protected release job. No key/password is placed in process
// arguments or artifact directories. The parent discards signer output.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const cli = require('@tauri-apps/cli');
(async () => {
  if (process.env.GITHUB_ACTIONS !== 'true' || process.env.GITHUB_REF !== 'refs/heads/main') throw Error();
  if (!process.env.TAURI_SIGNING_PRIVATE_KEY || !process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD) throw Error();
  const version = require('../package.json').version;
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw Error();
  const releaseNotes = fs.readFileSync(path.join(__dirname, '../docs/RELEASE.ru.txt'), 'utf8');
  const notes = fs.readFileSync(path.join(__dirname, '../docs/UPDATE-NOTES.ru.txt'), 'utf8');
  if (!notes.trim() || Buffer.byteLength(notes, 'utf8') > 4096) throw Error();
  const directory = path.resolve(process.argv[2]);
  const names = [`Moth-${version}-windows-x64-setup.exe`, `Moth-${version}-linux-x64.deb`, `Moth-${version}-windows-x64.zip`];
  process.chdir(directory);
  const metadata = {schema: 1, application: 'org.mothvpn.desktop', version, protocol: 2, notes, platforms: {}};
  for (const file of names) {
    const data = fs.readFileSync(file);
    if (data.length < 1024 || data.length > 256 * 1024 * 1024) throw Error();
    if (file.endsWith('.exe') && data.subarray(0, 2).toString() !== 'MZ') throw Error();
    if (file.endsWith('.deb') && data.subarray(0, 8).toString() !== '!<arch>\n') throw Error();
    await cli.run(['signer', 'sign', '--app-version', version, file]);
    if (!file.endsWith('.zip')) {
      const platform = file.endsWith('.exe') ? 'windows-x86_64' : 'linux-x86_64';
      metadata.platforms[platform] = {file, url: `https://github.com/Bu11t0N/Moth-Desktop/releases/download/v${version}/${file}`, size: data.length, sha256: crypto.createHash('sha256').update(data).digest('hex'), signature: fs.readFileSync(file + '.sig', 'utf8').trim()};
    }
  }
  fs.writeFileSync('moth-update.json', JSON.stringify(metadata, null, 2) + '\n', {flag: 'wx'});
  await cli.run(['signer', 'sign', '--app-version', version, 'moth-update.json']);
  fs.writeFileSync('RELEASE.txt', releaseNotes, {flag: 'wx'});
  const files = [...names, ...names.map(name => name + '.sig'), 'moth-update.json', 'moth-update.json.sig', 'RELEASE.txt'].sort();
  fs.writeFileSync('SHA256SUMS', files.map(file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex') + '  ' + file).join('\n') + '\n', {flag: 'wx'});
})().catch(() => { process.exitCode = 1; });
