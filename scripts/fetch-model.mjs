// Downloads the on-device NER model (GLiNER multilingual v2.1, ONNX, fp16)
// and checks every file against its SHA-256.
//
//   node scripts/fetch-model.mjs [target-dir]      (default: models/gliner_multi-v2.1)
//
// Used for development (tests) and to bundle the model into the offline
// installer. The pins below must match app/src-tauri/src/commands/model.rs.
import { createHash } from "node:crypto";
import { createWriteStream, existsSync, mkdirSync, renameSync, rmSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";

const REPO = "onnx-community/gliner_multi-v2.1";
const REVISION = "6ddaeb9413b0e71ad8457da1aab378a165b24058";
const FILES = [
  { path: "tokenizer.json", size: 16_331_948, sha256: "914bd3c8fb7b525af9e23b60d0ec7b1248ddb2b99014efd9c02ebeb022f8cab7" },
  { path: "onnx/model_fp16.onnx", size: 579_717_643, sha256: "9cec3c771bfa7e886e029cd013326b13ad7d18f58fac33824f43c106fbdfe920" },
];

const target = resolve(process.argv[2] ?? join(import.meta.dirname, "..", "models", "gliner_multi-v2.1"));

for (const file of FILES) {
  const out = join(target, file.path);
  if (existsSync(out) && statSync(out).size === file.size) {
    console.log(`ok       ${file.path} (already there)`);
    continue;
  }
  mkdirSync(dirname(out), { recursive: true });
  const url = `https://huggingface.co/${REPO}/resolve/${REVISION}/${file.path}`;
  console.log(`download ${file.path} (${Math.round(file.size / 1e6)} MB)`);
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
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
  console.log(`ok       ${file.path}`);
}
console.log(`Model ready in ${target}`);
