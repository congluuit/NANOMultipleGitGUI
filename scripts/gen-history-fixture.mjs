#!/usr/bin/env node
// Generates a repository with a large, branchy history for graph/performance testing.
//
//   node scripts/gen-history-fixture.mjs [targetDir] [commitCount] [seed]
//   (defaults: target/fixtures/graph-50k, 50000, 1)
//
// Uses `git fast-import`, so 50k commits take a few seconds. The history has a mainline plus
// up to 8 concurrent feature branches that fork from main and are merged back (--no-ff
// style), some long-lived release branches, tags, and Unicode/Vietnamese authors and messages.

import { spawn, execFileSync } from "node:child_process";
import { existsSync, mkdirSync, rmSync } from "node:fs";
import { resolve } from "node:path";

const target = resolve(process.argv[2] ?? "target/fixtures/graph-50k");
const total = Number(process.argv[3] ?? 50_000);
let seed = Number(process.argv[4] ?? 1) >>> 0 || 1;

function rand() {
  seed ^= seed << 13;
  seed >>>= 0;
  seed ^= seed >>> 17;
  seed ^= seed << 5;
  seed >>>= 0;
  return seed / 0x1_0000_0000;
}
const pick = (arr) => arr[Math.floor(rand() * arr.length)];

const authors = [
  ["Nguyễn Văn Hoàng", "hoang@acme.dev"],
  ["Trần Thị Lan", "lan@acme.dev"],
  ["Lê Minh", "minh@personal.dev"],
  ["Alice Martin", "alice@example.com"],
  ["Bob Chen", "bob@example.com"],
  ["dependabot[bot]", "bot@example.com"],
];
const verbs = ["feat", "fix", "refactor", "chore", "docs", "perf", "test"];
const areas = ["auth", "graph", "lfs", "accounts", "clone", "ui", "diff", "push", "settings"];
const phrases = [
  "handle edge case",
  "sửa lỗi hiển thị tiếng Việt",
  "improve performance",
  "add missing tests",
  "update dependencies",
  "tách module nhỏ hơn",
  "support self-hosted instances",
  "clean up",
];

if (existsSync(target)) rmSync(target, { recursive: true, force: true });
mkdirSync(target, { recursive: true });
execFileSync("git", ["init", "-q", "-b", "main", target]);

const fi = spawn("git", ["fast-import", "--quiet", "--done"], {
  cwd: target,
  stdio: ["pipe", "inherit", "inherit"],
});

const chunks = [];
let pending = 0;
function out(s) {
  chunks.push(s);
  pending += s.length;
  if (pending > 1 << 20) flush();
}
function flush() {
  if (chunks.length) fi.stdin.write(chunks.join(""));
  chunks.length = 0;
  pending = 0;
}
function data(s) {
  out(`data ${Buffer.byteLength(s)}\n${s}\n`);
}

let mark = 0;
let time = 1_600_000_000;
const heads = new Map(); // branch -> mark
const features = [];
let featureSeq = 0;

function commit(branch, parents, message) {
  mark++;
  time += 60 + Math.floor(rand() * 3600);
  const [name, email] = pick(authors);
  out(`commit refs/heads/${branch}\nmark :${mark}\n`);
  out(`author ${name} <${email}> ${time} +0700\ncommitter ${name} <${email}> ${time} +0700\n`);
  data(message);
  if (parents[0]) out(`from :${parents[0]}\n`);
  for (const p of parents.slice(1)) out(`merge :${p}\n`);
  out(`M 100644 inline src/${pick(areas)}/file${mark % 97}.txt\n`);
  data(`change ${mark}\n`);
  heads.set(branch, mark);
  return mark;
}

const msg = () => `${pick(verbs)}(${pick(areas)}): ${pick(phrases)}`;

commit("main", [], "chore: initial commit");
for (const rel of ["release/1.x", "release/2.x"]) heads.set(rel, heads.get("main"));

while (mark < total) {
  const r = rand();
  if (r < 0.06 && features.length < 8) {
    const name = `feature/${++featureSeq}-${pick(areas)}`;
    features.push(name);
    commit(name, [heads.get("main")], msg());
  } else if (r < 0.12 && features.length > 0) {
    const i = Math.floor(rand() * features.length);
    const [name] = features.splice(i, 1);
    commit("main", [heads.get("main"), heads.get(name)], `Merge branch '${name}'`);
    heads.delete(name);
  } else if (r < 0.14) {
    const rel = pick(["release/1.x", "release/2.x"]);
    commit(rel, [heads.get(rel)], `fix(release): backport ${pick(areas)} fix`);
  } else if (r < 0.6 && features.length > 0) {
    const name = pick(features);
    commit(name, [heads.get(name)], msg());
  } else {
    commit("main", [heads.get("main")], msg());
  }
  if (mark % 2500 === 0) out(`reset refs/tags/v0.${mark / 2500}.0\nfrom :${mark}\n\n`);
}

flush();
fi.stdin.end("done\n");
fi.on("close", (code) => {
  if (code !== 0) process.exit(code ?? 1);
  // Merged feature branches were deleted from `heads`; drop their refs like `git branch -d` would.
  const stale = execFileSync(
    "git",
    ["for-each-ref", "--format=%(refname)", "refs/heads/feature/"],
    {
      cwd: target,
    },
  )
    .toString()
    .split("\n")
    .filter((ref) => ref && !heads.has(ref.slice("refs/heads/".length)));
  execFileSync("git", ["update-ref", "--stdin"], {
    cwd: target,
    input: stale.map((ref) => `delete ${ref}\n`).join(""),
  });
  execFileSync("git", ["checkout", "-q", "-f", "main"], { cwd: target });
  const count = execFileSync("git", ["rev-list", "--all", "--count"], { cwd: target })
    .toString()
    .trim();
  console.log(`Generated ${count} commits (${featureSeq} feature branches) in ${target}`);
});
