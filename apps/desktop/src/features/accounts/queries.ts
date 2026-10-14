import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands } from "@/ipc";
import { queryClient } from "@/lib/queryClient";

export const accountKeys = {
  all: ["accounts"] as const,
  providers: ["accounts", "providers"] as const,
  list: ["accounts", "list"] as const,
  identities: (accountId: string | null) => ["accounts", "identities", accountId] as const,
  rules: ["accounts", "rules"] as const,
  settings: ["accounts", "settings"] as const,
  repoContext: (repoId: number) => ["accounts", "repoContext", repoId] as const,
};

export function useProviders() {
  return useQuery({
    queryKey: accountKeys.providers,
    queryFn: () => commands.providerList(),
    enabled: isTauri(),
  });
}

export function useAccounts() {
  return useQuery({
    queryKey: accountKeys.list,
    queryFn: () => commands.accountList(),
    enabled: isTauri(),
  });
}

/** Identities of one account, or of every account when `accountId` is `null`. */
export function useIdentities(accountId: string | null) {
  return useQuery({
    queryKey: accountKeys.identities(accountId),
    queryFn: () => commands.identityList(accountId),
    enabled: isTauri(),
  });
}

export function useRules() {
  return useQuery({
    queryKey: accountKeys.rules,
    queryFn: () => commands.ruleList(),
    enabled: isTauri(),
  });
}

/** Which account/identity a repository uses, and why (binding, rule or none). */
export function useRepoAccountContext(repoId: number | null | undefined) {
  return useQuery({
    queryKey: accountKeys.repoContext(repoId ?? -1),
    queryFn: () => commands.repoAccountContext(repoId!),
    enabled: isTauri() && repoId != null,
  });
}

/** Every account query (lists, identities, rules, per-repo contexts) after a change. */
export function invalidateAccounts() {
  void queryClient.invalidateQueries({ queryKey: accountKeys.all });
}
