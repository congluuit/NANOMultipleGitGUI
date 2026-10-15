/** A run of consecutive lines that come from the same commit. */
export interface BlameGroup {
  /** 0-based first line. */
  start: number;
  count: number;
  /** Index into the blame's commits. */
  commit: number;
}

/** Groups consecutive lines with the same commit. */
export function groupLines(lineCommits: number[]): BlameGroup[] {
  const groups: BlameGroup[] = [];
  lineCommits.forEach((commit, i) => {
    const last = groups[groups.length - 1];
    if (last && last.commit === commit && last.start + last.count === i) last.count++;
    else groups.push({ start: i, count: 1, commit });
  });
  return groups;
}

/** For each line, the index of its group (for O(1) lookups while rendering). */
export function groupIndex(groups: BlameGroup[], lines: number): Int32Array {
  const out = new Int32Array(lines);
  groups.forEach((g, i) => out.fill(i, g.start, g.start + g.count));
  return out;
}

export const AGE_BUCKETS = 6;

/**
 * Age bucket of a commit: 0 = newest … AGE_BUCKETS - 1 = oldest, spread over the file's own
 * time range (a log scale, so recent weeks are told apart even in an old file).
 */
export function ageBucket(time: number, newest: number, oldest: number): number {
  if (newest <= oldest || time >= newest) return 0;
  if (time <= oldest) return AGE_BUCKETS - 1;
  const span = Math.log1p(newest - oldest);
  const age = Math.log1p(newest - time);
  return Math.min(AGE_BUCKETS - 1, Math.floor((age / span) * AGE_BUCKETS));
}

/** Opacity of the age bar for a bucket: new commits strong, old ones faint. */
export function ageOpacity(bucket: number): number {
  return 1 - (bucket / (AGE_BUCKETS - 1)) * 0.8;
}

const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 365 * 24 * 3600],
  ["month", 30 * 24 * 3600],
  ["week", 7 * 24 * 3600],
  ["day", 24 * 3600],
  ["hour", 3600],
  ["minute", 60],
];

/** "3 days ago" / "3 ngày trước". */
export function relativeTime(seconds: number, nowSeconds: number, locale: string): string {
  const diff = seconds - nowSeconds;
  const fmt = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
  for (const [unit, size] of UNITS) {
    if (Math.abs(diff) >= size) return fmt.format(Math.round(diff / size), unit);
  }
  return fmt.format(0, "minute");
}

/** Range of commit times, ignoring uncommitted lines (time 0 / "now"). */
export function timeRange(commits: { time: number; uncommitted: boolean }[]): {
  newest: number;
  oldest: number;
} {
  let newest = -Infinity;
  let oldest = Infinity;
  for (const c of commits) {
    if (c.uncommitted) continue;
    newest = Math.max(newest, c.time);
    oldest = Math.min(oldest, c.time);
  }
  if (!Number.isFinite(newest)) return { newest: 0, oldest: 0 };
  return { newest, oldest };
}

/** Rows to render for a scroll position (with overscan). */
export function visibleRange(
  scrollTop: number,
  height: number,
  rowHeight: number,
  total: number,
  overscan = 20,
): { first: number; last: number } {
  const first = Math.max(0, Math.floor(scrollTop / rowHeight) - overscan);
  const last = Math.min(total, Math.ceil((scrollTop + height) / rowHeight) + overscan);
  return { first, last };
}
