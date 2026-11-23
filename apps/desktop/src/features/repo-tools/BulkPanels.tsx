import { useRef, useState } from "react";
import { useQueries } from "@tanstack/react-query";
import { ChevronDown, ChevronUp, GripVertical, LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { worktreeKeys } from "@/features/worktree/queries";
import { commands, errorMessage, type RepoOverview } from "@/ipc";
import { cn } from "@/lib/utils";
import { moveItem, syncOrder } from "./bulk";
import { bulkCommit, bulkPush, useBulkRun, type BulkItem, type BulkStatus } from "./bulkRun";

const STATUS_CLASS: Record<BulkStatus, string> = {
  waiting: "text-muted-foreground",
  checking: "text-muted-foreground",
  running: "text-info",
  done: "text-success",
  skipped: "text-muted-foreground",
  attention: "text-warning",
  failed: "text-danger",
  notRun: "text-muted-foreground",
};

/** Status of one repository in a bulk run, with "Open" when the user has to step in. */
function StatusCell({ item, onOpen }: { item: BulkItem; onOpen: (repoId: number) => void }) {
  const { t } = useTranslation();
  const busy = item.status === "checking" || item.status === "running";
  return (
    <div className="flex min-w-0 items-start gap-2">
      <div className="min-w-0 flex-1">
        <span className={cn("flex items-center gap-1 font-medium", STATUS_CLASS[item.status])}>
          {busy && <LoaderCircle className="size-3 animate-spin" />}
          {t(`overview.bulk.status.${item.status}`)}
        </span>
        {item.reason && (
          <span className="block text-[11px] break-words text-muted-foreground">{item.reason}</span>
        )}
      </div>
      {(item.status === "attention" || item.status === "failed") && (
        <Button
          size="xs"
          variant="outline"
          title={t("overview.bulk.openTip")}
          onClick={() => onOpen(item.repoId)}
        >
          {t("overview.bulk.open")}
        </Button>
      )}
    </div>
  );
}

/** Progress and result of the current (or last) bulk run. */
function RunList({ onOpen }: { onOpen: (repoId: number) => void }) {
  const { t } = useTranslation();
  const run = useBulkRun((s) => s.run);
  if (!run) return null;
  const finished = run.items.filter(
    (i) => i.status !== "waiting" && i.status !== "checking" && i.status !== "running",
  ).length;
  return (
    <div className="space-y-2">
      <p className="text-xs text-muted-foreground" aria-live="polite">
        {t("overview.bulk.progress", { done: finished, total: run.items.length })}
      </p>
      <ol className="max-h-[50vh] divide-y overflow-y-auto rounded-md border text-xs">
        {run.items.map((item, index) => (
          <li
            key={item.repoId}
            className="grid grid-cols-[1.5rem_minmax(0,12rem)_1fr] items-start gap-2 px-2 py-1.5"
            data-testid={`bulk-item-${item.repoId}`}
          >
            <span className="text-muted-foreground tabular-nums">{index + 1}</span>
            <span className="truncate font-medium">{item.name}</span>
            <StatusCell item={item} onOpen={onOpen} />
          </li>
        ))}
      </ol>
    </div>
  );
}

interface PanelProps {
  /** Repositories the action was opened for. */
  rows: RepoOverview[];
  onBack: () => void;
  onOpen: (repoId: number) => void;
}

function RunFooter({ onBack, children }: { onBack: () => void; children?: React.ReactNode }) {
  const { t } = useTranslation();
  const run = useBulkRun((s) => s.run);
  const cancel = useBulkRun((s) => s.cancel);
  const clear = useBulkRun((s) => s.clear);
  return (
    <div className="flex flex-wrap items-center gap-2">
      {run?.running ? (
        <Button size="sm" variant="outline" disabled={run.cancelling} onClick={cancel}>
          {run.cancelling ? t("overview.bulk.cancelling") : t("overview.bulk.cancel")}
        </Button>
      ) : (
        <Button
          size="sm"
          variant="ghost"
          onClick={() => {
            clear();
            onBack();
          }}
        >
          {t("overview.bulk.back")}
        </Button>
      )}
      <div className="flex-1" />
      {children}
    </div>
  );
}

/** "Push in order": arrange the repositories, pick what a failure does, push one at a time. */
export function BulkPushPanel({ rows, onBack, onOpen }: PanelProps) {
  const { t } = useTranslation();
  const run = useBulkRun((s) => s.run);
  const [order, setOrder] = useState<number[]>(() => rows.map((r) => r.repoId));
  const [stopOnFailure, setStopOnFailure] = useState(true);
  const drag = useRef<number | null>(null);
  const [dragOver, setDragOver] = useState<number | null>(null);

  // The user's order, with repositories added or gone since then.
  const current = syncOrder(
    order,
    rows.map((r) => r.repoId),
  );
  const byId = new Map(rows.map((r) => [r.repoId, r]));
  const ordered = current.map((id) => byId.get(id)).filter((r): r is RepoOverview => !!r);

  if (run?.kind === "push") {
    return (
      <div className="space-y-3">
        <RunList onOpen={onOpen} />
        <RunFooter onBack={onBack} />
      </div>
    );
  }

  const move = (from: number, to: number) => setOrder(moveItem(current, from, to));

  return (
    <div className="space-y-3">
      <p className="text-xs text-muted-foreground">{t("overview.bulk.push.description")}</p>
      {ordered.length === 0 ? (
        <p className="rounded-md border px-2 py-6 text-center text-xs text-muted-foreground">
          {t("overview.bulk.push.empty")}
        </p>
      ) : (
        <ol
          className="max-h-[45vh] divide-y overflow-y-auto rounded-md border text-xs"
          aria-label={t("overview.bulk.push.orderLabel")}
        >
          {ordered.map((row, index) => (
            <li
              key={row.repoId}
              draggable
              onDragStart={(e) => {
                drag.current = index;
                e.dataTransfer.effectAllowed = "move";
              }}
              onDragOver={(e) => {
                e.preventDefault();
                setDragOver(index);
              }}
              onDragLeave={() => setDragOver((d) => (d === index ? null : d))}
              onDrop={(e) => {
                e.preventDefault();
                if (drag.current !== null) move(drag.current, index);
                drag.current = null;
                setDragOver(null);
              }}
              onDragEnd={() => {
                drag.current = null;
                setDragOver(null);
              }}
              className={cn(
                "flex items-center gap-2 px-2 py-1.5",
                dragOver === index && "bg-accent",
              )}
              data-testid={`push-order-${row.repoId}`}
            >
              <GripVertical
                className="size-3.5 shrink-0 cursor-grab text-muted-foreground"
                aria-label={t("overview.bulk.push.drag")}
              />
              <span className="w-5 text-muted-foreground tabular-nums">{index + 1}</span>
              <span className="min-w-0 flex-1 truncate font-medium" title={row.path}>
                {row.name}
              </span>
              <span className="max-w-40 truncate text-muted-foreground">{row.branch}</span>
              <span className="w-24 text-right text-success tabular-nums">
                {t("overview.bulk.push.ahead", { count: row.ahead })}
              </span>
              <Button
                size="icon-xs"
                variant="ghost"
                disabled={index === 0}
                aria-label={t("overview.bulk.push.moveUp", { name: row.name })}
                onClick={() => move(index, index - 1)}
              >
                <ChevronUp />
              </Button>
              <Button
                size="icon-xs"
                variant="ghost"
                disabled={index === ordered.length - 1}
                aria-label={t("overview.bulk.push.moveDown", { name: row.name })}
                onClick={() => move(index, index + 1)}
              >
                <ChevronDown />
              </Button>
            </li>
          ))}
        </ol>
      )}
      <fieldset className="space-y-1 text-xs">
        <legend className="mb-1 font-medium">{t("overview.bulk.push.onFailure")}</legend>
        <label className="flex items-center gap-1.5">
          <input
            type="radio"
            name="bulk-push-failure"
            checked={stopOnFailure}
            onChange={() => setStopOnFailure(true)}
            className="accent-primary"
          />
          {t("overview.bulk.push.stop")}
        </label>
        <label className="flex items-center gap-1.5">
          <input
            type="radio"
            name="bulk-push-failure"
            checked={!stopOnFailure}
            onChange={() => setStopOnFailure(false)}
            className="accent-primary"
          />
          {t("overview.bulk.push.continue")}
        </label>
      </fieldset>
      <p className="text-[11px] text-muted-foreground">{t("overview.bulk.guardNote")}</p>
      <RunFooter onBack={onBack}>
        <Button
          size="sm"
          disabled={ordered.length === 0 || !!run?.running}
          onClick={() => void bulkPush(ordered, stopOnFailure)}
        >
          {t("overview.bulk.push.start", { count: ordered.length })}
        </Button>
      </RunFooter>
    </div>
  );
}

/** "Quick commit": one message for the staged changes of each repository. */
export function BulkCommitPanel({ rows, onBack, onOpen }: PanelProps) {
  const { t } = useTranslation();
  const run = useBulkRun((s) => s.run);
  const [message, setMessage] = useState("");
  const readable = rows.filter((r) => !r.missing && !r.error);
  const statuses = useQueries({
    queries: readable.map((r) => ({
      queryKey: worktreeKeys.status(r.repoId),
      queryFn: () => commands.worktreeStatus(r.repoId),
      staleTime: 0,
    })),
  });

  if (run?.kind === "commit") {
    return (
      <div className="space-y-3">
        <RunList onOpen={onOpen} />
        <RunFooter onBack={onBack} />
      </div>
    );
  }

  const loading = statuses.some((s) => s.isPending);
  const preview = readable.map((row, i) => {
    const q = statuses[i];
    const staged = q?.data?.staged.length ?? 0;
    const conflicts = q?.data?.conflicted.length ?? 0;
    const error = q?.error ? errorMessage(q.error) : null;
    const pending = q?.isPending ?? true;
    const commit = !pending && !error && staged > 0 && conflicts === 0;
    return { row, staged, conflicts, error, pending, commit };
  });
  const willCommit = preview.filter((p) => p.commit);
  const willSkip = preview.filter((p) => !p.commit && !p.pending);
  const trimmed = message.trim();

  return (
    <div className="space-y-3">
      <p className="text-xs text-muted-foreground">{t("overview.bulk.commit.description")}</p>
      <textarea
        value={message}
        onChange={(e) => setMessage(e.target.value)}
        placeholder={t("overview.bulk.commit.placeholder")}
        aria-label={t("overview.bulk.commit.message")}
        rows={3}
        className="w-full resize-y rounded-md border bg-background px-2 py-1.5 font-mono text-xs focus:border-primary focus:outline-none"
      />
      {loading && (
        <p className="flex items-center gap-1 text-xs text-muted-foreground">
          <LoaderCircle className="size-3 animate-spin" />
          {t("overview.bulk.commit.loading")}
        </p>
      )}
      <div className="grid gap-3 sm:grid-cols-2">
        <section className="space-y-1">
          <h3 className="text-xs font-semibold">
            {t("overview.bulk.commit.willCommit", { count: willCommit.length })}
          </h3>
          <ul className="max-h-[30vh] divide-y overflow-y-auto rounded-md border text-xs">
            {willCommit.map(({ row, staged }) => (
              <li key={row.repoId} className="flex gap-2 px-2 py-1">
                <span className="min-w-0 flex-1 truncate font-medium">{row.name}</span>
                <span className="text-muted-foreground">
                  {t("overview.bulk.commit.staged", { count: staged })}
                </span>
              </li>
            ))}
            {willCommit.length === 0 && !loading && (
              <li className="px-2 py-2 text-muted-foreground">
                {t("overview.bulk.commit.noneStaged")}
              </li>
            )}
          </ul>
        </section>
        <section className="space-y-1">
          <h3 className="text-xs font-semibold">
            {t("overview.bulk.commit.willSkip", {
              count: willSkip.length + rows.length - readable.length,
            })}
          </h3>
          <ul className="max-h-[30vh] divide-y overflow-y-auto rounded-md border text-xs">
            {willSkip.map(({ row, conflicts, error }) => (
              <li key={row.repoId} className="flex gap-2 px-2 py-1">
                <span className="min-w-0 flex-1 truncate font-medium">{row.name}</span>
                <span className="text-muted-foreground">
                  {error ??
                    (conflicts > 0
                      ? t("overview.bulk.reason.conflicts")
                      : t("overview.bulk.reason.nothingStaged"))}
                </span>
              </li>
            ))}
            {rows
              .filter((r) => r.missing || r.error)
              .map((row) => (
                <li key={row.repoId} className="flex gap-2 px-2 py-1">
                  <span className="min-w-0 flex-1 truncate font-medium">{row.name}</span>
                  <span className="text-muted-foreground">
                    {t("overview.bulk.reason.unreadable")}
                  </span>
                </li>
              ))}
          </ul>
        </section>
      </div>
      <p className="text-[11px] text-muted-foreground">{t("overview.bulk.commit.stageNote")}</p>
      <p className="text-[11px] text-muted-foreground">{t("overview.bulk.guardNote")}</p>
      <RunFooter onBack={onBack}>
        <Button
          size="sm"
          disabled={willCommit.length === 0 || !trimmed || !!run?.running}
          title={!trimmed ? t("overview.bulk.commit.needMessage") : undefined}
          onClick={() =>
            void bulkCommit(
              willCommit.map((p) => p.row),
              trimmed,
            )
          }
        >
          {t("overview.bulk.commit.start", { count: willCommit.length })}
        </Button>
      </RunFooter>
    </div>
  );
}
