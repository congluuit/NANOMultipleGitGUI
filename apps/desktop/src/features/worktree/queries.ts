import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands, errorMessage, type DiffRequest, type StatusEntry } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { repoKeys } from "@/features/repos/queries";
import { useUi } from "@/stores/ui";

export const worktreeKeys = {
  status: (id: number) => ["status", id] as const,
  identity: (id: number) => ["identity", id] as const,
  diffs: (id: number) => ["diff", id] as const,
  diff: (id: number, r: DiffRequest) =>
    ["diff", id, r.side, r.path, r.oldPath ?? null, r.commit ?? null] as const,
  contents: (id: number, r: DiffRequest) =>
    ["contents", id, r.side, r.path, r.oldPath ?? null, r.commit ?? null] as const,
};

export function useWorktreeStatus(repoId: number, enabled = true) {
  return useQuery({
    queryKey: worktreeKeys.status(repoId),
    queryFn: () => commands.worktreeStatus(repoId),
    enabled: isTauri() && enabled,
  });
}

export function useCommitIdentity(repoId: number) {
  return useQuery({
    queryKey: worktreeKeys.identity(repoId),
    queryFn: () => commands.commitIdentity(repoId),
    enabled: isTauri(),
    staleTime: 30_000,
  });
}

/** Re-reads everything that a working-tree operation may have changed. */
export function invalidateWorktree(repoId: number) {
  void queryClient.invalidateQueries({ queryKey: worktreeKeys.status(repoId) });
  void queryClient.invalidateQueries({ queryKey: worktreeKeys.diffs(repoId) });
  void queryClient.invalidateQueries({ queryKey: ["contents", repoId] });
  void queryClient.invalidateQueries({ queryKey: ["opstate", repoId] });
}

/** After a commit, checkout, reset...: history and refs change too. */
export function invalidateRepo(repoId: number) {
  invalidateWorktree(repoId);
  void queryClient.invalidateQueries({ queryKey: repoKeys.refs(repoId) });
  void queryClient.invalidateQueries({ queryKey: repoKeys.graph(repoId) });
}

/** Runs a repository operation, reporting failures in the notice bar. */
export async function runOp<T>(
  repoId: number,
  op: () => Promise<T>,
  refresh: (repoId: number) => void = invalidateWorktree,
): Promise<T | undefined> {
  try {
    return await op();
  } catch (e) {
    useUi.getState().showNotice({ kind: "error", message: errorMessage(e) });
    return undefined;
  } finally {
    refresh(repoId);
  }
}

/** Paths to pass to `unstage` for an entry (both sides of a staged rename). */
export function unstagePaths(entries: StatusEntry[]): string[] {
  return entries.flatMap((e) => (e.oldPath ? [e.oldPath, e.path] : [e.path]));
}

/** Splits entries for `discard`: tracked files are restored, untracked ones deleted. */
export function discardPaths(entries: StatusEntry[]): { tracked: string[]; untracked: string[] } {
  return {
    tracked: entries.filter((e) => e.kind !== "untracked").map((e) => e.path),
    untracked: entries.filter((e) => e.kind === "untracked").map((e) => e.path),
  };
}

/** Diff to open for a file of the staging panel. */
export function requestFor(entry: StatusEntry, staged: boolean): DiffRequest {
  if (staged) return { side: "staged", path: entry.path, oldPath: entry.oldPath ?? undefined };
  return { side: entry.kind === "untracked" ? "untracked" : "unstaged", path: entry.path };
}
