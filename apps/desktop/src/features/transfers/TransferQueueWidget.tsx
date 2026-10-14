import { useMemo, useState, type ReactNode } from "react";
import {
  AlertTriangle,
  ArrowDownToLine,
  ArrowUpDown,
  ArrowUpFromLine,
  CheckCircle2,
  LoaderCircle,
  RefreshCw,
  X,
} from "lucide-react";
import { Popover } from "radix-ui";
import { useTranslation } from "react-i18next";
import type { LfsFailure } from "@/ipc";
import { cn } from "@/lib/utils";
import { useWorkspace } from "@/stores/workspace";
import { accountLabel, cancelQueued, downloadAnyway, repoAccount } from "./actions";
import {
  failureTexts,
  formatBytes,
  itemPercent,
  lfsSummaryText,
  percentOf,
  progressLine,
  summarize,
} from "./format";
import { retryOf, useTransferQueue, type QueueItem } from "./store";

const SMALL_BUTTON =
  "shrink-0 rounded px-1.5 py-0.5 text-[11px] hover:bg-surface-2 disabled:opacity-50";

function Bar({ percent, tone = "primary" }: { percent: number | null; tone?: string }) {
  return (
    <span className="block h-1.5 w-full overflow-hidden rounded bg-border">
      <span
        className={cn(
          "block h-full transition-[width]",
          tone === "danger" ? "bg-danger" : tone === "muted" ? "bg-muted-foreground" : "bg-primary",
          percent === null && "w-1/3 animate-pulse",
        )}
        style={percent === null ? undefined : { width: `${percent}%` }}
      />
    </span>
  );
}

function CopyText({ text }: { text: string }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  return (
    <span className="flex min-w-0 items-center gap-1">
      <code className="min-w-0 truncate rounded bg-surface-2 px-1 py-0.5 select-all" title={text}>
        {text}
      </code>
      <button
        type="button"
        className={SMALL_BUTTON}
        onClick={() => {
          void navigator.clipboard
            ?.writeText(text)
            .then(() => setCopied(true))
            .catch(() => undefined);
        }}
      >
        {copied ? t("transfers.copied") : t("transfers.copy")}
      </button>
    </span>
  );
}

function FailureBlock({ item, failure }: { item: QueueItem; failure: LfsFailure }) {
  const { t } = useTranslation();
  const texts = failureTexts(failure, accountLabel(repoAccount(item.repoId)));
  const link = failure.billingUrl ?? failure.settingsUrl;
  return (
    <div className="mt-1 space-y-1 rounded border border-danger/30 bg-danger/5 p-1.5 text-[11px]">
      <p className="font-medium text-danger">{texts.title}</p>
      <p className="text-muted-foreground">{texts.hint}</p>
      {link && <CopyText text={link} />}
      {texts.extra.map((line) => (
        <p key={line} className="truncate text-muted-foreground" title={line}>
          {line}
        </p>
      ))}
      {failure.kind === "insufficientDiskSpace" && (
        <button
          type="button"
          className={cn(SMALL_BUTTON, "border")}
          onClick={() => void downloadAnyway(item)}
        >
          {t("transfers.ignoreDiskSpace")}
        </button>
      )}
      {failure.message && (
        <details>
          <summary className="cursor-pointer text-muted-foreground">
            {t("transfers.details")}
          </summary>
          <pre className="mt-1 max-h-24 overflow-auto font-mono text-[10px] whitespace-pre-wrap">
            {failure.message}
          </pre>
        </details>
      )}
    </div>
  );
}

function KindIcon({ item }: { item: QueueItem }) {
  if (item.status === "running") {
    return <LoaderCircle className="size-3.5 shrink-0 animate-spin text-primary" />;
  }
  if (item.status === "failed") return <AlertTriangle className="size-3.5 shrink-0 text-danger" />;
  if (item.status === "cancelled") return <X className="size-3.5 shrink-0 text-muted-foreground" />;
  if (item.kind === "lfsUpload" || item.kind === "push") {
    return <ArrowUpFromLine className="size-3.5 shrink-0 text-success" />;
  }
  if (item.kind === "fetch") return <RefreshCw className="size-3.5 shrink-0 text-success" />;
  return <ArrowDownToLine className="size-3.5 shrink-0 text-success" />;
}

