import { useState } from "react";
import {
  Filter,
  GitPullRequest,
  GitPullRequestCreate,
  LoaderCircle,
  RefreshCw,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { useSortedAccounts } from "@/features/accounts/switcher";
import type { PullSummary } from "@/ipc";
import { cn } from "@/lib/utils";
import { useHostingNav } from "@/stores/hosting";
import { describePullError } from "./errors";
import { pullNumber, shortTerm } from "./model";
import { DecisionMark } from "./parts";
import { usePullReviews } from "./queries";
import { useSidebarPulls } from "./sidebarData";
import { openCreatePull, usePullUi, type PullFilters } from "./store";

const STATES = ["open", "closed", "merged", "all"] as const;

function Empty({ children }: { children: React.ReactNode }) {
  return <li className="py-1 pr-2 pl-8 text-[11px] text-muted-foreground italic">{children}</li>;
}

function SmallButton({
  label,
  onClick,
  children,
}: {
  label: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className="flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-surface-2 hover:text-foreground"
    >
      {children}
    </button>
  );
}

function FilterPopover({
  repoId,
  filters,
  accountId,
}: {
  repoId: number;
  filters: PullFilters;
  accountId: string | null;
}) {
  const { t } = useTranslation();
  const set = usePullUi((s) => s.setFilters);
  const me = useSortedAccounts().find((a) => a.id === accountId)?.username ?? null;
  const active = !!(filters.author || filters.reviewer || filters.label);
  const field = (key: "author" | "reviewer" | "label", label: string) => (
    <label className="flex flex-col gap-0.5 text-[11px] text-muted-foreground">
      {label}
      <input
        value={filters[key]}
        onChange={(e) => set(repoId, { [key]: e.target.value })}
        className="h-7 rounded-md border bg-background px-2 text-xs text-foreground focus:border-primary focus:outline-none"
      />
    </label>
  );
  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={t("pulls.list.filters")}
          title={t("pulls.list.filters")}
          className={cn(
            "flex size-5 shrink-0 items-center justify-center rounded hover:bg-surface-2 hover:text-foreground",
            active ? "text-primary" : "text-muted-foreground",
          )}
        >
          <Filter className="size-3" />
        </button>
      </PopoverTrigger>
      <PopoverContent side="right" align="start" className="flex w-60 flex-col gap-2 p-3">
        {me && (
          <div className="flex flex-wrap gap-1">
            <button
              type="button"
              onClick={() => set(repoId, { author: me, reviewer: "" })}
              className="rounded border px-1.5 py-0.5 text-[11px] hover:bg-surface-2"
            >
              {t("pulls.list.mine")}
            </button>
            <button
              type="button"
              onClick={() => set(repoId, { reviewer: me, author: "" })}
              className="rounded border px-1.5 py-0.5 text-[11px] hover:bg-surface-2"
            >
              {t("pulls.list.toReview")}
            </button>
          </div>
        )}
        {field("author", t("pulls.list.author"))}
        {field("reviewer", t("pulls.list.reviewer"))}
        {field("label", t("pulls.list.label"))}
        {active && (
          <button
            type="button"
            onClick={() => set(repoId, { author: "", reviewer: "", label: "" })}
            className="self-start text-[11px] text-primary hover:underline"
          >
            {t("pulls.list.clearFilters")}
          </button>
        )}
      </PopoverContent>
    </Popover>
  );
}

function Row({
  item,
  accountId,
  fullName,
  repoId,
}: {
  item: PullSummary;
  accountId: string;
  fullName: string;
  repoId: number;
}) {
  const { t } = useTranslation();
  const open = useHostingNav((s) => s.openPullRequest);
  const selected = useHostingNav(
    (s) =>
      s.pullRequest?.number === item.number &&
      s.pullRequest.fullName === fullName &&
      s.pullRequest.accountId === accountId,
  );
  const ref = { accountId, fullName, number: item.number };
  const reviews = usePullReviews(ref, item.state === "open");
  const branches =
    item.sourceBranch && item.targetBranch ? `${item.sourceBranch} → ${item.targetBranch}` : "";
  const title = [
    `${pullNumber(null, item.number)} ${item.title}`,
    item.author ? t("pulls.list.by", { user: item.author.username }) : "",
    branches,
  ]
    .filter(Boolean)
    .join("\n");
  return (
    <li title={title}>
      <button
        type="button"
        onClick={() => open({ ...ref, repoId })}
        className={cn(
          "flex h-6 w-full items-center gap-1.5 truncate pr-2 pl-8 text-left text-xs hover:bg-surface-2",
          selected && "bg-surface-2 font-semibold",
        )}
      >
        <span className="shrink-0 text-muted-foreground">#{item.number}</span>
        <span className={cn("min-w-0 flex-1 truncate", item.draft && "text-muted-foreground")}>
          {item.title}
        </span>
        {item.draft && (
          <span className="shrink-0 rounded bg-surface-2 px-1 text-[9px] text-muted-foreground">
            {t("pulls.state.draft")}
          </span>
        )}
        {item.state !== "open" && (
          <span className="shrink-0 text-[9px] text-muted-foreground">
            {t(`pulls.state.${item.state}`)}
          </span>
        )}
        {reviews.data && <DecisionMark decision={reviews.data.decision} />}
        {item.author && (
          <span className="max-w-16 shrink-0 truncate text-[10px] text-muted-foreground">
            {item.author.username}
          </span>
        )}
      </button>
    </li>
  );
}

