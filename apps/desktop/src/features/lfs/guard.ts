import type { GuardFinding, GuardLevel, GuardProvider, GuardReport, RepoRefs } from "@/ipc";

/** Host of a remote URL: `https://host/…`, `ssh://user@host:22/…` or scp-like `user@host:path`. */
export function remoteHost(url: string): string | null {
  const trimmed = url.trim();
  const withScheme = /^[a-z][a-z0-9+.-]*:\/\//i.exec(trimmed);
  if (withScheme) {
    try {
      return new URL(trimmed).hostname.toLowerCase() || null;
    } catch {
      return null;
    }
  }
  // scp-like syntax: [user@]host:path (a single drive letter is a Windows path, not a host).
  const host = /^(?:[^@/]+@)?([^:/]+):(?!\/\/)/.exec(trimmed)?.[1];
  return host && host.length > 1 ? host.toLowerCase() : null;
}

/** Large File Guard provider for a remote URL (PLAN §6.2 thresholds). */
export function providerFromUrl(url: string | null | undefined): GuardProvider {
  const host = url ? remoteHost(url) : null;
  if (!host) return "other";
  if (host === "github.com" || host.endsWith(".github.com")) return "github";
  if (host.includes("gitlab")) return "gitlab";
  return "other";
}

/** Provider of the repository's main remote: `origin` if present, else the first with a URL. */
export function guessProvider(refs: Pick<RepoRefs, "remotes"> | undefined): GuardProvider {
  const remotes = refs?.remotes ?? [];
  const main = remotes.find((r) => r.name === "origin" && r.url) ?? remotes.find((r) => r.url);
  return providerFromUrl(main?.url);
}

const SEVERITY: Record<GuardLevel, number> = { none: 0, suggest: 1, warn: 2, block: 3 };

/** Findings the dialog lists: anything over a threshold, suggested, or staged un-normalized. */
export function relevantFindings(report: GuardReport): GuardFinding[] {
  return report.findings
    .filter((f) => f.level !== "none" || f.needsRenormalize)
    .sort((a, b) => SEVERITY[b.level] - SEVERITY[a.level]);
}

export interface GuardSummary {
  block: number;
  warn: number;
  suggest: number;
  /** Files over the warn or block threshold. */
  over: number;
  /** "Commit anyway" must be disabled. */
  blocked: boolean;
}

export function summarize(findings: GuardFinding[]): GuardSummary {
  const count = (level: GuardLevel) => findings.filter((f) => f.level === level).length;
  const block = count("block");
  const warn = count("warn");
  return { block, warn, suggest: count("suggest"), over: block + warn, blocked: block > 0 };
}

export type TrackMode = "extension" | "path";

/**
 * `.gitattributes` patterns to add for `findings`: `*.ext` (or the anchored path when the
 * file has no extension) or the anchored path. Files already routed through LFS only need
 * re-staging. Deduplicated, in first-seen order.
 */
export function choosePatterns(findings: GuardFinding[], mode: TrackMode): string[] {
  const out: string[] = [];
  for (const f of findings) {
    if (f.alreadyLfs) continue;
    const pattern =
      mode === "extension" ? (f.patternByExtension ?? f.patternByPath) : f.patternByPath;
    if (!out.includes(pattern)) out.push(pattern);
  }
  return out;
}

/** Size shown next to a file: the real file size, else what the commit would store. */
export const displaySize = (f: GuardFinding): number | null => f.diskSize ?? f.size;
