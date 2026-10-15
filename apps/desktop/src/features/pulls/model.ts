/** Pure helpers of the pull / merge request feature (Phase 4.1). No React, no IPC. */
import type { Account, Provider, ProviderFamily, PullComment, PullSide } from "@/ipc";

/** "PR" on GitHub, "MR" on GitLab (unknown family → "PR"). */
export const shortTerm = (family: ProviderFamily | null | undefined) =>
  family === "gitlab" ? "MR" : "PR";

/** `#12` on GitHub, `!12` on GitLab. */
export const pullNumber = (family: ProviderFamily | null | undefined, n: number) =>
  `${family === "gitlab" ? "!" : "#"}${n}`;

/** Provider family of an account (`null` while accounts / providers load). */
export function familyOfAccount(
  account: Pick<Account, "providerId"> | null | undefined,
  providers: readonly Provider[] | undefined,
): ProviderFamily | null {
  if (!account) return null;
  const kind = providers?.find((p) => p.id === account.providerId)?.kind;
  if (!kind) return null;
  return kind === "gitlab" || kind === "gitlabSelfHosted" ? "gitlab" : "github";
}

// ── Unified diff ────────────────────────────────────────────────────────────────────────────

export type PatchRow =
  | { kind: "hunk"; text: string }
  | { kind: "add" | "del" | "ctx"; text: string; oldLine: number | null; newLine: number | null }
  | { kind: "meta"; text: string };

const HUNK = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/;

/** Rows of a unified diff (hunks from the first `@@`), with old/new line numbers. */
export function parsePatch(patch: string): PatchRow[] {
  const rows: PatchRow[] = [];
  let oldLine = 0;
  let newLine = 0;
  let inHunk = false;
  const lines = patch.split("\n");
  // A trailing newline yields one empty last element that is not a context line.
  if (lines.length && lines[lines.length - 1] === "") lines.pop();
  for (const raw of lines) {
    const line = raw.endsWith("\r") ? raw.slice(0, -1) : raw;
    const m = HUNK.exec(line);
    if (m) {
      oldLine = Number(m[1]);
      newLine = Number(m[2]);
      inHunk = true;
      rows.push({ kind: "hunk", text: line });
      continue;
    }
    if (!inHunk) continue;
    const sign = line[0];
    const text = line.slice(1);
    if (sign === "+") {
      rows.push({ kind: "add", text, oldLine: null, newLine: newLine++ });
    } else if (sign === "-") {
      rows.push({ kind: "del", text, oldLine: oldLine++, newLine: null });
    } else if (sign === "\\") {
      rows.push({ kind: "meta", text: line });
    } else {
      rows.push({ kind: "ctx", text, oldLine: oldLine++, newLine: newLine++ });
    }
  }
  return rows;
}

/** Where a new comment on `row` goes: the new side unless the line was deleted. */
export function rowAnchor(
  row: Extract<PatchRow, { kind: "add" | "del" | "ctx" }>,
): { line: number; side: PullSide; pairedLine: number | null } | null {
  if (row.kind === "del" && row.oldLine !== null) {
    return { line: row.oldLine, side: "old", pairedLine: null };
  }
  if (row.newLine === null) return null;
  return {
    line: row.newLine,
    side: "new",
    pairedLine: row.kind === "ctx" ? row.oldLine : null,
  };
}

// ── Comment threads ─────────────────────────────────────────────────────────────────────────

export interface Thread {
  id: string;
  path: string;
  /** `null` = outdated (the line is not in the current diff). */
  line: number | null;
  side: PullSide;
  comments: PullComment[];
  resolved: boolean | null;
}

/** Line comments grouped by thread (first comment's anchor), oldest thread first. */
export function lineThreads(comments: readonly PullComment[]): Thread[] {
  const byId = new Map<string, Thread>();
  for (const c of comments) {
    // GitLab replies in a line discussion carry no position: they join by thread id.
    const joinsThread = !!c.threadId && byId.has(c.threadId);
    if (c.kind !== "line" && !joinsThread) continue;
    const id = c.threadId ?? c.id;
    const existing = byId.get(id);
    if (existing) {
      existing.comments.push(c);
      if (c.resolved !== null) existing.resolved = c.resolved;
      continue;
    }
    if (!c.anchor) continue;
    byId.set(id, {
      id,
      path: c.anchor.path,
      line: c.anchor.line,
      side: c.anchor.side,
      comments: [c],
      resolved: c.resolved,
    });
  }
  return [...byId.values()];
}

