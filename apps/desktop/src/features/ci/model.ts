/**
 * Pure logic of the CI status feature (PLAN §5.6, Phase 4.3): which commits to ask for, the
 * glyph / colour of a state, job counts and durations.
 */
import type { CiBatch, CiEntry, CiJob, CiJobState, CiState, GraphRow, RepoRefs } from "@/ipc";

/** Most commits asked for in one call (the backend caps at 40 too). */
export const MAX_CI_COMMITS = 40;
/** Recent commits asked for besides the branch tips. */
export const RECENT_COMMITS = 20;
/** Running pipelines are re-read this often while they are on screen. */
export const PENDING_REFRESH_MS = 20_000;

/** Glyph of a state: ✓ ✗ ● – (and nothing for "no CI"). */
export function ciGlyph(state: CiState): string {
  switch (state) {
    case "success":
      return "✓";
    case "failure":
      return "✗";
    case "pending":
      return "●";
    case "neutral":
      return "–";
    case "none":
      return "";
  }
}

/** Theme token (`--success`…) of a state; `null` = muted text. */
export function ciToken(state: CiState | CiJobState): "success" | "danger" | "warning" | null {
  switch (state) {
    case "success":
      return "success";
    case "failure":
    case "cancelled":
      return "danger";
    case "pending":
    case "queued":
    case "running":
      return "warning";
    default:
      return null;
  }
}

/** Tailwind text class of a state. */
export function ciTextClass(state: CiState | CiJobState): string {
  // Literal class names: Tailwind only generates classes it finds in the source.
  switch (ciToken(state)) {
    case "success":
      return "text-success";
    case "danger":
      return "text-danger";
    case "warning":
      return "text-warning";
    default:
      return "text-muted-foreground";
  }
}

/**
 * Commits worth a CI badge: HEAD, the tips of local branches and the first
 * {@link RECENT_COMMITS} rows of the graph (newest first), de-duplicated and capped.
 */
export function ciCandidateHashes(
  refs: RepoRefs | null | undefined,
  rows: readonly Pick<GraphRow, "hash">[] = [],
  limit = MAX_CI_COMMITS,
): string[] {
  const out: string[] = [];
  const add = (oid: string | null | undefined) => {
    if (oid && out.length < limit && !out.includes(oid)) out.push(oid);
  };
  add(refs?.head.oid);
  for (const b of refs?.local ?? []) add(b.oid);
  for (const row of rows.slice(0, RECENT_COMMITS)) add(row.hash);
  return out;
}

/** Entries of a batch by commit id (lower case). */
export function entriesBySha(batch: CiBatch | undefined): Map<string, CiEntry> {
  const map = new Map<string, CiEntry>();
  for (const e of batch?.entries ?? []) map.set(e.status.sha.toLowerCase(), e);
  return map;
}

/** Whether something in the batch is still running (keep polling). */
export const hasPending = (batch: CiBatch | undefined) =>
  batch?.entries.some((e) => e.status.state === "pending") ?? false;

export interface JobCounts {
  total: number;
  success: number;
  failure: number;
  running: number;
  other: number;
}

export function countJobs(jobs: readonly CiJob[]): JobCounts {
  const counts: JobCounts = { total: jobs.length, success: 0, failure: 0, running: 0, other: 0 };
  for (const j of jobs) {
    if (j.state === "success") counts.success++;
    else if ((j.state === "failure" || j.state === "cancelled") && !j.allowFailure)
      counts.failure++;
    else if (j.state === "queued" || j.state === "running") counts.running++;
    else counts.other++;
  }
  return counts;
}

/** Failed first, then running, then the rest; stable otherwise. */
export function sortJobs(jobs: readonly CiJob[]): CiJob[] {
  const rank = (j: CiJob) =>
    (j.state === "failure" || j.state === "cancelled") && !j.allowFailure
      ? 0
      : j.state === "running" || j.state === "queued"
        ? 1
        : 2;
  return jobs
    .map((job, i) => ({ job, i }))
    .sort((a, b) => rank(a.job) - rank(b.job) || a.i - b.i)
    .map((x) => x.job);
}

/** `1m 05s`, `42s`, `1h 02m`; `null` when the job has not started. `now` in Unix seconds. */
export function jobDuration(job: Pick<CiJob, "startedAt" | "finishedAt">, now: number) {
  if (job.startedAt == null) return null;
  const secs = Math.max(0, (job.finishedAt ?? now) - job.startedAt);
  if (secs < 60) return `${secs}s`;
  const m = Math.floor(secs / 60);
  if (m < 60) return `${m}m ${String(secs % 60).padStart(2, "0")}s`;
  return `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, "0")}m`;
}

/** Only https links are opened. */
export const safeUrl = (url: string | null | undefined) =>
  url && /^https:\/\//i.test(url) ? url : null;
