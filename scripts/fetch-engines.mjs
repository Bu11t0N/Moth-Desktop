// Official archives, exact archive AND executable pins. Never resolve "latest".
import { readFile, writeFile, mkdir, chmod, rename } from "node:fs/promises";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const lock = JSON.parse(
  await readFile(path.join(root, "engines.lock.json"), "utf8"),
);
const args = process.argv.slice(2);
const inspect = args.includes("--inspect");
const all = args.includes("--all");
const platform = `${process.platform}-${process.arch}`;
const hash = (buffer) => createHash("sha256").update(buffer).digest("hex");
if (lock.schemaVersion !== 1) throw new Error("Unsupported engine lock");
const cache = path.join(root, ".local", "downloads");
await mkdir(cache, { recursive: true });
for (const [name, engine] of Object.entries(lock.engines)) {
  for (const [target, asset] of Object.entries(engine.platforms)) {
    if (!all && target !== platform) continue;
    const archive = path.join(
      cache,
      `${name}-${engine.version}-${target}${asset.url.endsWith(".zip") ? ".zip" : ".tar.gz"}`,
    );
    let bytes;
    try {
      bytes = await readFile(archive);
    } catch {}
    if (!bytes || hash(bytes) !== asset.archiveSha256) {
      console.log(`Downloading ${name} ${engine.version} (${target})`);
      const response = await fetch(asset.url, {
        signal: AbortSignal.timeout(120000),
      });
      if (!response.ok)
        throw new Error(`Download failed: HTTP ${response.status}`);
      if (
        Number(response.headers.get("content-length") || 0) >
        128 * 1024 * 1024
      )
        throw new Error("Archive too large");
      bytes = Buffer.from(await response.arrayBuffer());
      if (
        bytes.length > 128 * 1024 * 1024 ||
        hash(bytes) !== asset.archiveSha256
      )
        throw new Error("Archive SHA-256 mismatch");
      await writeFile(archive, bytes);
    }
    // Extract ONLY the pinned member to memory, no arbitrary archive paths on disk.
    const unzip = process.platform !== "win32" && asset.url.endsWith(".zip");
    const result = spawnSync(
      unzip ? "unzip" : "tar",
      unzip ? ["-p", archive, asset.member] : ["-xOf", archive, asset.member],
      { maxBuffer: 128 * 1024 * 1024, windowsHide: true },
    );
    if (result.error || result.status !== 0)
      throw new Error(`Cannot extract ${name}; install tar/bsdtar`);
    const binary = result.stdout;
    const sha = hash(binary);
    if (inspect) {
      console.log(JSON.stringify({ name, target, binarySha256: sha }));
      continue;
    }
    if (!asset.binarySha256 || sha !== asset.binarySha256)
      throw new Error(`Executable SHA-256 mismatch: ${name}`);
    const dir = path.join(root, ".local", "engines", target);
    await mkdir(dir, { recursive: true });
    const destination = path.join(dir, asset.file);
    await writeFile(destination + ".new", binary, { mode: 0o700 });
    if (process.platform !== "win32") await chmod(destination + ".new", 0o700);
    await rename(destination + ".new", destination);
    console.log(`Verified ${name} ${engine.version} (${target})`);
  }
}
