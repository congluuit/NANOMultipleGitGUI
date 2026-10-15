import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands, type IssueListQuery, type IssueStateFilter } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import type { HostedItemRef } from "@/stores/hosting";

export const issueKeys = {
  all: ["issues"] as const,
  remotes: (repoId: number) => ["issues", "remotes", repoId] as const,
  list: (repoId: number, query: IssueListQuery) => ["issues", "list", repoId, query] as const,
  open: (repoId: number) => ["issues", "list", repoId, "openForSuggest"] as const,
  detail: (r: Pick<HostedItemRef, "accountId" | "fullName" | "number">) =>
    ["issues", "detail", r.accountId, r.fullName, r.number] as const,
  comments: (r: Pick<HostedItemRef, "accountId" | "fullName" | "number">) =>
    ["issues", "comments", r.accountId, r.fullName, r.number] as const,
  labels: (accountId: string, fullName: string) =>
    ["issues", "labels", accountId, fullName] as const,
  assignable: (accountId: string, fullName: string) =>
    ["issues", "assignable", accountId, fullName] as const,
};

/** GitHub / GitLab remotes of a repository with the account each uses (`origin` first). */
export function useHostedRemotes(repoId: number | null | undefined) {
  return useQuery({
    queryKey: issueKeys.remotes(repoId ?? -1),
    queryFn: () => commands.hostedRemoteList(repoId!),
    enabled: isTauri() && repoId != null,
    staleTime: 30_000,
  });
}

export interface IssueFilter {
  state: IssueStateFilter;
  label: string | null;
  assignee: string | null;
  author: string | null;
  search: string;
}

export const DEFAULT_FILTER: IssueFilter = {
  state: "open",
  label: null,
  assignee: null,
  author: null,
  search: "",
};

export const toQuery = (f: IssueFilter, page: number, perPage = 30): IssueListQuery => ({
  state: f.state,
  labels: f.label ? [f.label] : [],
  assignee: f.assignee,
  author: f.author,
  search: f.search.trim() || null,
  page,
  perPage,
});

/** One page of issues of the repository's main hosted remote. */
export function useIssueList(repoId: number, query: IssueListQuery, enabled: boolean) {
  return useQuery({
    queryKey: issueKeys.list(repoId, query),
    queryFn: () => commands.issueList(repoId, null, query),
    enabled: isTauri() && enabled,
    staleTime: 60_000,
    placeholderData: keepPreviousData,
    retry: false,
  });
}

/** Open issues for the `#` suggestions of the commit box (first 100). */
export function useOpenIssuesForSuggest(repoId: number, enabled: boolean) {
  return useQuery({
    queryKey: issueKeys.open(repoId),
    queryFn: () => commands.issueList(repoId, null, { state: "open", page: 1, perPage: 100 }),
    enabled: isTauri() && enabled,
    staleTime: 2 * 60_000,
    retry: false,
  });
}

export function useIssue(ref: HostedItemRef) {
  return useQuery({
    queryKey: issueKeys.detail(ref),
    queryFn: () => commands.issueGet(ref.accountId, ref.fullName, ref.number),
    enabled: isTauri(),
    staleTime: 30_000,
    retry: false,
  });
}

export function useIssueComments(ref: HostedItemRef) {
  return useQuery({
    queryKey: issueKeys.comments(ref),
    queryFn: () => commands.issueComments(ref.accountId, ref.fullName, ref.number),
    enabled: isTauri(),
    staleTime: 30_000,
    retry: false,
  });
}

export function useIssueLabels(accountId: string | null, fullName: string | null) {
  return useQuery({
    queryKey: issueKeys.labels(accountId ?? "", fullName ?? ""),
    queryFn: () => commands.issueLabels(accountId!, fullName!),
    enabled: isTauri() && !!accountId && !!fullName,
    staleTime: 5 * 60_000,
    retry: false,
  });
}

export function useAssignableUsers(accountId: string | null, fullName: string | null) {
  return useQuery({
    queryKey: issueKeys.assignable(accountId ?? "", fullName ?? ""),
    queryFn: () => commands.issueAssignableUsers(accountId!, fullName!),
    enabled: isTauri() && !!accountId && !!fullName,
    staleTime: 5 * 60_000,
    retry: false,
  });
}

/** Listings and the "#" suggestions after an issue changed. */
export function invalidateIssueLists() {
  void queryClient.invalidateQueries({ queryKey: ["issues", "list"] });
}
