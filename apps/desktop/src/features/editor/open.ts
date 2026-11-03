/**
 * Opening any file of the repository in the built-in editor (quick open, file tree, search
 * results), optionally at a line, and the per-repository list of recently opened files.
 */
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { create } from "zustand";
import { useExpandedFolders } from "@/features/files/store";
import { worktreeKeys } from "@/features/worktree/queries";
import { commands, type WorkingTreeStatus } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useHostingNav } from "@/stores/hosting";
import { useWorktreeStore } from "@/stores/worktree";

/** Where the editor puts the cursor once the file is shown (1-based; line 0: focus only). */
export interface Reveal {
  repoId: number;
  path: string;
  line: number;
  column: number;
}

export const useReveal = create<{ reveal: Reveal | null }>()(() => ({ reveal: null }));

/** Takes the pending reveal for a file (it applies once). */
export function takeReveal(repoId: number, path: string): Reveal | null {
  const r = useReveal.getState().reveal;
  if (!r || r.repoId !== repoId || r.path !== path) return null;
  useReveal.setState({ reveal: null });
  return r;
}

function isUntracked(repoId: number, path: string): boolean {
  const status = queryClient.getQueryData<WorkingTreeStatus>(worktreeKeys.status(repoId));
  if (status) return status.unstaged.some((e) => e.path === path && e.kind === "untracked");
  const files = queryClient.getQueryData<{ untracked: string[] }>(worktreeKeys.files(repoId));
  return !!files?.untracked.includes(path);
}

export interface OpenOptions {
  line?: number;
  column?: number;
  /** Known to be untracked (just created): skips the lookup in the cached status. */
  untracked?: boolean;
}

/** Shows a working-tree file in the editor ("File" mode of the diff view). */
export function openInEditor(repoId: number, path: string, opts: OpenOptions = {}) {
  const nav = useHostingNav.getState();
  nav.closePullRequest();
  nav.closeIssue();
  // Without a line, the editor only takes the focus (line 0).
  useReveal.setState({
    reveal: { repoId, path, line: opts.line ?? 0, column: opts.column ?? 1 },
  });
  const untracked = opts.untracked ?? isUntracked(repoId, path);
  const store = useWorktreeStore.getState();
  store.setDiffMode("file");
  store.openDiff(repoId, { side: untracked ? "untracked" : "unstaged", path });
  useExpandedFolders.getState().reveal(repoId, path);
  pushRecentFile(repoId, path);
}

export function useRepoFiles(repoId: number | null, enabled = true) {
  return useQuery({
    queryKey: worktreeKeys.files(repoId ?? -1),
    queryFn: () => commands.repoFiles(repoId ?? -1),
    enabled: isTauri() && repoId !== null && enabled,
    staleTime: 10_000,
  });
}

const RECENT_KEY = "nanomgg.recentFiles";
const RECENT_LIMIT = 15;

export function recentFiles(repoId: number): string[] {
  try {
    const all: unknown = JSON.parse(globalThis.localStorage?.getItem(RECENT_KEY) ?? "{}");
    const list = (all as Record<string, unknown>)[repoId];
    return Array.isArray(list) ? list.filter((p): p is string => typeof p === "string") : [];
  } catch {
    return [];
  }
}

function pushRecentFile(repoId: number, path: string) {
  try {
    const all = JSON.parse(globalThis.localStorage?.getItem(RECENT_KEY) ?? "{}") as Record<
      string,
      string[]
    >;
    all[repoId] = [path, ...recentFiles(repoId).filter((p) => p !== path)].slice(0, RECENT_LIMIT);
    globalThis.localStorage?.setItem(RECENT_KEY, JSON.stringify(all));
  } catch {
    // Recents are a convenience only.
  }
}
