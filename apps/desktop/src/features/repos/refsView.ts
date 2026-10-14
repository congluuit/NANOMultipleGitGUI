import type { Head, RepoRefs } from "@/ipc";

export type Transport = "HTTPS" | "HTTP" | "SSH" | "LOCAL";

/** How a remote URL is reached: `https://…`, `git@host:path` / `ssh://…`, or a local path. */
export function transportOf(url: string | null | undefined): Transport | null {
  if (!url) return null;
  const u = url.trim().toLowerCase();
  if (u.startsWith("https://")) return "HTTPS";
  if (u.startsWith("http://")) return "HTTP";
  if (u.startsWith("ssh://") || u.startsWith("git+ssh://")) return "SSH";
  if (u.startsWith("file://")) return "LOCAL";
  // scp-like syntax `user@host:path`, but not a Windows drive path like `C:\repo`.
  if (/^[\w.-]+@[\w.-]+:/.test(u)) return "SSH";
  return "LOCAL";
}

export const shortOid = (oid: string | null | undefined) => (oid ? oid.slice(0, 7) : "");

export function matchesFilter(name: string, filter: string): boolean {
  const f = filter.trim().toLowerCase();
  return !f || name.toLowerCase().includes(f);
}

/** Remote used for the status bar: the current branch's upstream remote, else `origin`, else the first. */
export function primaryRemote(refs: RepoRefs) {
  const current = refs.local.find((b) => b.isHead);
  const upstreamRemote = current?.upstream
    ? refs.remotes
        .filter((r) => current.upstream!.startsWith(`${r.name}/`))
        .sort((a, b) => b.name.length - a.name.length)[0]
    : undefined;
  return upstreamRemote ?? refs.remotes.find((r) => r.name === "origin") ?? refs.remotes[0] ?? null;
}

export type HeadState =
  | { kind: "branch"; branch: string }
  | { kind: "unborn"; branch: string }
  | { kind: "detached"; oid: string };

export function headState(head: Head): HeadState {
  if (head.unborn) return { kind: "unborn", branch: head.branch ?? "" };
  if (head.detached || !head.branch) return { kind: "detached", oid: shortOid(head.oid) };
  return { kind: "branch", branch: head.branch };
}
