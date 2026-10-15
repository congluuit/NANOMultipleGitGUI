import { useState } from "react";
import { useQueries } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import {
  CircleDot,
  GitPullRequest,
  GitPullRequestDraft,
  LoaderCircle,
  RefreshCw,
  Square,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { openExternal } from "@/features/accounts/manager/openExternal";
import { useProviders } from "@/features/accounts/queries";
import { accountLabel, useSortedAccounts } from "@/features/accounts/switcher";
import { relativeTime } from "@/features/clone/model";
import { problemActions } from "@/features/issues/actions";
import { describeIssueError } from "@/features/issues/errors";
import { hostName } from "@/features/issues/model";
import { MyWorkTabs } from "@/features/notes/MyWorkTabs";
import { NotesPanel } from "@/features/notes/NotesPanel";
import { useNotesUi } from "@/features/notes/store";
import { issueKeys } from "@/features/issues/queries";
import { commands, type Account, type WorkAccount } from "@/ipc";
import { cn } from "@/lib/utils";
import { useHostingNav } from "@/stores/hosting";
import { useWorkspace } from "@/stores/workspace";
import { findOpenRepo, groupRows, groupTotal, GROUPS, type WorkGroup, type WorkRow } from "./model";
import { cancelMyWork, refreshMyWork, useMyWork } from "./queries";
import { useMyWorkUi } from "./store";

/** Mounted once (App): keeps the "My work" data fresh for the badge and hosts the panel. */
export function MyWorkHost() {
  const open = useMyWorkUi((s) => s.open);
  const setOpen = useMyWorkUi((s) => s.setOpen);
  useMyWork();
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      {open && <MyWorkPanel />}
    </Dialog>
  );
}

function Dot({ account }: { account: Account | undefined }) {
  return (
    <span
      aria-hidden
      className="size-2 shrink-0 rounded-full"
      style={{ background: account?.color ?? "var(--muted-foreground)" }}
    />
  );
}

/** Open repository tabs with their hosted remotes (to open items in the right tab). */
function useOpenTabRemotes() {
  const tabs = useWorkspace((s) => s.tabs);
  const usable = tabs.filter((tab) => !tab.missing);
  const results = useQueries({
    queries: usable.map((tab) => ({
      queryKey: issueKeys.remotes(tab.id),
      queryFn: () => commands.hostedRemoteList(tab.id),
      enabled: isTauri(),
      staleTime: 30_000,
    })),
  });
  return usable.map((tab, i) => ({ id: tab.id, remotes: results[i]?.data }));
}