/** Key of a diff line for thread lookup. */
export const lineKey = (side: PullSide, line: number) => `${side}:${line}`;

/** Threads of one file by line key, plus the outdated ones. */
export function threadsOfFile(
  threads: readonly Thread[],
  path: string,
): { byLine: Map<string, Thread[]>; outdated: Thread[] } {
  const byLine = new Map<string, Thread[]>();
  const outdated: Thread[] = [];
  for (const th of threads) {
    if (th.path !== path) continue;
    if (th.line === null) {
      outdated.push(th);
      continue;
    }
    const key = lineKey(th.side, th.line);
    const list = byLine.get(key);
    if (list) list.push(th);
    else byLine.set(key, [th]);
  }
  return { byLine, outdated };
}

/** Line threads per file path (for the file list badges). */
export function threadCountByPath(threads: readonly Thread[]): Map<string, number> {
  const out = new Map<string, number>();
  for (const th of threads) out.set(th.path, (out.get(th.path) ?? 0) + 1);
  return out;
}

/** Comments of the conversation tab: everything except replies inside line threads. */
export function timeline(comments: readonly PullComment[]): PullComment[] {
  const roots = new Map(lineThreads(comments).map((th) => [th.id, th.comments[0]?.id]));
  return comments.filter((c) => {
    const thread = c.threadId ?? (c.kind === "line" ? c.id : null);
    if (thread === null || !roots.has(thread)) return true;
    return roots.get(thread) === c.id;
  });
}

// ── Display ─────────────────────────────────────────────────────────────────────────────────

/** `owner:branch` for a branch of another project, else the branch. */
export function branchLabel(branch: string, repo: string | null, base: string): string {
  if (!repo || repo.toLowerCase() === base.toLowerCase()) return branch;
  return `${repo.split("/")[0]}:${branch}`;
}

/** Comma / whitespace separated names → list (no empty, no `@`, no duplicates). */
export function splitNames(text: string): string[] {
  const out: string[] = [];
  for (const raw of text.split(/[\s,]+/)) {
    const v = raw.trim().replace(/^@/, "");
    if (v && !out.includes(v)) out.push(v);
  }
  return out;
}

/** Comma separated labels (labels may contain spaces). */
export function splitLabels(text: string): string[] {
  const out: string[] = [];
  for (const raw of text.split(",")) {
    const v = raw.trim();
    if (v && !out.includes(v)) out.push(v);
  }
  return out;
}

const URL_RE = /https:\/\/[^\s<>()"'`]+[^\s<>()"'`.,;:!?]/g;

/** Splits untrusted text into plain parts and `https://` links (nothing else becomes a link). */
export function splitLinks(text: string): { text: string; url: boolean }[] {
  const out: { text: string; url: boolean }[] = [];
  let last = 0;
  for (const m of text.matchAll(URL_RE)) {
    const at = m.index ?? 0;
    if (at > last) out.push({ text: text.slice(last, at), url: false });
    out.push({ text: m[0], url: true });
    last = at + m[0].length;
  }
  if (last < text.length) out.push({ text: text.slice(last), url: false });
  return out;
}

/** Relative time ("3 phút trước") with `Intl.RelativeTimeFormat`. `now`/`ts` in seconds. */
export function relativeTime(ts: number | null, language: string, now = Date.now() / 1000) {
  if (ts === null) return "";
  const diff = ts - now;
  const abs = Math.abs(diff);
  const fmt = new Intl.RelativeTimeFormat(language, { numeric: "auto" });
  if (abs < 60) return fmt.format(Math.round(diff), "second");
  if (abs < 3600) return fmt.format(Math.round(diff / 60), "minute");
  if (abs < 86400) return fmt.format(Math.round(diff / 3600), "hour");
  if (abs < 86400 * 30) return fmt.format(Math.round(diff / 86400), "day");
  if (abs < 86400 * 365) return fmt.format(Math.round(diff / (86400 * 30)), "month");
  return fmt.format(Math.round(diff / (86400 * 365)), "year");
}
