import type { TFunction } from "i18next";
import type { Account, ProviderKind } from "@/ipc";
import { cliToolOf } from "./model";

/** How the account signed in, for account cards ("Imported from gh CLI" for CLI tokens). */
export function authMethodText(
  t: TFunction,
  account: Pick<Account, "authMethod">,
  providerKind: ProviderKind | undefined,
): string {
  if (account.authMethod === "importedCli")
    return t("cliSync.source", { tool: cliToolOf(providerKind) });
  return t(`accounts.authMethod.${account.authMethod}`);
}
