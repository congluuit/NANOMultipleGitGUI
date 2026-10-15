import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands } from "@/ipc";

export const forkKeys = {
  namespaces: (accountId: string) => ["forks", "namespaces", accountId] as const,
  /** Under `["hosting"]`, invalidated after remotes change. */
  remotes: (repoId: number) => ["hosting", "remotes", repoId] as const,
};

/** Where the account may fork to (cached 5 minutes). */
export function useForkNamespaces(accountId: string | null, enabled = true) {
  return useQuery({
    queryKey: forkKeys.namespaces(accountId ?? ""),
    queryFn: () => commands.forkNamespaces(accountId!),
    enabled: isTauri() && enabled && !!accountId,
    staleTime: 5 * 60_000,
    retry: false,
  });
}

/** GitHub / GitLab remotes of an open repository with their accounts. */
export function useForkSourceRemotes(repoId: number) {
  return useQuery({
    queryKey: forkKeys.remotes(repoId),
    queryFn: () => commands.hostedRemoteList(repoId),
    enabled: isTauri(),
    staleTime: 30_000,
  });
}
