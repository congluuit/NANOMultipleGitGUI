import type { HostedRemote, IssueSummary, ProviderFamily } from "@/ipc";

/** A piece of untrusted text: plain text, an `https://` link, or an issue / PR reference. */
export type TextPart =
  | { type: "text"; text: string }
  | { type: "url"; url: string }
  /** `#12` (GitHub: issue or PR; GitLab: issue) or `!12` (GitLab merge request). */
  | { type: "ref"; raw: string; number: number; mergeRequest: boolean };

const TOKEN = /(https:\/\/[^\s<>"'`]+)|(?<![\w&/#!])([#!])(\d{1,7})(?![\w-])/g;

/** Splits `text` into parts; `!12` is only a reference on GitLab. */
export function splitRefs(text: string, family: ProviderFamily | null): TextPart[] {
  const parts: TextPart[] = [];
  let last = 0;
  const push = (text: string) => {
    if (!text) return;
    const prev = parts[parts.length - 1];
    if (prev?.type === "text") prev.text += text;
    else parts.push({ type: "text", text });
  };
  for (const m of text.matchAll(TOKEN)) {
    const index = m.index ?? 0;
    push(text.slice(last, index));
    last = index + m[0].length;
    if (m[1]) {
      // Trailing punctuation belongs to the sentence, not the URL.
      const url = m[1].replace(/[.,;:!?)\]]+$/, "");
      parts.push({ type: "url", url });
      push(m[1].slice(url.length));
      continue;
    }
    const mergeRequest = m[2] === "!";
    const number = Number(m[3]);
    if ((mergeRequest && family !== "gitlab") || number <= 0) {
      push(m[0]);
      continue;
    }
    parts.push({ type: "ref", raw: m[0], number, mergeRequest });
  }
  push(text.slice(last));
  return parts;
}

/** An unfinished `#…` before the caret: where it starts and what was typed after `#`. */
export interface RefTrigger {
  start: number;
  query: string;
}

export function refTrigger(value: string, caret: number): RefTrigger | null {
  const before = value.slice(0, caret);
  const m = /(^|[\s([{,;])#([^\s#]{0,40})$/u.exec(before);
  if (!m) return null;
  return { start: before.length - m[2]!.length - 1, query: m[2]! };
}

/** Replaces the trigger with `insert` (plus a space); returns the new value and caret. */
export function applyRef(
  value: string,
  trigger: RefTrigger,
  caret: number,
  insert: string,
): { value: string; caret: number } {
  const after = value.slice(caret);
  const glue = after.startsWith(" ") ? "" : " ";
  const next = value.slice(0, trigger.start) + insert + glue + after;
  return { value: next, caret: trigger.start + insert.length + 1 };
}

/** Lower-case without Vietnamese / Latin accents, for loose matching. */
export function fold(text: string): string {
  return text.toLowerCase().normalize("NFD").replace(/\p{M}/gu, "").replace(/đ/g, "d");
}

/** Open issues matching what was typed after `#` (number prefix or words of the title). */
export function matchIssues(issues: readonly IssueSummary[], query: string, max = 8) {
  const q = fold(query.trim());
  if (!q) return issues.slice(0, max);
  const byNumber = /^\d+$/.test(q);
  return issues
    .filter((i) => (byNumber ? String(i.number).startsWith(q) : fold(i.title).includes(q)))
    .slice(0, max);
}

/** The hosted remote the issue list uses: `origin` first (the backend already sorts). */
export const primaryRemote = (remotes: readonly HostedRemote[] | undefined) => remotes?.[0] ?? null;

/** Host name without port, lower-cased (`GitLab.acme.dev:8443` → `gitlab.acme.dev`). */
export function hostName(hostOrUrl: string): string {
  let h = hostOrUrl.trim().toLowerCase();
  if (h.includes("://")) {
    try {
      return new URL(h).hostname;
    } catch {
      return h;
    }
  }
  h = h.replace(/^[^@]*@/, "");
  if (h.startsWith("[")) return h.slice(0, h.indexOf("]") + 1);
  return h.split(":")[0] ?? h;
}

/** Same project: equal full name (case-insensitive) on the same host. */
export const sameProject = (
  remote: Pick<HostedRemote, "host" | "fullName">,
  host: string,
  fullName: string,
) =>
  hostName(remote.host) === hostName(host) &&
  remote.fullName.toLowerCase() === fullName.toLowerCase();
