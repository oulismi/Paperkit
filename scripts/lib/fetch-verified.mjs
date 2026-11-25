// Downloads files and checks each against its SHA-256; skips files already present.
import { createHash } from "node:crypto";
import { createWriteStream, existsSync, mkdirSync, renameSync, rmSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";

/** @param {{ url: string, path: string, size: number, sha256: string }[]} files  @param {string} target */
export async function fetchVerified(files, target) {
  for (const file of files) {
    const out = join(target, file.path);
    if (existsSync(out) && statSync(out).size === file.size) continue;
    mkdirSync(dirname(out), { recursive: true });
    console.log(`download ${file.path} (${(file.size / 1e6).toFixed(1)} MB)`);
    const res = await fetch(file.url);
    if (!res.ok) throw new Error(`${file.url}: HTTP ${res.status}`);
    const hash = createHash("sha256");
    const partial = `${out}.partial`;
    await pipeline(
      Readable.fromWeb(res.body),
      async function* (source) {
        for await (const chunk of source) {
          hash.update(chunk);
          yield chunk;
        }
      },
      createWriteStream(partial),
    );
    const digest = hash.digest("hex");
    if (digest !== file.sha256) {
      rmSync(partial);
      throw new Error(`${file.path}: checksum mismatch (got ${digest})`);
    }
    renameSync(partial, out);
  }
  console.log(`ready: ${target}`);
}
