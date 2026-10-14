import { Archive, GitFork, Lock, RefreshCw, Star, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Account, AccountRepos, CloneRepo, Provider } from "@/ipc";
import { formatBytes } from "@/features/transfers/format";
import { cn } from "@/lib/utils";
import { accountTitle, MAX_ROWS, relativeTime } from "./model";

function RepoRow({
  repo,
  account,
  providers,
  selected,
  now,
  onSelect,
}: {
  repo: CloneRepo;
  account: Account | undefined;
  providers: Provider[] | undefined;
  selected: boolean;
  now: number;
  onSelect: (repo: CloneRepo) => void;
}) {
  const { t, i18n } = useTranslation();
  return (
    <li>
      <button
        type="button"
        role="option"
        aria-selected={selected}
        onClick={() => onSelect(repo)}
        title={repo.description ?? repo.webUrl}
        className={cn(
          "flex w-full items-center gap-2 rounded px-2 py-1 text-left text-xs hover:bg-surface",
          selected && "bg-primary/10 ring-1 ring-primary/40",
        )}
      >
        <span className="flex w-44 shrink-0 items-center gap-1.5 truncate text-muted-foreground">
          <span
            aria-hidden
            className="size-2 shrink-0 rounded-full"
            style={{ backgroundColor: account?.color ?? "currentColor" }}
          />
          <span className="truncate">{accountTitle(account, providers)}</span>
        </span>
        <span className="flex min-w-0 flex-1 items-center gap-1.5">
          <span className="truncate font-medium">{repo.fullName}</span>
          {repo.private && (
            <Lock
              className="size-3 shrink-0 text-muted-foreground"
              aria-label={t("clone.private")}
            />
          )}
          {repo.fork && (
            <span className="flex shrink-0 items-center gap-0.5 text-muted-foreground">
              <GitFork className="size-3" />
              {repo.parentFullName
                ? t("clone.forkOf", { parent: repo.parentFullName })
                : t("clone.fork")}
            </span>
          )}
          {repo.archived && (
            <Archive
              className="size-3 shrink-0 text-muted-foreground"
              aria-label={t("clone.archived")}
            />
          )}
        </span>
        {repo.stars > 0 && (
          <span className="flex shrink-0 items-center gap-0.5 text-muted-foreground">
            <Star className="size-3" />
            {repo.stars}
          </span>
        )}
        {repo.lfsSizeBytes != null && repo.lfsSizeBytes > 0 && (
          <span className="shrink-0 rounded bg-surface-2 px-1 font-mono text-[10px]">
            LFS {formatBytes(repo.lfsSizeBytes)}
          </span>
        )}
        {repo.updatedAt != null && (
          <span className="w-28 shrink-0 truncate text-right text-muted-foreground">
            {t("clone.updated", { when: relativeTime(repo.updatedAt, now, i18n.language) })}
          </span>
        )}
      </button>
    </li>
  );
}

/** Per-account load problems, shown above the list (other accounts stay usable). */
function AccountErrors({
  accounts,
  byId,
  providers,
}: {
  accounts: AccountRepos[];
  byId: Map<string, Account>;
  providers: Provider[] | undefined;
}) {
  const { t } = useTranslation();
  const failed = accounts.filter((a) => a.error);
  if (failed.length === 0) return null;
  return (
    <ul className="space-y-0.5">
      {failed.map((a) => (
        <li
          key={a.accountId}
          className="flex items-center gap-1.5 text-[11px] text-warning"
          title={a.error?.message}
        >
          <TriangleAlert className="size-3 shrink-0" />
          <span className="truncate">
            {t("clone.listError", {
              account: accountTitle(byId.get(a.accountId), providers),
              reason: t(`clone.listErrorCode.${a.error?.code ?? "other"}`, {
                defaultValue: a.error?.message ?? "",
              }),
            })}
          </span>
        </li>
      ))}
    </ul>
  );
}

export function RepoList({
  repos,
  total,
  accounts,
  byId,
  providers,
  selectedKey,
  loading,
  refreshing,
  onRefresh,
  onSelect,
  now,
}: {
  repos: CloneRepo[];
  total: number;
  accounts: AccountRepos[];
  byId: Map<string, Account>;
  providers: Provider[] | undefined;
  selectedKey: string | null;
  loading: boolean;
  refreshing: boolean;
  onRefresh: () => void;
  onSelect: (repo: CloneRepo) => void;
  now: number;
}) {
  const { t } = useTranslation();
  const shown = repos.slice(0, MAX_ROWS);
  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between text-[11px] text-muted-foreground">
        <span>
          {loading ? t("clone.loading") : t("clone.count", { count: repos.length, total })}
        </span>
        <button
          type="button"
          onClick={onRefresh}
          disabled={refreshing}
          className="flex items-center gap-1 rounded px-1 hover:bg-surface hover:text-foreground disabled:opacity-50"
          aria-label={t("clone.refresh")}
          title={t("clone.refresh")}
        >
          <RefreshCw className={cn("size-3", refreshing && "animate-spin")} />
          {t("clone.refresh")}
        </button>
      </div>
      <AccountErrors accounts={accounts} byId={byId} providers={providers} />
      <ul
        role="listbox"
        aria-label={t("clone.listLabel")}
        className="h-56 overflow-y-auto rounded-md border bg-background p-1"
      >
        {shown.map((repo) => (
          <RepoRow
            key={`${repo.accountId}/${repo.id}`}
            repo={repo}
            account={byId.get(repo.accountId)}
            providers={providers}
            selected={selectedKey === `${repo.accountId}/${repo.id}`}
            now={now}
            onSelect={onSelect}
          />
        ))}
        {!loading && shown.length === 0 && (
          <li className="p-4 text-center text-xs text-muted-foreground">
            {total === 0 ? t("clone.emptyNoAccounts") : t("clone.noMatch")}
          </li>
        )}
        {repos.length > MAX_ROWS && (
          <li className="p-2 text-center text-[11px] text-muted-foreground">
            {t("clone.more", { count: repos.length - MAX_ROWS })}
          </li>
        )}
      </ul>
    </div>
  );
}
