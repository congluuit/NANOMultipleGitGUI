import type { PushRequest, RepoRefs, TransferProgress } from "@/ipc";

/** Splits `origin/feature/x` into remote and branch using the configured remote names. */
export function splitUpstream(
  upstream: string,
  remotes: readonly string[],
): { remote: string; branch: string } | null {
  const remote = remotes
    .filter((r) => upstream.startsWith(`${r}/`))
    .sort((a, b) => b.length - a.length)[0];
  return remote ? { remote, branch: upstream.slice(remote.length + 1) } : null;
}

/**
 * What "Push" does for the current branch: to its upstream when it has one, otherwise to
 * `origin` (or the only remote) with `--set-upstream`. `null` when there is nothing to push to.
 */
export function pushPlan(refs: RepoRefs, forceWithLease = false): PushRequest | null {
  const head = refs.head;
  if (head.detached || head.unborn || !head.branch) return null;
  const names = refs.remotes.map((r) => r.name);
  const current = refs.local.find((b) => b.isHead);
  const up =
    current?.upstream && !current.upstreamGone ? splitUpstream(current.upstream, names) : null;
  if (up) {
    return {
      remote: up.remote,
      branch: head.branch,
      remoteBranch: up.branch === head.branch ? null : up.branch,
      setUpstream: false,
      forceWithLease,
    };
  }
  const remote = names.includes("origin") ? "origin" : names.length === 1 ? names[0] : undefined;
  if (!remote) return null;
  return { remote, branch: head.branch, remoteBranch: null, setUpstream: true, forceWithLease };
}

/** One-line description of a progress event: `Receiving objects 45% · 1.2 MiB · 2 MiB/s`. */
export function progressText(
  p: Pick<TransferProgress, "label" | "percent" | "bytes" | "throughput" | "current" | "total">,
): string {
  const parts = [p.label];
  if (p.percent !== null) parts[0] = `${p.label} ${p.percent}%`;
  else if (p.current !== null && p.total !== null) parts[0] = `${p.label} ${p.current}/${p.total}`;
  if (p.bytes) parts.push(p.bytes);
  if (p.throughput) parts.push(p.throughput);
  return parts.join(" · ");
}

let counter = 0;

/** Unique id for a transfer started by this window. */
export function newOpId(kind: string): string {
  counter += 1;
  return `${kind}-${Date.now().toString(36)}-${counter}`;
}
