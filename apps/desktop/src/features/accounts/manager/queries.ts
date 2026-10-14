import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { accountKeys, invalidateAccounts } from "../queries";

/** Extra keys under `accountKeys.all`, so `invalidateAccounts()` refreshes them too. */
export const managerKeys = {
  health: ["accounts", "health"] as const,
  colors: ["accounts", "colors"] as const,
  authConfig: (providerId: string) => ["accounts", "authConfig", providerId] as const,
  emails: (accountId: string) => ["accounts", "emails", accountId] as const,
};

/** Token health of every account; re-checked every few minutes (background refresh). */
export function useAccountHealth() {
  return useQuery({
    queryKey: managerKeys.health,
    queryFn: () => commands.accountHealthAll(),
    enabled: isTauri(),
    refetchInterval: 5 * 60_000,
  });
}

export function useAccountColors() {
  return useQuery({
    queryKey: managerKeys.colors,
    queryFn: () => commands.accountColors(),
    enabled: isTauri(),
    staleTime: Infinity,
  });
}

export function useAuthConfig(providerId: string | null) {
  return useQuery({
    queryKey: managerKeys.authConfig(providerId ?? ""),
    queryFn: () => commands.authConfig(providerId!),
    enabled: isTauri() && providerId !== null,
  });
}

/** Provider emails of an account (identity suggestions). Missing scope → empty list. */
export function useAccountEmails(accountId: string | null) {
  return useQuery({
    queryKey: managerKeys.emails(accountId ?? ""),
    queryFn: () => commands.accountEmails(accountId!),
    enabled: isTauri() && accountId !== null,
    staleTime: 10 * 60_000,
    retry: false,
  });
}

export function useAccountSettings() {
  return useQuery({
    queryKey: accountKeys.settings,
    queryFn: () => commands.accountSettingsGet(),
    enabled: isTauri(),
  });
}

/** After any account change: lists, health, identities, rules, repo contexts. */
export function refreshAccounts() {
  invalidateAccounts();
}

export function refreshHealth() {
  void queryClient.invalidateQueries({ queryKey: managerKeys.health });
}
