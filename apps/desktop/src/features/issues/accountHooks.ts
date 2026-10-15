import { useProviders } from "@/features/accounts/queries";
import { accountLabel, useSortedAccounts } from "@/features/accounts/switcher";
import type { Account, ProviderFamily } from "@/ipc";

/** Account by id from the account list (`null` while loading / unknown). */
export function useAccountById(accountId: string | null | undefined): Account | null {
  const accounts = useSortedAccounts();
  return accounts.find((a) => a.id === accountId) ?? null;
}

/** GitHub or GitLab, from the account's provider (`null` while loading). */
export function useAccountFamily(accountId: string | null | undefined): ProviderFamily | null {
  const account = useAccountById(accountId);
  const providers = useProviders().data;
  const kind = providers?.find((p) => p.id === account?.providerId)?.kind;
  if (!kind) return null;
  return kind.startsWith("gitlab") ? "gitlab" : "github";
}

/** Display name of an account id ("Công ty", "hoangnv"…), or the id itself while loading. */
export function useAccountName(accountId: string | null | undefined): string {
  const account = useAccountById(accountId);
  return account ? accountLabel(account) : (accountId ?? "");
}