function MyWorkPanel() {
  const { t, i18n } = useTranslation();
  const work = useMyWork();
  const accounts = useSortedAccounts();
  const providers = useProviders().data ?? [];
  const filter = useMyWorkUi((s) => s.accountId);
  const setFilter = useMyWorkUi((s) => s.setAccount);
  const setOpen = useMyWorkUi((s) => s.setOpen);
  const openManager = useAccountDialogs((s) => s.openManager);
  const tabs = useOpenTabRemotes();
  const activeId = useWorkspace((s) => s.activeId);
  const data = work.data;
  const byId = new Map(accounts.map((a) => [a.id, a]));
  const name = (id: string) => {
    const a = byId.get(id);
    return a ? accountLabel(a) : id;
  };
  const failed = (data?.accounts ?? []).filter((a) => a.error);
  const listed = (data?.accounts ?? []).filter((a) => a.lists);
  const [now] = useState(() => Date.now());
  // Phase 6.5: "My notes" tab.
  const tab = useNotesUi((s) => s.tab);

  const openRow = (row: WorkRow) => {
    const account = byId.get(row.accountId);
    const provider = providers.find((p) => p.id === account?.providerId);
    const host = provider ? hostName(provider.baseUrl) : "";
    const repoId = host ? findOpenRepo(tabs, activeId, host, row.item.fullName) : null;
    if (repoId === null && tabs.length === 0) {
      // No repository tab to show the view in: the browser is the only place.
      if (row.item.webUrl.startsWith("https://")) void openExternal(row.item.webUrl);
      return;
    }
    if (repoId !== null && repoId !== activeId) useWorkspace.getState().setActive(repoId);
    const ref = {
      accountId: row.accountId,
      fullName: row.item.fullName,
      number: row.item.number,
      repoId,
    };
    const nav = useHostingNav.getState();
    if (row.item.kind === "pullRequest") nav.openPullRequest(ref);
    else nav.openIssue(ref);
    setOpen(false);
  };

  return (
    <DialogContent
      className={cn(
        "flex h-[min(44rem,calc(100dvh-4rem))] flex-col gap-3",
        tab === "notes" ? "sm:max-w-5xl" : "sm:max-w-3xl",
      )}
    >
      <DialogHeader>
        <DialogTitle>{t("mywork.title")}</DialogTitle>
        <DialogDescription>
          {tab === "notes" ? t("notes.description") : t("mywork.description")}
        </DialogDescription>
      </DialogHeader>
      <MyWorkTabs />
      {tab === "notes" ? (
        <NotesPanel onLeave={() => setOpen(false)} />
      ) : (
        <>
          <div className="flex flex-wrap items-center gap-1 text-xs">
            <FilterChip active={filter === null} onClick={() => setFilter(null)}>
              {t("mywork.allAccounts")}
            </FilterChip>
            {(data?.accounts ?? []).map((a) => (
              <FilterChip
                key={`f-${a.accountId}`}
                active={filter === a.accountId}
                onClick={() => setFilter(filter === a.accountId ? null : a.accountId)}
              >
                <Dot account={byId.get(a.accountId)} />
                {name(a.accountId)}
              </FilterChip>
            ))}
            <span className="flex-1" />
            {work.isFetching ? (
              <Button size="xs" variant="ghost" onClick={() => void cancelMyWork()}>
                <Square />
                {t("mywork.stop")}
              </Button>
            ) : null}
            <Button
              size="xs"
              variant="outline"
              disabled={work.isFetching}
              onClick={() => void refreshMyWork().catch(() => undefined)}
            >
              <RefreshCw className={cn(work.isFetching && "animate-spin")} />
              {t("mywork.refresh")}
            </Button>
          </div>
          <ScrollArea className="min-h-0 flex-1 rounded-md border">
            <div className="space-y-4 p-3 text-xs">
              {work.isPending && (
                <p className="flex items-center gap-2 text-muted-foreground">
                  <LoaderCircle className="size-4 animate-spin" />
                  {t("mywork.loading")}
                </p>
              )}
              {work.isError && <p className="text-danger">{String(work.error)}</p>}
              {data && data.accounts.length === 0 && (
                <div className="space-y-1 text-muted-foreground">
                  <p>{t(accounts.length ? "mywork.noTokens" : "mywork.noAccounts")}</p>
                  <Button size="xs" variant="outline" onClick={() => openManager()}>
                    {t("mywork.manageAccounts")}
                  </Button>
                </div>
              )}
              {failed
                .filter((a) => !filter || a.accountId === filter)
                .map((a) => (
                  <AccountProblem
                    key={`e-${a.accountId}`}
                    work={a}
                    account={byId.get(a.accountId)}
                  />
                ))}
              {data && data.withoutToken.length > 0 && !filter && (
                <p className="text-muted-foreground">
                  {t("mywork.withoutToken", {
                    count: data.withoutToken.length,
                    names: data.withoutToken.map(name).join(", "),
                  })}{" "}
                  <button
                    type="button"
                    className="text-primary underline-offset-2 hover:underline"
                    onClick={() => openManager({ accountId: data.withoutToken[0] ?? null })}
                  >
                    {t("mywork.manageAccounts")}
                  </button>
                </p>
              )}
              {data &&
                listed.length > 0 &&
                GROUPS.map((g) => (
                  <Group
                    key={`g-${g}`}
                    group={g}
                    rows={groupRows(data.accounts, g, filter)}
                    total={groupTotal(data.accounts, g, filter)}
                    render={(row) => (
                      <Row
                        key={`r-${g}-${row.accountId}-${row.item.fullName}-${row.item.number}`}
                        row={row}
                        account={byId.get(row.accountId)}
                        accountName={name(row.accountId)}
                        when={
                          row.item.updatedAt != null
                            ? relativeTime(row.item.updatedAt, now, i18n.language)
                            : ""
                        }
                        onOpen={() => openRow(row)}
                      />
                    )}
                  />
                ))}
            </div>
          </ScrollArea>
        </>
      )}
    </DialogContent>
  );
}

