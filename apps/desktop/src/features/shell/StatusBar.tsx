import { GitBranch, GitCommitHorizontal } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useRepoRefs } from "@/features/repos/queries";
import { headState, primaryRemote, transportOf } from "@/features/repos/refsView";
import { useActiveAccount } from "@/features/accounts/switcher";
import { TransferQueueWidget } from "@/features/transfers/TransferQueueWidget";
import { HeadCiIndicator } from "@/features/ci/CommitCi";
import { useActiveRepo } from "@/stores/workspace";

export function StatusBar() {
  const { t } = useTranslation();
  const repo = useActiveRepo();
  const account = useActiveAccount();
  const refs = useRepoRefs(repo?.id ?? null, !repo?.missing).data;

  const head = refs ? headState(refs.head) : null;
  const current = refs?.local.find((b) => b.isHead);
  const remote = refs ? primaryRemote(refs) : null;
  const transport = transportOf(remote?.url);

  return (
    // Narrow windows: the path truncates first, then the remote and the account details hide.
    <footer className="flex h-6 shrink-0 items-center gap-3 overflow-hidden border-t bg-surface-2 px-3 text-[11px] whitespace-nowrap text-muted-foreground">
      {repo && (
        <>
          <span className="flex min-w-0 shrink-0 items-center gap-1.5">
            <span
              className="size-2 rounded-full"
              style={{ background: account?.color ?? "var(--warning)" }}
            />
            {account ? (
              <span className="font-medium text-foreground" data-testid="status-account">
                {account.label}
                <span className="font-normal text-muted-foreground max-md:hidden">
                  {[account.providerName, account.transport?.toUpperCase()]
                    .filter(Boolean)
                    .map((part) => ` · ${part}`)
                    .join("")}
                </span>
              </span>
            ) : (
              <span className="text-warning">{t("account.unbound")}</span>
            )}
          </span>
          {refs && (
            <>
              <span className="h-3 w-px bg-border max-lg:hidden" />
              <span className="max-lg:hidden" title={remote?.url ?? undefined}>
                {remote ? `${remote.name} · ${transport}` : t("status.noRemote")}
              </span>
              <span className="h-3 w-px shrink-0 bg-border" />
              {head?.kind === "detached" ? (
                <span className="flex shrink-0 items-center gap-1 text-warning">
                  <GitCommitHorizontal className="size-3" />
                  {t("status.detached", { oid: head.oid })}
                </span>
              ) : (
                <span className="flex min-w-0 shrink items-center gap-1 truncate">
                  <GitBranch className="size-3" />
                  {head?.kind === "branch" || head?.kind === "unborn" ? head.branch : ""}
                  {head?.kind === "unborn" && ` (${t("status.unborn")})`}
                  {current?.upstream && ` ↑${current.ahead} ↓${current.behind}`}
                </span>
              )}
              <HeadCiIndicator />
            </>
          )}
        </>
      )}
      <span className="flex-1" />
      <TransferQueueWidget />
      {repo && (
        <span className="min-w-0 truncate" title={repo.path}>
          {repo.path}
        </span>
      )}
    </footer>
  );
}
