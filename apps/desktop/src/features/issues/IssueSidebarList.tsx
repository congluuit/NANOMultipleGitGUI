import { useInfiniteQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { CircleCheck, CircleDot, Filter, LoaderCircle, Plus, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { accountLabel } from "@/features/accounts/switcher";
import { commands, type IssueStateFilter } from "@/ipc";
import { cn } from "@/lib/utils";
import { useHostingNav } from "@/stores/hosting";
import { useAccountById } from "./accountHooks";
import { problemActions } from "./actions";
import { IssueContextHint } from "./context";
import { useIssueContext } from "./useIssueContext";
import { describeIssueError } from "./errors";
import { fold } from "./model";
import { issueKeys, toQuery, useIssueLabels } from "./queries";
import { useIssueFilter, useIssueUi } from "./store";

const PER_PAGE = 30;
const STATES: IssueStateFilter[] = ["open", "closed", "all"];

/** "Issues" section of the sidebar: issues of the repository's main GitHub / GitLab remote. */
export function IssueSidebarList({
  repoId,
  filter: typed = "",
}: {
  repoId: number;
  filter?: string;
}) {
  const { t, i18n } = useTranslation();
  const ctx = useIssueContext(repoId);
  const filter = useIssueFilter(repoId);
  const setFilter = useIssueUi((s) => s.setFilter);
  const openCreate = useIssueUi((s) => s.openCreate);
  const current = useHostingNav((s) => s.issue);
  const openIssue = useHostingNav((s) => s.openIssue);
  const ready = ctx.status === "ready";
  const accountId = ready ? ctx.accountId : null;
  const fullName = ready ? ctx.remote.fullName : null;
  const account = useAccountById(accountId);
  const labels = useIssueLabels(accountId, fullName);
  const query = toQuery(filter, 1, PER_PAGE);
  const list = useInfiniteQuery({
    queryKey: issueKeys.list(repoId, query),
    queryFn: ({ pageParam }) => commands.issueList(repoId, null, { ...query, page: pageParam }),
    initialPageParam: 1,
    getNextPageParam: (last) => last.page.nextPage ?? undefined,
    enabled: isTauri() && ready,
    staleTime: 60_000,
    retry: false,
  });

  if (!ready) return <IssueContextHint state={ctx} compact />;

  const items = (list.data?.pages ?? []).flatMap((p) => p.page.items);
  const needle = fold(typed.trim());
  const shown = needle
    ? items.filter((i) => fold(`#${i.number} ${i.title}`).includes(needle))
    : items;
  const problem = list.isError
    ? describeIssueError(t, list.error, account ? accountLabel(account) : "", i18n.language)
    : null;
  const me = account?.username ?? null;
  const filtered = filter.label || filter.assignee || filter.author;

  return (
    <>
      <li className="flex h-6 items-center gap-0.5 pr-1 pl-6 text-[11px]">
        {STATES.map((s) => (
          <button
            key={`state-${s}`}
            type="button"
            aria-pressed={filter.state === s}
            onClick={() => setFilter(repoId, { state: s })}
            className={cn(
              "rounded px-1.5 py-0.5 text-muted-foreground hover:bg-surface-2 hover:text-foreground",
              filter.state === s && "bg-surface-2 font-medium text-foreground",
            )}
          >
            {t(`issues.state.${s}`)}
          </button>
        ))}
        <span className="flex-1" />
        <DropdownMenu>
          <DropdownMenuTrigger
            aria-label={t("issues.filter.title")}
            title={t("issues.filter.title")}
            className={cn(
              "rounded p-1 text-muted-foreground hover:bg-surface-2 hover:text-foreground",
              filtered && "text-primary",
            )}
          >
            <Filter className="size-3" />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="max-h-80 w-56 overflow-y-auto text-xs">
            <DropdownMenuLabel className="text-[11px] text-muted-foreground">
              {t("issues.filter.people")}
            </DropdownMenuLabel>
            {me && (
              <DropdownMenuCheckboxItem
                checked={filter.assignee === me}
                onCheckedChange={(on) => setFilter(repoId, { assignee: on ? me : null })}
              >
                {t("issues.filter.assignedToMe")}
              </DropdownMenuCheckboxItem>
            )}
            <DropdownMenuCheckboxItem
              checked={filter.assignee === "@none"}
              onCheckedChange={(on) => setFilter(repoId, { assignee: on ? "@none" : null })}
            >
              {t("issues.filter.unassigned")}
            </DropdownMenuCheckboxItem>
            {me && (
              <DropdownMenuCheckboxItem
                checked={filter.author === me}
                onCheckedChange={(on) => setFilter(repoId, { author: on ? me : null })}
              >
                {t("issues.filter.createdByMe")}
              </DropdownMenuCheckboxItem>
            )}
            <DropdownMenuSeparator />
            <DropdownMenuLabel className="text-[11px] text-muted-foreground">
              {t("issues.filter.label")}
            </DropdownMenuLabel>
            {(labels.data ?? []).length === 0 && (
              <DropdownMenuItem disabled>{t("issues.filter.noLabels")}</DropdownMenuItem>
            )}
            {(labels.data ?? []).map((l) => (
              <DropdownMenuCheckboxItem
                key={`label-${l.name}`}
                checked={filter.label === l.name}
                onCheckedChange={(on) => setFilter(repoId, { label: on ? l.name : null })}
              >
                <LabelDot color={l.color} />
                <span className="truncate">{l.name}</span>
              </DropdownMenuCheckboxItem>
            ))}
            {filtered && (
              <>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  onSelect={() => setFilter(repoId, { label: null, assignee: null, author: null })}
                >
                  {t("issues.filter.clear")}
                </DropdownMenuItem>
              </>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
        <button
          type="button"
          aria-label={t("issues.refresh")}
          title={t("issues.refresh")}
          onClick={() => void list.refetch()}
          className="rounded p-1 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
        >
          <RefreshCw className={cn("size-3", list.isFetching && "animate-spin")} />
        </button>
        <button
          type="button"
          aria-label={t("issues.create.open")}
          title={t("issues.create.open")}
          onClick={() => openCreate(repoId)}
          className="rounded p-1 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
        >
          <Plus className="size-3" />
        </button>
      </li>
      {list.isPending && (
        <li className="flex items-center gap-1.5 py-1 pl-8 text-[11px] text-muted-foreground">
          <LoaderCircle className="size-3 animate-spin" />
          {t("issues.loading")}
        </li>
      )}
      {problem && !problem.cancelled && (
        <li className="py-1 pr-2 pl-8 text-[11px]">
          <p className="text-danger">{problem.message}</p>
          {problem.hint && <p className="text-muted-foreground">{problem.hint}</p>}
          {problemActions(problem, ctx.accountId).map((a) => (
            <button
              key={`fix-${a.label}`}
              type="button"
              onClick={a.run}
              className="mr-2 text-primary underline-offset-2 hover:underline"
            >
              {a.label}
            </button>
          ))}
        </li>
      )}
      {list.isSuccess && shown.length === 0 && (
        <li className="py-1 pr-2 pl-8 text-[11px] text-muted-foreground italic">
          {t(filter.state === "open" && !filtered && !needle ? "issues.noOpen" : "issues.none")}
        </li>
      )}
      {shown.map((i) => {
        const active =
          current?.number === i.number &&
          current.fullName === ctx.remote.fullName &&
          current.accountId === ctx.accountId;
        const Icon = i.state === "open" ? CircleDot : CircleCheck;
        return (
          <li key={`issue-${i.number}`} title={i.title}>
            <button
              type="button"
              onClick={() =>
                openIssue({
                  accountId: ctx.accountId,
                  fullName: ctx.remote.fullName,
                  number: i.number,
                  repoId,
                })
              }
              className={cn(
                "flex h-6 w-full items-center gap-1.5 truncate pr-2 pl-8 text-left text-xs hover:bg-surface-2",
                active && "bg-surface-2 font-semibold",
              )}
            >
              <Icon
                className={cn(
                  "size-3.5 shrink-0",
                  i.state === "open" ? "text-success" : "text-muted-foreground",
                )}
              />
              <span className="shrink-0 text-muted-foreground tabular-nums">#{i.number}</span>
              <span className="truncate">{i.title}</span>
            </button>
          </li>
        );
      })}
      {list.hasNextPage && (
        <li>
          <button
            type="button"
            disabled={list.isFetchingNextPage}
            onClick={() => void list.fetchNextPage()}
            className="flex h-6 w-full items-center gap-1.5 pl-8 text-[11px] text-primary hover:bg-surface-2 disabled:opacity-60"
          >
            {list.isFetchingNextPage && <LoaderCircle className="size-3 animate-spin" />}
            {t("issues.loadMore")}
          </button>
        </li>
      )}
    </>
  );
}

/** Label color chip; the color is validated by the backend (`#rrggbb` or absent). */
export function LabelDot({ color }: { color: string | null }) {
  return (
    <span
      aria-hidden
      className="size-2 shrink-0 rounded-full border"
      style={color ? { background: color, borderColor: color } : undefined}
    />
  );
}
