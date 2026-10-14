import { useTranslation } from "react-i18next";
import type { Account, Identity, Provider, RepoAccountContext } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useActiveRepo } from "@/stores/workspace";
import {
  accountKeys,
  useAccounts,
  useIdentities,
  useProviders,
  useRepoAccountContext,
} from "../queries";
import {
  describeRepoAccount,
  providerNameOf,
  ruleSummary,
  sortAccounts,
  toAccountSummary,
  type AccountSummary,
  type RepoAccount,
} from "./model";

/**
 * Account context of a repository with display fields: `{ account, identity, source, color,
 * label, providerName, transport, … }`. `repoId` `null` (or a missing folder) → unbound.
 */
export function useRepoAccount(repoId: number | null | undefined): RepoAccount & {
  loading: boolean;
} {
  const ctx = useRepoAccountContext(repoId);
  const providers = useProviders().data;
  return {
    ...describeRepoAccount(repoId == null ? null : ctx.data, providers),
    loading: repoId != null && ctx.isPending && ctx.fetchStatus !== "idle",
  };
}

/** Account of the active repository (`null` without a usable repo). */
export function useActiveRepoAccount() {
  const repo = useActiveRepo();
  return useRepoAccount(repo && !repo.missing ? repo.id : null);
}

/** Accounts in switcher order (`Ctrl/Cmd+Alt+1..9`). */
export function useSortedAccounts(): Account[] {
  return sortAccounts(useAccounts().data ?? []);
}

/** Identities of every account. */
export function useAllIdentities(): Identity[] {
  return useIdentities(null).data ?? [];
}

/** `providerId → "GitLab acme"` (`null` while providers load). */
export function useProviderName() {
  const providers = useProviders().data;
  return (providerId: string) => providerNameOf({ providerId }, providers);
}

/** "theo rule github.com/acme/*", "đã khóa", "từ lúc clone"… */
export function useSourceText() {
  const { t } = useTranslation();
  return (r: Pick<RepoAccount, "source" | "matchedRule">) =>
    r.source === "rule"
      ? t("switcher.source.rule", { rule: r.matchedRule ? ruleSummary(r.matchedRule) : "" })
      : t(`switcher.source.${r.source}`);
}

// ── Flat account summaries (status bar, transfers) ─────────────────────────────────────────

/**
 * The active repo's account as an {@link AccountSummary} (`label`, `color`, `providerName`,
 * `identity`, `transport`) or `null`.
 */
export function useActiveAccount(): AccountSummary | null {
  return toAccountSummary(useActiveRepoAccount());
}

/** {@link AccountSummary} of a repo's account (`null` when unbound). */
export function useRepoAccountSummary(repoId: number | null | undefined): AccountSummary | null {
  return toAccountSummary(useRepoAccount(repoId));
}

/**
 * Non-hook variant for event handlers and plain modules: reads the query cache, so it is `null`
 * until some component has loaded the repo's context (the account pill does).
 */
export function repoAccountSummary(repoId: number): AccountSummary | null {
  return toAccountSummary(cachedRepoAccount(repoId));
}

/** {@link RepoAccount} from the query cache (non-hook). */
export function cachedRepoAccount(repoId: number): RepoAccount {
  return describeRepoAccount(
    queryClient.getQueryData<RepoAccountContext>(accountKeys.repoContext(repoId)),
    queryClient.getQueryData<Provider[]>(accountKeys.providers),
  );
}

/** Accounts (switcher order) and identities from the query cache (non-hook). */
export function cachedAccounts(): { accounts: Account[]; identities: Identity[] } {
  return {
    accounts: sortAccounts(queryClient.getQueryData<Account[]>(accountKeys.list) ?? []),
    identities: queryClient.getQueryData<Identity[]>(accountKeys.identities(null)) ?? [],
  };
}