/**
 * "PR / MR" section of the sidebar (also the rail popover): open pull requests of the repo's
 * hosted project, with state / people / label filters. Renders `<li>` rows.
 */
export function PullSidebarList({ repoId, filter = "" }: { repoId: number; filter?: string }) {
  const { t } = useTranslation();
  const { filters, remotes, remote, usable, list } = useSidebarPulls(repoId);
  const set = usePullUi((s) => s.setFilters);
  const openManager = useAccountDialogs((s) => s.openManager);
  const term = shortTerm(remote?.family);
  const [errorOpen, setErrorOpen] = useState(false);

  if (remotes.isPending) return <Empty>{t("pulls.list.loading")}</Empty>;
  if (!remote) return <Empty>{t("pulls.list.noRemote")}</Empty>;

  const toolbar = (
    <li className="flex h-6 items-center gap-1 pr-2 pl-7">
      <select
        aria-label={t("pulls.list.state")}
        value={filters.state}
        onChange={(e) => set(repoId, { state: e.target.value as PullFilters["state"] })}
        className="h-5 min-w-0 rounded border bg-background px-1 text-[11px]"
      >
        {STATES.map((s) => (
          <option key={s} value={s}>
            {t(`pulls.list.stateOption.${s}`)}
          </option>
        ))}
      </select>
      {(remotes.data?.length ?? 0) > 1 && (
        <select
          aria-label={t("pulls.list.remote")}
          value={remote.remote}
          onChange={(e) => set(repoId, { remote: e.target.value })}
          className="h-5 max-w-24 min-w-0 rounded border bg-background px-1 text-[11px]"
          title={remote.fullName}
        >
          {remotes.data?.map((r) => (
            <option key={r.remote} value={r.remote}>
              {r.remote}
            </option>
          ))}
        </select>
      )}
      <span className="flex-1" />
      <FilterPopover repoId={repoId} filters={filters} accountId={remote.accountId} />
      <SmallButton label={t("pulls.list.refresh")} onClick={() => void list.refetch()}>
        <RefreshCw className={cn("size-3", list.isFetching && "animate-spin")} />
      </SmallButton>
      <SmallButton label={t("pulls.create.open", { term })} onClick={() => openCreatePull(repoId)}>
        <GitPullRequestCreate className="size-3" />
      </SmallButton>
    </li>
  );

  if (!remote.accountId) {
    return (
      <>
        {toolbar}
        <li className="py-1 pr-2 pl-8 text-[11px] text-muted-foreground">
          {t("pulls.list.unbound", { remote: remote.remote })}{" "}
          <button
            type="button"
            className="text-primary hover:underline"
            onClick={() => openManager()}
          >
            {t("pulls.list.bind")}
          </button>
        </li>
      </>
    );
  }
  if (!usable) {
    return (
      <>
        {toolbar}
        <li className="py-1 pr-2 pl-8 text-[11px] text-muted-foreground">
          {t("pulls.list.noToken")}{" "}
          <button
            type="button"
            className="text-primary hover:underline"
            onClick={() => openManager({ accountId: remote.accountId, reauth: true })}
          >
            {t("pulls.list.signIn")}
          </button>
        </li>
      </>
    );
  }

  if (list.isError) {
    const d = describePullError(list.error);
    return (
      <>
        {toolbar}
        <li className="py-1 pr-2 pl-8 text-[11px]" role="alert">
          <button
            type="button"
            className="text-left text-danger"
            onClick={() => setErrorOpen(!errorOpen)}
          >
            {d.message}
          </button>
          {errorOpen && d.hint && <p className="mt-0.5 text-muted-foreground">{d.hint}</p>}
          <div className="mt-0.5 flex flex-wrap gap-2">
            {d.actions.map((a) => (
              <button
                key={a.label}
                type="button"
                className="text-primary hover:underline"
                onClick={a.run}
              >
                {a.label}
              </button>
            ))}
            <button
              type="button"
              className="text-primary hover:underline"
              onClick={() => void list.refetch()}
            >
              {t("pulls.list.retry")}
            </button>
          </div>
        </li>
      </>
    );
  }

  const needle = filter.trim().toLowerCase();
  const items = (list.data?.pages ?? [])
    .flatMap((p) => p.items)
    .filter(
      (i) => !needle || i.title.toLowerCase().includes(needle) || String(i.number) === needle,
    );

  return (
    <>
      {toolbar}
      {list.isPending && (
        <li className="flex items-center gap-1 py-1 pl-8 text-[11px] text-muted-foreground">
          <LoaderCircle className="size-3 animate-spin" />
          {t("pulls.list.loading")}
        </li>
      )}
      {list.data && items.length === 0 && <Empty>{t("pulls.list.empty", { term })}</Empty>}
      {items.map((item) => (
        <Row
          key={`pull-${item.number}`}
          item={item}
          accountId={remote.accountId!}
          fullName={remote.fullName}
          repoId={repoId}
        />
      ))}
      {list.hasNextPage && (
        <li className="pl-8">
          <button
            type="button"
            disabled={list.isFetchingNextPage}
            onClick={() => void list.fetchNextPage()}
            className="flex h-6 items-center gap-1 text-[11px] text-primary hover:underline disabled:opacity-60"
          >
            <GitPullRequest className="size-3" />
            {t("pulls.list.more")}
          </button>
        </li>
      )}
    </>
  );
}
