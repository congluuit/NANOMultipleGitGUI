import type { FsckProblem, FsckProblemKind, LfsFile, LfsLock, LockList } from "@/ipc";

/** Locks by repository path. */
export function locksByPath(list: LockList | null | undefined): Map<string, LfsLock> {
  return new Map((list?.locks ?? []).map((l) => [l.path, l]));
}

export type FileState = "downloaded" | "pointer" | "missing";

/** What the user sees for an LFS file: real content, pointer only, or object missing. */
export function fileState(f: LfsFile): FileState {
  if (f.checkedOut) return "downloaded";
  return f.downloaded ? "pointer" : "missing";
}

export interface FileFilter {
  text: string;
  pointersOnly: boolean;
  lockedOnly: boolean;
}

export function filterFiles(
  files: readonly LfsFile[],
  locks: Map<string, LfsLock>,
  f: FileFilter,
): LfsFile[] {
  const q = f.text.trim().toLowerCase();
  return files.filter(
    (file) =>
      (!q || file.path.toLowerCase().includes(q)) &&
      (!f.pointersOnly || !file.checkedOut) &&
      (!f.lockedOnly || locks.has(file.path)),
  );
}

/** Totals for the panel summary. */
export function fileTotals(files: readonly LfsFile[]) {
  let bytes = 0;
  let pointers = 0;
  for (const f of files) {
    bytes += f.size;
    if (!f.checkedOut) pointers++;
  }
  return { count: files.length, bytes, pointers };
}

/** Paths of the selection that still need downloading. */
export function downloadable(files: readonly LfsFile[], selected: ReadonlySet<string>): string[] {
  return files.filter((f) => selected.has(f.path) && !f.checkedOut).map((f) => f.path);
}

/** Lock badge for a working-tree file: `null` when not locked. */
export function lockBadge(lock: LfsLock | undefined): { owner: string; ours: boolean } | null {
  if (!lock) return null;
  return { owner: lock.owner ?? "?", ours: lock.ours };
}

/** Problems grouped by kind for the fsck summary. */
export function groupProblems(
  problems: readonly FsckProblem[],
): Map<FsckProblemKind, FsckProblem[]> {
  const map = new Map<FsckProblemKind, FsckProblem[]>();
  for (const p of problems) map.set(p.kind, [...(map.get(p.kind) ?? []), p]);
  return map;
}
