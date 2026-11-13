/**
 * Repositories in the middle of a long batch (staging thousands of files in chunks): their
 * status is not refreshed on every file-system event, which would re-read and re-render the
 * whole file list after each chunk. The batch refreshes once when it ends.
 */
const quiet = new Set<number>();

export function setQuiet(repoId: number, on: boolean) {
  if (on) quiet.add(repoId);
  else quiet.delete(repoId);
}

export const isQuiet = (repoId: number) => quiet.has(repoId);
