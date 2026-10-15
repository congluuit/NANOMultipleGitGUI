/** Palette commands of step 4.2: "New issue…", "Open issue #…". */
import { CircleDot, CirclePlus } from "lucide-react";
import i18n from "@/i18n";
import {
  registerCommands,
  type PaletteCommand,
  type PaletteContext,
} from "@/features/command-palette/registry";
import { commands } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useHostingNav } from "@/stores/hosting";
import { primaryRemote } from "./model";
import { issueKeys } from "./queries";
import { useIssueUi } from "./store";

const ISSUE_NUMBER = /^#?\s*(\d{1,7})$/;

/** Opens issue `#n` of the repository's main remote; throws a readable error when it cannot. */
export async function openIssueNumber(repoId: number, value: string) {
  const t = i18n.t.bind(i18n);
  const m = ISSUE_NUMBER.exec(value.trim());
  if (!m) throw new Error(t("issues.cmd.openInvalid"));
  const remotes = await queryClient.fetchQuery({
    queryKey: issueKeys.remotes(repoId),
    queryFn: () => commands.hostedRemoteList(repoId),
    staleTime: 30_000,
  });
  const remote = primaryRemote(remotes);
  if (!remote) throw new Error(t("issues.context.noRemote"));
  if (!remote.accountId) throw new Error(t("issues.context.unbound", { remote: remote.remote }));
  if (!remote.hasToken) throw new Error(t("issues.context.noToken", { remote: remote.remote }));
  useHostingNav.getState().openIssue({
    accountId: remote.accountId,
    fullName: remote.fullName,
    number: Number(m[1]),
    repoId,
  });
}

export function issueCommands(c: PaletteContext): PaletteCommand[] {
  const { t, repo } = c;
  if (!repo) return [];
  const id = repo.id;
  const group = t("issues.cmd.group");
  return [
    {
      id: "issues.create",
      title: t("issues.cmd.create"),
      group,
      icon: CirclePlus,
      keywords: ["issue", "new issue", "bug", "tao issue", "bao loi"],
      run: () => useIssueUi.getState().openCreate(id),
    },
    {
      id: "issues.open",
      title: t("issues.cmd.open"),
      group,
      icon: CircleDot,
      keywords: ["issue", "open issue", "mo issue", "#"],
      input: {
        placeholder: t("issues.cmd.openPlaceholder"),
        validate: (v) => (ISSUE_NUMBER.test(v.trim()) ? null : t("issues.cmd.openInvalid")),
        run: (v) => openIssueNumber(id, v),
      },
    },
  ];
}

registerCommands("issues", issueCommands);
