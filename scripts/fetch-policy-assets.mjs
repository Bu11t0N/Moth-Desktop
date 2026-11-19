// Install exactly one official Mihomo executable and two pinned geo datasets.
import { readFile, writeFile, mkdir, rename } from "node:fs/promises";
import { createHash } from "node:crypto";
import { gunzipSync } from "node:zlib";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = fileURLToPath(new URL("../", import.meta.url));
const lock = JSON.parse(await readFile(path.join(root, "policy-assets.lock.json"), "utf8"));
const cache = path.join(root, ".local", "downloads");
const max = 80 * 1024 * 1024;
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
await mkdir(cache, { recursive: true });

async function download(url, expected) {
  const destination = path.join(cache, new URL(url).pathname.split("/").at(-1));
  let bytes = await readFile(destination).catch(() => null);
  if (bytes && sha(bytes) === expected) return { destination, bytes };
  const response = await fetch(url, { signal: AbortSignal.timeout(120000) });
  if (!response.ok || Number(response.headers.get("content-length")) > max) {
    throw new Error("Pinned policy asset unavailable");
  }
  const chunks = [];
  let size = 0;
  for await (const chunk of response.body) {
    size += chunk.length;
    if (size > max) throw new Error("Pinned policy asset exceeds size limit");
    chunks.push(chunk);
  }
  bytes = Buffer.concat(chunks);
  if (sha(bytes) !== expected) throw new Error("Pinned policy asset checksum mismatch");
  await writeFile(destination + ".new", bytes);
  await rename(destination + ".new", destination);
  return { destination, bytes };
}

const selected = process.argv.includes("--all") ? Object.keys(lock.platforms) : [`${process.platform}-${process.arch}`];
for (const platform of selected) {
  const asset = lock.platforms[platform];
  if (!asset) throw new Error("Unsupported policy engine platform");
  const archive = await download(asset.url, asset.archiveSha256);
  let binary;
  if (asset.member) {
    const windows = process.platform === "win32";
    const args = windows ? ["-xOf", archive.destination, asset.member] : ["-p", archive.destination, asset.member];
    const result = spawnSync(windows ? "tar" : "unzip", args, { maxBuffer: max, windowsHide: true });
    if (result.error || result.status !== 0) throw new Error("Cannot extract the pinned Mihomo member");
    binary = result.stdout;
  } else {
    binary = gunzipSync(archive.bytes, { maxOutputLength: max });
  }
  if (sha(binary) !== asset.sha256) throw new Error("Mihomo executable checksum mismatch");
  const directory = path.join(root, ".local", "engines", platform);
  await mkdir(directory, { recursive: true });
  await writeFile(path.join(directory, asset.file), binary, { mode: 0o700 });
  for (const geo of lock.geo) {
    const { bytes } = await download(geo.url, geo.sha256);
    await writeFile(path.join(directory, geo.file), bytes, { mode: 0o644 });
  }
  console.log(`Verified Mihomo ${lock.version} and geo data (${platform})`);
}