export function QueueRow({ item }: { item: QueueItem }) {
  const { t } = useTranslation();
  const percent = itemPercent(item);
  const retry = item.status === "failed" || item.status === "cancelled" ? retryOf(item.opId) : null;
  const line = progressLine(item);
  return (
    <li className="space-y-1 py-1.5" data-testid={`transfer-${item.opId}`}>
      <div className="flex items-center gap-1.5">
        <KindIcon item={item} />
        <span className="shrink-0 font-medium">{t(`transfers.kind.${item.kind}`)}</span>
        {item.subject && (
          <span className="min-w-0 truncate text-muted-foreground" title={item.subject}>
            {item.subject}
          </span>
        )}
        <span className="flex-1" />
        {item.status !== "running" && (
          <span
            className={cn(
              "shrink-0 text-[10px]",
              item.status === "failed" ? "text-danger" : "text-muted-foreground",
            )}
          >
            {t(`transfers.status.${item.status}`)}
          </span>
        )}
        {item.status === "running" ? (
          <button
            type="button"
            className={cn(SMALL_BUTTON, "text-danger hover:bg-danger/10")}
            disabled={item.cancelling}
            onClick={() => void cancelQueued(item.opId)}
          >
            {item.cancelling ? t("transfers.cancelling") : t("transfers.cancel")}
          </button>
        ) : (
          <>
            {retry && (
              <button type="button" className={SMALL_BUTTON} onClick={retry}>
                {t("transfers.retry")}
              </button>
            )}
            <button
              type="button"
              className={cn(SMALL_BUTTON, "text-muted-foreground")}
              aria-label={t("transfers.dismiss")}
              onClick={() => useTransferQueue.getState().dismiss(item.opId)}
            >
              <X className="size-3" />
            </button>
          </>
        )}
      </div>
      {(item.status === "running" || item.lfs) && (
        <Bar
          percent={percent}
          tone={
            item.status === "failed" ? "danger" : item.status === "cancelled" ? "muted" : "primary"
          }
        />
      )}
      {line && <p className="truncate text-[11px] text-muted-foreground">{line}</p>}
      {item.status === "running" && item.lfs && item.lfs.active.length > 0 && (
        <ul className="space-y-0.5 pl-2">
          {item.lfs.active.map((f) => (
            <li key={f.name} className="flex items-center gap-2 text-[10px] text-muted-foreground">
              <span className="min-w-0 flex-1 truncate" title={f.name}>
                {f.name}
              </span>
              <span className="w-16 shrink-0">
                <Bar percent={f.bytesTotal ? percentOf(f.bytesDone, f.bytesTotal) : null} />
              </span>
              <span className="w-28 shrink-0 text-right tabular-nums">
                {formatBytes(f.bytesDone)}
                {f.bytesTotal ? ` / ${formatBytes(f.bytesTotal)}` : ""}
              </span>
            </li>
          ))}
        </ul>
      )}
      {item.failure && <FailureBlock item={item} failure={item.failure} />}
      {item.error && !item.failure && (
        <p className="text-[11px] break-words text-danger">{item.error}</p>
      )}
    </li>
  );
}