function FilterChip({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={cn(
        "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 hover:bg-surface-2",
        active && "border-primary bg-surface-2 font-medium",
      )}
    >
      {children}
    </button>
  );
}

function AccountProblem({ work, account }: { work: WorkAccount; account: Account | undefined }) {
  const { t, i18n } = useTranslation();
  const label = account ? accountLabel(account) : work.accountId;
  const problem = describeIssueError(t, work.error, label, i18n.language);
  if (problem.cancelled) return null;
  return (
    <div className="space-y-1 rounded-md border border-danger/40 bg-danger/5 p-2">
      <p className="flex items-center gap-1.5 font-medium">
        <Dot account={account} />
        {label}
      </p>
      <p className="text-danger">{problem.message}</p>
      {problem.hint && <p className="text-muted-foreground">{problem.hint}</p>}
      <div className="flex flex-wrap gap-2">
        {problemActions(problem, work.accountId).map((a) => (
          <Button key={`fix-${a.label}`} size="xs" variant="outline" onClick={a.run}>
            {a.label}
          </Button>
        ))}
      </div>
    </div>
  );
}

function Group({
  group,
  rows,
  total,
  render,
}: {
  group: WorkGroup;
  rows: WorkRow[];
  total: number;
  render: (row: WorkRow) => React.ReactNode;
}) {
  const { t } = useTranslation();
  return (
    <section aria-label={t(`mywork.group.${group}`)}>
      <h3 className="mb-1 flex items-center gap-2 text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">
        {t(`mywork.group.${group}`)}
        <span className="rounded-full bg-surface-2 px-1.5 font-normal text-foreground">
          {total}
        </span>
      </h3>
      {rows.length === 0 ? (
        <p className="pl-1 text-muted-foreground italic">{t(`mywork.empty.${group}`)}</p>
      ) : (
        <ul className="divide-y rounded-md border">{rows.map(render)}</ul>
      )}
      {total > rows.length && rows.length > 0 && (
        <p className="mt-1 pl-1 text-[11px] text-muted-foreground">
          {t("mywork.more", { count: total - rows.length })}
        </p>
      )}
    </section>
  );
}

function Row({
  row,
  account,
  accountName,
  when,
  onOpen,
}: {
  row: WorkRow;
  account: Account | undefined;
  accountName: string;
  when: string;
  onOpen: () => void;
}) {
  const { t } = useTranslation();
  const { item } = row;
  const Icon =
    item.kind === "issue" ? CircleDot : item.draft ? GitPullRequestDraft : GitPullRequest;
  return (
    <li>
      <button
        type="button"
        onClick={onOpen}
        title={item.title}
        className="flex w-full items-center gap-2 px-2 py-1.5 text-left hover:bg-surface-2"
      >
        <Icon
          className={cn(
            "size-3.5 shrink-0",
            item.kind === "issue" || !item.draft ? "text-success" : "text-muted-foreground",
          )}
        />
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="flex min-w-0 items-center gap-1.5">
            <span className="truncate font-medium">{item.title}</span>
            {item.draft && (
              <span className="shrink-0 rounded border px-1 text-[10px] text-muted-foreground">
                {t("mywork.draft")}
              </span>
            )}
          </span>
          <span className="flex min-w-0 items-center gap-1.5 text-[11px] text-muted-foreground">
            <span className="inline-flex shrink-0 items-center gap-1">
              <Dot account={account} />
              {accountName}
            </span>
            <span className="truncate">
              {item.fullName}
              {item.webUrl.includes("/-/merge_requests/") ? "!" : "#"}
              {item.number}
            </span>
            {item.author && <span className="shrink-0">· {item.author}</span>}
          </span>
        </span>
        <span className="shrink-0 text-[11px] text-muted-foreground">{when}</span>
      </button>
    </li>
  );
}
