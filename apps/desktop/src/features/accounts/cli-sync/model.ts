/** Pure helpers of the gh / glab account sync (PLAN 3B.1). */
import type {
  Account,
  CliAccountEntry,
  CliAccountRef,
  CliAccountsScan,
  CliImportItem,
  CliImportStatus,
  CliTool,
  ProviderKind,
} from "@/ipc";

/** Accounts not in the app yet. */
export function newAccounts(scan: CliAccountsScan | undefined): CliAccountEntry[] {
  return scan?.accounts.filter((a) => a.existingAccountId === null) ?? [];
}

export function toolInstalled(scan: CliAccountsScan | undefined, tool: CliTool): boolean {
  return scan?.tools.find((t) => t.tool === tool)?.installed ?? false;
}

/** New accounts whose CLI is installed (the token can be read): ticked by default. */
export function defaultSelection(scan: CliAccountsScan | undefined): string[] {
  return newAccounts(scan)
    .filter((a) => toolInstalled(scan, a.tool))
    .map((a) => a.key);
}

export function toRef(a: CliAccountEntry): CliAccountRef {
  return { tool: a.tool, host: a.host, username: a.username };
}

/** The account now exists in the app (usable from the pill / switcher). */
export function isAdded(status: CliImportStatus): boolean {
  return status === "added" || status === "missingScopes";
}

export type ResultTone = "ok" | "warn" | "danger" | "muted";

export function resultTone(status: CliImportStatus): ResultTone {
  switch (status) {
    case "added":
      return "ok";
    case "missingScopes":
    case "noToken":
    case "cliMissing":
      return "warn";
    case "alreadyExists":
    case "notFound":
      return "muted";
    default:
      return "danger";
  }
}

/** The CLI command that signs the account in again (shown in fix hints). */
export function loginCommand(tool: CliTool, host: string): string {
  return tool === "gh" ? `gh auth login -h ${host}` : `glab auth login --hostname ${host}`;
}

/** The command that shows which account the CLI uses. */
export function statusCommand(tool: CliTool): string {
  return tool === "gh" ? "gh auth status" : "glab auth status";
}

/** Command to copy for a result, when typing one in a terminal fixes it. */
export function fixCommandOf(item: CliImportItem): string | null {
  switch (item.status) {
    case "missingScopes":
      return item.fixCommand;
    case "invalidToken":
    case "noToken":
      return loginCommand(item.tool, item.host);
    case "wrongUser":
      return statusCommand(item.tool);
    default:
      return null;
  }
}

/** Where an imported account's token came from, by provider family. */
export function cliToolOf(kind: ProviderKind | undefined): CliTool {
  return kind === "gitlab" || kind === "gitlabSelfHosted" ? "glab" : "gh";
}

export function isImported(account: Pick<Account, "authMethod">): boolean {
  return account.authMethod === "importedCli";
}

/** Keys of the new accounts the startup notice is about. */
export function noticeKeys(scan: CliAccountsScan | undefined): string[] {
  return newAccounts(scan).map((a) => a.key);
}
