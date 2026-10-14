/**
 * Account switcher (PLAN §4.3, §4.5): which account a repo uses, binding / one-time override,
 * `Ctrl/Cmd+Alt+1..9`, the "unbound" banner.
 *
 * Flat account info for status bar / transfers: `useActiveAccount()`, `useRepoAccountSummary(id)`
 * and the non-hook `repoAccountSummary(id)` (query cache) return an `AccountSummary`
 * (`label`, `color`, `providerName`, `identity` (nullable), `transport`) or `null` when unbound.
 */
export { bindNthAccount, bindRepo, setAccountOnce, unbindRepo, type BindOptions } from "./actions";
export {
  cachedAccounts,
  cachedRepoAccount,
  repoAccountSummary,
  useActiveAccount,
  useActiveRepoAccount,
  useAllIdentities,
  useProviderName,
  useRepoAccount,
  useRepoAccountSummary,
  useSortedAccounts,
  useSourceText,
} from "./hooks";
export {
  accountLabel,
  accountShortcutHint,
  accountShortcutIndex,
  defaultIdentity,
  describeRepoAccount,
  identitiesOf,
  ruleSummary,
  sortAccounts,
  toAccountSummary,
  type AccountSummary,
  type RepoAccount,
} from "./model";
export {
  asAccountOf,
  clearOneShot,
  oneShotApplies,
  peekOneShot,
  setOneShot,
  settleOneShot,
  takeOneShot,
  useAsAccount,
  useOneShot,
  type OneShot,
  type OneShotOp,
  type TakeOp,
} from "./oneShot";
export { CommitAs } from "./CommitAs";
export { UnboundBanner } from "./UnboundBanner";
export { useAccountShortcuts } from "./useAccountShortcuts";
