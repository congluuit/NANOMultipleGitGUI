#!/usr/bin/env node
// Sets the app version everywhere it is declared: pnpm release:version 0.2.0
// (root and desktop package.json — Tauri reads the latter — and the Cargo workspace version).
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const version = process.argv[2]?.replace(/^v/, "");
if (!version || !/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error("usage: pnpm release:version <x.y.z[-beta.n]>");
  process.exit(1);
}

for (const file of ["package.json", "apps/desktop/package.json"]) {
  const path = join(root, file);
  const json = JSON.parse(readFileSync(path, "utf8"));
  json.version = version;
  writeFileSync(path, JSON.stringify(json, null, 2) + "\n");
}

const cargoPath = join(root, "Cargo.toml");
const cargo = readFileSync(cargoPath, "utf8");
const updated = cargo.replace(
  /(\[workspace\.package\][^[]*?\nversion\s*=\s*")[^"]*(")/,
  `$1${version}$2`,
);
if (updated === cargo && !cargo.includes(`version = "${version}"`)) {
  console.error("could not find [workspace.package] version in Cargo.toml");
  process.exit(1);
}
writeFileSync(cargoPath, updated);

// Refresh the workspace crates' versions in Cargo.lock without touching dependencies.
execFileSync("cargo", ["update", "--workspace", "--offline"], { cwd: root, stdio: "inherit" });

console.log(`version set to ${version}
next: update CHANGELOG.md, commit, then
  git tag v${version} && git push origin main v${version}`);
