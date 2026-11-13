import i18n from "i18next";
import { accountKeys } from "@/features/accounts/queries";
import { remoteAccountName, remoteWithHost } from "@/features/accounts/switcher/tracking";
import { repoKeys } from "@/features/repos/queries";
import type { RepoAccountContext, RepoRefs } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import type { Notice } from "@/stores/ui";
import { useWorkspace } from "@/stores/workspace";
import { actionsContext, helpActions } from "./actions";
import type { HelpRequest } from "./store";

/** `https://user:secret@host` and long token-looking words never leave the app. */
export function scrub(text: string): string {
  return text
    .replace(/(https?:\/\/)[^/\s@]+@/gi, "$1***@")
    .replace(/\b(gh[pousr]_|github_pat_|glpat-|sk-|sk-ant-)[A-Za-z0-9_-]{8,}/g, "***")
    .replace(/\b[A-Fa-f0-9]{40,}\b/g, (m) => (m.length === 40 ? m : "***"));
}

const cut = (text: string, max: number) =>
  text.length > max ? `${text.slice(0, max)}\n[...]` : text;

/**
 * The facts sent to the AI about an error notice: what failed, the app's explanation, git's
 * output and the state of the repository (branch, remotes and their accounts). Never a token.
 */
export function helpRequest(notice: Notice): HelpRequest {
  const repoId = notice.repoId;
  const tab = useWorkspace.getState().tabs.find((x) => x.id === repoId);
  const lines: string[] = [`Error: ${notice.message}`];
  if (notice.reason) lines.push(`Error kind (app): ${notice.reason}`);
  if (notice.hint) lines.push(`App's explanation: ${notice.hint}`);
  const actions = helpActions(notice, [askAiLabel()]);
  if (repoId !== undefined) {
    const refs = queryClient.getQueryData<RepoRefs>(repoKeys.refs(repoId));
    const ctx = queryClient.getQueryData<RepoAccountContext>(accountKeys.repoContext(repoId));
    if (tab) lines.push(`Repository: ${tab.name}`);
    if (refs) {
      const head = refs.head;
      const branch = refs.local.find((b) => b.name === head.branch);
      lines.push(
        head.detached
          ? `HEAD: detached at ${head.oid?.slice(0, 7) ?? "?"}`
          : `Current branch: ${head.branch ?? "?"}${head.unborn ? " (no commit yet)" : ""}`,
      );
      if (branch) {
        lines.push(
          branch.upstream
            ? `Tracks: ${branch.upstream} (ahead ${branch.ahead}, behind ${branch.behind}${branch.upstreamGone ? ", gone on the server" : ""})`
            : "Tracks: nothing (not pushed yet)",
        );
      }
    }
    for (const r of ctx?.perRemote ?? []) {
      lines.push(
        `Remote ${remoteWithHost(r)} over ${r.transport}: account ${remoteAccountName(r) ?? "none"}`,
      );
    }
  }
  lines.push(`App language: ${i18n.language}`, `System: ${navigator.platform || "?"}`);
  if (actions.length) lines.push("", actionsContext(actions));
  if (notice.detail) lines.push("", "Git output:", cut(notice.detail.trim(), 6000));
  return {
    title: notice.message,
    context: scrub(lines.join("\n")),
    repoName: tab?.name ?? null,
    repoPath: tab?.path ?? "",
    actions,
  };
}

export const askAiLabel = () => i18n.t("aiHelp.ask");
