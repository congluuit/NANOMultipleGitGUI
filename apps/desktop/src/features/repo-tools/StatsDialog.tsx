import { LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { splitPath } from "@/features/commit/format";
import { formatDate } from "@/features/graph/draw";
import { errorMessage, type RepoStats } from "@/ipc";
import { barPercent } from "./model";
import { useRepoStats } from "./queries";

const WEEK_SECONDS = 7 * 24 * 3600;

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section>
      <h3 className="mb-1 text-[11px] font-semibold text-muted-foreground uppercase">{title}</h3>
      {children}
    </section>
  );
}

function Activity({ stats }: { stats: RepoStats }) {
  const { t, i18n } = useTranslation();
  const max = Math.max(1, ...stats.weeks);
  const last = stats.weeks.length - 1;
  return (
    <div
      className="flex h-20 items-end gap-px rounded-md border bg-background p-2"
      role="img"
      aria-label={t("stats.activityLabel", { count: stats.weeks.reduce((a, b) => a + b, 0) })}
    >
      {stats.weeks.map((count, i) => {
        const start = stats.lastWeekStart - (last - i) * WEEK_SECONDS;
        return (
          <div
            key={start}
            className="flex h-full flex-1 items-end"
            title={t("stats.weekTip", { count, date: formatDate(start, i18n.language) })}
          >
            <div
              className={count > 0 ? "w-full rounded-sm bg-primary" : "w-full bg-border"}
              style={{ height: count > 0 ? `${barPercent(count, max)}%` : "1px" }}
            />
          </div>
        );
      })}
    </div>
  );
}

function Bars({ rows }: { rows: { key: string; label: React.ReactNode; value: number }[] }) {
  const max = Math.max(1, ...rows.map((r) => r.value));
  return (
    <ul className="space-y-1">
      {rows.map((r) => (
        <li key={r.key} className="flex items-center gap-2 text-xs">
          <span className="min-w-0 flex-1 truncate">{r.label}</span>
          <span className="h-1.5 w-28 shrink-0 overflow-hidden rounded-full bg-border">
            <span
              className="block h-full rounded-full bg-primary"
              style={{ width: `${barPercent(r.value, max)}%` }}
            />
          </span>
          <span className="w-10 shrink-0 text-right font-mono tabular-nums">{r.value}</span>
        </li>
      ))}
    </ul>
  );
}

function StatsBody({ stats }: { stats: RepoStats }) {
  const { t, i18n } = useTranslation();
  const date = (secs: number | null) => (secs ? formatDate(secs, i18n.language) : "");
  const tiles = [
    { label: t("stats.commits"), value: stats.totalCommits.toLocaleString(i18n.language) },
    { label: t("stats.authors"), value: stats.authorCount.toLocaleString(i18n.language) },
    { label: t("stats.firstCommit"), value: date(stats.firstCommit) },
    { label: t("stats.lastCommit"), value: date(stats.lastCommit) },
  ];
  return (
    <div className="max-h-[65vh] space-y-4 overflow-y-auto pr-1">
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
        {tiles.map((tile) => (
          <div key={tile.label} className="rounded-md border bg-background px-3 py-2">
            <div className="text-[11px] text-muted-foreground">{tile.label}</div>
            <div className="truncate text-sm font-semibold">{tile.value || t("stats.none")}</div>
          </div>
        ))}
      </div>
      <Section title={t("stats.activity")}>
        <Activity stats={stats} />
      </Section>
      <div className="grid gap-4 sm:grid-cols-2">
        <Section title={t("stats.topAuthors")}>
          {stats.authors.length ? (
            <Bars
              rows={stats.authors.map((a) => ({
                key: `${a.name} ${a.email}`,
                label: <span title={a.email}>{a.name}</span>,
                value: a.commits,
              }))}
            />
          ) : (
            <p className="text-xs text-muted-foreground">{t("stats.none")}</p>
          )}
        </Section>
        <Section title={t("stats.topFiles")}>
          {stats.files.length ? (
            <Bars
              rows={stats.files.map((f) => {
                const { dir, name } = splitPath(f.path);
                return {
                  key: f.path,
                  label: (
                    <span title={f.path}>
                      <span className="text-muted-foreground">{dir}</span>
                      {name}
                    </span>
                  ),
                  value: f.changes,
                };
              })}
            />
          ) : (
            <p className="text-xs text-muted-foreground">{t("stats.noRecent")}</p>
          )}
        </Section>
      </div>
    </div>
  );
}

/** Commits, authors, weekly activity and the files that change most. */
export function StatsDialog({ repoId, onClose }: { repoId: number; onClose: () => void }) {
  const { t } = useTranslation();
  const stats = useRepoStats(repoId, true);
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{t("stats.title")}</DialogTitle>
          <DialogDescription>{t("stats.description")}</DialogDescription>
        </DialogHeader>
        {stats.isPending && (
          <p className="flex items-center gap-2 text-xs text-muted-foreground">
            <LoaderCircle className="size-4 animate-spin" />
            {t("tools.loading")}
          </p>
        )}
        {stats.isError && <p className="text-xs text-danger">{errorMessage(stats.error)}</p>}
        {stats.data && <StatsBody stats={stats.data} />}
      </DialogContent>
    </Dialog>
  );
}
