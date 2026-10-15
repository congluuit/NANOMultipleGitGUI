/**
 * Accounts signed in with the gh / glab CLI (PLAN 3B.1).
 *
 * - `CliAccountsPanel` (`{ onAdded?, compact? }`): list + checkboxes + "Add to app" +
 *   "Scan again"; self-contained (own query / mutation). Used in the account manager and the
 *   import wizard; the onboarding "Accounts" step can embed it as is.
 * - `CliSyncNoticeHost`: startup notice, mounted once in `App.tsx`.
 * - `authMethodText`: account-card label ("Imported from gh CLI").
 */
export { CliAccountsPanel, type CliAccountsPanelProps } from "./CliAccountsPanel";
export { CliSyncNoticeHost } from "./CliSyncNotice";
export { authMethodText } from "./labels";
export { cliSyncKeys, rescanCliAccounts, useCliAccountsScan } from "./queries";
