// Builds clawed-hook in release mode and places it where Tauri's
// `externalBin` expects it: src-tauri/binaries/clawed-hook-<target-triple>[.exe].
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const triple =
  process.env.TAURI_ENV_TARGET_TRIPLE ||
  execFileSync("rustc", ["-vV"], { encoding: "utf8" }).match(/^host: (.+)$/m)[1].trim();
const exe = triple.includes("windows") ? ".exe" : "";

const args = ["build", "-p", "clawed-hook", "--release"];
if (process.env.TAURI_ENV_TARGET_TRIPLE) args.push("--target", triple);
execFileSync("cargo", args, { cwd: root, stdio: "inherit" });

const built = process.env.TAURI_ENV_TARGET_TRIPLE
  ? join(root, "target", triple, "release", `clawed-hook${exe}`)
  : join(root, "target", "release", `clawed-hook${exe}`);
const dest = join(root, "src-tauri", "binaries", `clawed-hook-${triple}${exe}`);
mkdirSync(dirname(dest), { recursive: true });
copyFileSync(built, dest);
console.log(`clawed-hook -> ${dest}`);
