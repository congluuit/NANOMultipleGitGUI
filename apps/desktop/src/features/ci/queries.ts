/**
 * CI data hooks (Phase 4.3). The backend caches per commit (final results ~10 min, running ones
 * ~15 s); these hooks re-read running commits every {@link PENDING_REFRESH_MS} while mounted.
 */
import { useMemo } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { useRepoRefs } from "@/features/repos/queries";
import { primaryRemote } from "@/features/repos/refsView";
import { commands, type CiBatch, type CiEntry, type CiProblem } from "@/ipc";
import { entriesBySha, hasPending, MAX_CI_COMMITS, PENDING_REFRESH_MS } from "./model";

export const ciKeys = {
  all: ["ci"] as const,
  repo: (repoId: number) => ["ci", "repo", repoId] as const,
  statuses: (repoId: number, remote: string | null, withJobs: boolean, shas: string) =>
    ["ci", "repo", repoId, remote, withJobs, shas] as const,
  project: (accountId: string, fullName: string, sha: string) =>
    ["ci", "project", accountId, fullName, sha] as const,
};

const refetch = (q: { state: { data?: CiBatch } }) =>
  hasPending(q.state.data) ? PENDING_REFRESH_MS : false;

/** Remote whose project is asked first: the current branch's upstream remote, else origin. */
export function useCiRemote(repoId: number | null): string | null {
  const refs = useRepoRefs(repoId).data;
  return refs ? (primaryRemote(refs)?.name ?? null) : null;
}

export interface CiStatuses {
  /** By commit id (lower case). */
  bySha: Map<string, CiEntry>;
  problem: CiProblem | null;
  isLoading: boolean;
}

/**
 * CI of several commits of an open repository (graph badges). Pass the hashes worth a badge
 * (`ciCandidateHashes`); at most {@link MAX_CI_COMMITS} are read. Jobs are not loaded.
 */
export function useCiStatuses(repoId: number | null, hashes: readonly string[]): CiStatuses {
  const remote = useCiRemote(repoId);
  const shas = useMemo(
    () => [...new Set(hashes.map((h) => h.toLowerCase()))].slice(0, MAX_CI_COMMITS).sort(),
    [hashes],
  );
  const key = shas.join(",");
  const query = useQuery({
    queryKey: ciKeys.statuses(repoId ?? -1, remote, false, key),
    queryFn: () => commands.ciStatuses(repoId!, shas, remote, false),
    enabled: isTauri() && repoId !== null && shas.length > 0,
    staleTime: 30_000,
    placeholderData: keepPreviousData,
    refetchInterval: refetch,
    retry: false,
  });
  const bySha = useMemo(() => entriesBySha(query.data), [query.data]);
  return { bySha, problem: query.data?.problem ?? null, isLoading: query.isLoading };
}

/** CI of one commit of an open repository, with its jobs (commit details, status bar). */
export function useCiStatus(repoId: number | null, sha: string | null | undefined) {
  const remote = useCiRemote(repoId);
  const id = sha?.toLowerCase() ?? "";
  const query = useQuery({
    queryKey: ciKeys.statuses(repoId ?? -1, remote, true, id),
    queryFn: () => commands.ciStatuses(repoId!, [id], remote, true),
    enabled: isTauri() && repoId !== null && id.length >= 7,
    staleTime: 15_000,
    refetchInterval: refetch,
    retry: false,
  });
  return {
    entry: query.data?.entries[0] ?? null,
    problem: query.data?.problem ?? null,
    isLoading: query.isLoading,
    error: query.error,
  };
}

/** CI of one commit of any project read with `accountId` (PR / MR pages). */
export function useCiStatusFor(accountId: string, fullName: string, sha: string) {
  const query = useQuery({
    queryKey: ciKeys.project(accountId, fullName, sha.toLowerCase()),
    queryFn: () => commands.ciStatusFor(accountId, fullName, sha),
    enabled: isTauri() && accountId.length > 0 && fullName.length > 0 && sha.length >= 7,
    staleTime: 15_000,
    refetchInterval: refetch,
    retry: false,
  });
  return {
    entry: query.data?.entries[0] ?? null,
    problem: query.data?.problem ?? null,
    isLoading: query.isLoading,
    error: query.error,
  };
}
