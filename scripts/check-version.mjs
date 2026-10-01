// Fails unless Cargo.toml, package.json, package-lock.json and
// tauri.conf.json carry the same version. With a tag argument (`v0.1.3` or
// `refs/tags/v0.1.3`), the tag must match too.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (p) => readFileSync(join(root, p), "utf8");

const cargo = read("Cargo.toml").match(/\[workspace\.package\][^[]*?^version = "([^"]+)"/m)?.[1];
const lock = JSON.parse(read("package-lock.json"));
const versions = {
  "Cargo.toml": cargo,
  "package.json": JSON.parse(read("package.json")).version,
  "package-lock.json": lock.version,
  "package-lock.json packages[\"\"]": lock.packages?.[""]?.version,
  "src-tauri/tauri.conf.json": JSON.parse(read("src-tauri/tauri.conf.json")).version,
};

const tag = process.argv[2]?.replace(/^refs\/tags\//, "");
if (tag) versions[`tag ${tag}`] = tag.replace(/^v/, "");

const distinct = new Set(Object.values(versions));
for (const [file, v] of Object.entries(versions)) console.log(`${file.padEnd(36)} ${v}`);
if (distinct.size !== 1 || distinct.has(undefined)) {
  console.error("version mismatch");
  process.exit(1);
}
