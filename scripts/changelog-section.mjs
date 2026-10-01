// Prints the CHANGELOG.md section for a version (`v0.1.3` or `0.1.3`).
// Used as the GitHub release notes.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const version = (process.argv[2] ?? "").replace(/^refs\/tags\//, "").replace(/^v/, "");
if (!version) {
  console.error("usage: changelog-section.mjs <version>");
  process.exit(1);
}

const lines = readFileSync(join(root, "CHANGELOG.md"), "utf8").split(/\r?\n/);
const start = lines.findIndex((l) => l.startsWith(`## [${version}]`));
if (start < 0) {
  console.error(`no CHANGELOG.md section for ${version}`);
  process.exit(1);
}
const end = lines.findIndex((l, i) => i > start && /^## /.test(l));
const body = lines.slice(start + 1, end < 0 ? undefined : end);
while (body.length && !body.at(-1).trim()) body.pop();
while (body.length && !body[0].trim()) body.shift();
console.log(body.join("\n"));