/** Popover body: transfers grouped by repository, running first. */
export function QueueList({ items }: { items: readonly QueueItem[] }) {
  const { t } = useTranslation();
  const tabs = useWorkspace((s) => s.tabs);
  const groups = useMemo(() => {
    const byRepo = new Map<number, QueueItem[]>();
    const sorted = [...items].sort(
      (a, b) => Number(b.status === "running") - Number(a.status === "running"),
    );
    for (const item of sorted) {
      const list = byRepo.get(item.repoId) ?? [];
      list.push(item);
      byRepo.set(item.repoId, list);
    }
    return [...byRepo.entries()];
  }, [items]);
  if (!items.length) {
    return <p className="p-3 text-xs text-muted-foreground">{t("transfers.empty")}</p>;
  }
  return (
    <div className="max-h-96 overflow-y-auto px-3 py-1 text-xs">
      {groups.map(([repoId, list]) => (
        <section key={repoId} className="py-1">
          <h3 className="text-[11px] font-semibold text-muted-foreground uppercase">
            {tabs.find((tab) => tab.id === repoId)?.name ??
              t("transfers.unknownRepo", { id: repoId })}
          </h3>
          <ul className="divide-y">
            {list.map((item) => (
              <QueueRow key={item.opId} item={item} />
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}

/** Status-bar entry of the transfer queue (`⇅ LFS 42% · 760 MB/1.8 GB`) with its popover. */
export function TransferQueueWidget() {
  const { t } = useTranslation();
  const itemsById = useTransferQueue((s) => s.items);
  const order = useTransferQueue((s) => s.order);
  const items = useMemo(
    () => order.map((id) => itemsById[id]).filter((i): i is QueueItem => !!i),
    [itemsById, order],
  );
  const summary = useMemo(() => summarize(items), [items]);
  if (!items.length) return null;

  let label: ReactNode;
  if (summary.lfs) {
    label = (
      <>
        <ArrowUpDown className="size-3 shrink-0 text-primary" />
        <span className="font-medium">{lfsSummaryText(summary.lfs)}</span>
        {summary.lfs.percent !== null && (
          <span className="h-1.5 w-14 shrink-0">
            <Bar percent={summary.lfs.percent} />
          </span>
        )}
      </>
    );
  } else if (summary.git) {
    const git = summary.git;
    label = (
      <>
        <LoaderCircle className="size-3 shrink-0 animate-spin text-primary" />
        <span className="shrink-0 font-medium">{t(`transfers.kind.${git.kind}`)}</span>
        {git.percent !== null && (
          <span className="h-1.5 w-14 shrink-0">
            <Bar percent={git.percent} />
          </span>
        )}
        <span className="min-w-0 truncate text-muted-foreground">
          {git.text || t("transfers.starting")}
        </span>
      </>
    );
  } else if (summary.failed) {
    label = (
      <>
        <AlertTriangle className="size-3 shrink-0 text-danger" />
        <span className="text-danger">{t("transfers.failedCount", { count: summary.failed })}</span>
      </>
    );
  } else {
    label = (
      <>
        <CheckCircle2 className="size-3 shrink-0 text-success" />
        <span>{t("transfers.doneCount", { count: summary.finished })}</span>
      </>
    );
  }
  const extraRunning = summary.running - (summary.lfs || summary.git ? 1 : 0);

  return (
    <Popover.Root>
      <Popover.Trigger asChild>
        <button
          type="button"
          role="status"
          aria-label={t("transfers.open")}
          className="flex max-w-96 min-w-0 items-center gap-1.5 rounded px-1 text-foreground hover:bg-surface"
        >
          {label}
          {extraRunning > 0 && !summary.lfs && (
            <span className="shrink-0 text-muted-foreground">+{extraRunning}</span>
          )}
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          side="top"
          align="end"
          sideOffset={6}
          className="z-50 w-[28rem] rounded-md border bg-popover text-popover-foreground shadow-lg"
        >
          <div className="flex items-center gap-2 border-b px-3 py-2 text-xs">
            <span className="font-semibold">{t("transfers.title")}</span>
            <span className="text-muted-foreground">
              {t("transfers.runningCount", { count: summary.running })}
            </span>
            <span className="flex-1" />
            {summary.finished > 0 && (
              <button
                type="button"
                className={SMALL_BUTTON}
                onClick={() => useTransferQueue.getState().clearFinished()}
              >
                {t("transfers.clearFinished")}
              </button>
            )}
          </div>
          <QueueList items={items} />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
