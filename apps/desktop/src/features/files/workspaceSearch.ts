/**
 * Search in every open repository (the search panel's "All open repositories" scope).
 *
 * The backend searches the repositories a few at a time and sends each repository's result on
 * a channel as soon as it is known; the results live here, so they survive the panel being
 * hidden. A new query cancels the running search (`transfer_cancel`) and anything the old
 * search still sends is ignored.
 */
import { Channel } from "@tauri-apps/api/core";
import { create } from "zustand";
import {
  commands,
  errorMessage,
  type RepoSearchResult,
  type RepoTab,
  type SearchMatch,
  type SearchOptions,
  type WorkspaceSearchSummary,
} from "@/ipc";
import { groupByFile } from "./search";

/** Matching lines over every repository together. */
export const WORKSPACE_LIMIT = 2000;

export type WorkspaceSearchStatus = "idle" | "running" | "done" | "cancelled" | "error";

export interface WorkspaceSearchState {
  /** Query and options of the current results (see {@link searchKey}). */
  key: string | null;
  query: string;
  options: SearchOptions | null;
  status: WorkspaceSearchStatus;
  /** One entry per repository, in the order they finished. */
  results: RepoSearchResult[];
  /** Open repositories when the search started. */
  repoCount: number;
  summary: WorkspaceSearchSummary | null;
  error: string | null;
  opId: string | null;
}

const initial: WorkspaceSearchState = {
  key: null,
  query: "",
  options: null,
  status: "idle",
  results: [],
  repoCount: 0,
  summary: null,
  error: null,
  opId: null,
};

export const useWorkspaceSearch = create<WorkspaceSearchState>()(() => initial);

export const searchKey = (query: string, options: SearchOptions) =>
  JSON.stringify([query, options]);

let seq = 0;

function stopRunning() {
  const s = useWorkspaceSearch.getState();
  if (s.status === "running" && s.opId) {
    void commands.transferCancel(s.opId).catch(() => {});
  }
}

/**
 * Searches every open repository for `query`. Does nothing when the same search already ran
 * (unless `force`); an empty query clears the results. A search still running is cancelled.
 */
export function runWorkspaceSearch(
  query: string,
  options: SearchOptions,
  repoCount: number,
  { force = false }: { force?: boolean } = {},
) {
  const key = searchKey(query, options);
  if (!force && useWorkspaceSearch.getState().key === key) return;
  stopRunning();
  if (!query.trim()) {
    useWorkspaceSearch.setState(initial);
    return;
  }
  seq += 1;
  const opId = `workspace-search-${seq}-${Date.now()}`;
  useWorkspaceSearch.setState({
    ...initial,
    key,
    query,
    options,
    status: "running",
    repoCount,
    opId,
  });
  const current = () => {
    const s = useWorkspaceSearch.getState();
    return s.opId === opId ? s : null;
  };
  const channel = new Channel<RepoSearchResult>();
  channel.onmessage = (result) => {
    if (current()?.status !== "running") return;
    useWorkspaceSearch.setState((s) => ({ results: [...s.results, result] }));
  };
  commands
    .workspaceSearch(opId, query, options, WORKSPACE_LIMIT, channel)
    .then((summary) => {
      const s = current();
      if (!s) return;
      const status = s.status === "cancelled" || summary.cancelled ? "cancelled" : "done";
      useWorkspaceSearch.setState({ status, summary });
    })
    .catch((e: unknown) => {
      if (!current()) return;
      useWorkspaceSearch.setState({ status: "error", error: errorMessage(e) });
    });
}

/** Stops the running search; what was found so far stays. */
export function cancelWorkspaceSearch() {
  if (useWorkspaceSearch.getState().status !== "running") return;
  stopRunning();
  useWorkspaceSearch.setState({ status: "cancelled" });
}

/** Forgets the results (and stops a running search). */
export function resetWorkspaceSearch() {
  stopRunning();
  useWorkspaceSearch.setState(initial);
}

/** One repository of the results, as the panel lists it. */
export interface RepoGroup {
  repoId: number;
  name: string;
  matches: SearchMatch[];
  files: [string, SearchMatch[]][];
  truncated: boolean;
  missing: boolean;
  error: string | null;
}

/**
 * The repositories to list, in tab order: those with matches, and those that could not be
 * searched (with the reason). Repositories without a match, and tabs closed since, are left
 * out.
 */
export function groupWorkspaceResults(
  results: readonly RepoSearchResult[],
  tabs: readonly RepoTab[],
): RepoGroup[] {
  const order = new Map(tabs.map((t, i) => [t.id, i]));
  return results
    .filter((r) => order.has(r.repoId) && (r.matches.length > 0 || r.missing || r.error !== null))
    .sort((a, b) => (order.get(a.repoId) ?? 0) - (order.get(b.repoId) ?? 0))
    .map((r) => ({
      repoId: r.repoId,
      name: r.name,
      matches: r.matches,
      files: groupByFile(r.matches),
      truncated: r.truncated,
      missing: r.missing,
      error: r.error,
    }));
}

/** Matching lines, files and repositories with a match. */
export function workspaceTotals(groups: readonly RepoGroup[]) {
  let matches = 0;
  let files = 0;
  let repos = 0;
  for (const g of groups) {
    matches += g.matches.length;
    files += g.files.length;
    if (g.matches.length > 0) repos += 1;
  }
  return { matches, files, repos };
}
