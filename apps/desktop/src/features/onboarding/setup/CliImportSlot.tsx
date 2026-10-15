import { CliAccountsPanel } from "@/features/accounts/cli-sync";

/** Accounts already signed in with gh / glab, added with their token on request (3B.1). */
export function CliImportSlot() {
  return <CliAccountsPanel compact />;
}
