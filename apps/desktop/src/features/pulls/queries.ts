import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { useAccounts, useProviders } from "@/features/accounts/queries";
import { commands, type PullListQuery } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import type { HostedItemRef } from "@/stores/hosting";
import { familyOfAccount } from "./model";

type Item = Pick<HostedItemRef, "accountId" | "fullName" | "number">;

export const pullKeys = {
  all: ["pulls"] as const,
  remotes: (repoId: number) => ["pulls", "remotes", repoId] as const,
  list: (accountId: string, fullName: string, query: PullListQuery) =>
    ["pulls", "list", accountId, fullName, query] as const,
  item: (r: Item) => ["pulls", "item", r.accountId, r.fullName, r.number] as const,
  part: (r: Item, part: string) =>
    ["pulls", "item", r.accountId, r.fullName, r.number, part] as const,
  createContext: (repoId: number, branch: string | null, remote: string | null) =>
    ["pulls", "createContext", repoId, branch, remote] as const,
};

/** Provider data is fresh enough for a minute; a refresh button forces it. */
const STALE = 60_000;

/** Hosted remotes of a repo with the account each one uses (`origin` first). */
export function useHostedRemotes(repoId: number | null | undefined) {
  return useQuery({
    queryKey: pullKeys.remotes(repoId ?? -1),
    queryFn: () => commands.hostedRemoteList(repoId!),
    enabled: repoId != null && repoId >= 0,
    staleTime: 30_000,
  });
}

const PER_PAGE = 30;

export function usePullList(
  accountId: string | null,
  fullName: string | null,
  query: PullListQuery,
) {
  return useInfiniteQuery({
    queryKey: pullKeys.list(accountId ?? "", fullName ?? "", query),
    queryFn: ({ pageParam }) =>
      commands.pullList(accountId!, fullName!, { ...query, page: pageParam, perPage: PER_PAGE }),
    initialPageParam: 1,
    getNextPageParam: (last) => last.nextPage ?? undefined,
    enabled: !!accountId && !!fullName,
    staleTime: STALE,
    retry: false,
  });
}

export function usePullDetail(r: Item) {
  return useQuery({
    queryKey: pullKeys.part(r, "detail"),
    queryFn: () => commands.pullDetail(r.accountId, r.fullName, r.number),
    staleTime: STALE,
    retry: false,
  });
}

/** Reviewers + decision; also used per sidebar row (lazy, cached longer). */
export function usePullReviews(r: Item, enabled = true) {
  return useQuery({
    queryKey: pullKeys.part(r, "reviews"),
    queryFn: () => commands.pullReviews(r.accountId, r.fullName, r.number),
    staleTime: 2 * STALE,
    enabled,
    retry: false,
  });
}

export function usePullCommits(r: Item, enabled = true) {
  return useQuery({
    queryKey: pullKeys.part(r, "commits"),
    queryFn: () => commands.pullCommits(r.accountId, r.fullName, r.number),
    staleTime: STALE,
    enabled,
    retry: false,
  });
}

export function usePullFiles(r: Item, enabled = true) {
  return useQuery({
    queryKey: pullKeys.part(r, "files"),
    queryFn: () => commands.pullFiles(r.accountId, r.fullName, r.number),
    staleTime: STALE,
    enabled,
    retry: false,
  });
}

export function usePullComments(r: Item) {
  return useQuery({
    queryKey: pullKeys.part(r, "comments"),
    queryFn: () => commands.pullComments(r.accountId, r.fullName, r.number),
    staleTime: STALE,
    retry: false,
  });
}

/** Re-reads everything about one pull request (after a write). */
export function invalidatePull(r: Item) {
  void queryClient.invalidateQueries({ queryKey: pullKeys.item(r) });
}

/** Re-reads the listings (after create / merge / close). */
export function invalidatePullLists() {
  void queryClient.invalidateQueries({ queryKey: ["pulls", "list"] });
}

/** Provider family of an account (`null` while loading). */
export function useAccountFamily(accountId: string | null | undefined) {
  const accounts = useAccounts().data;
  const providers = useProviders().data;
  return familyOfAccount(
    accounts?.find((a) => a.id === accountId),
    providers,
  );
}
