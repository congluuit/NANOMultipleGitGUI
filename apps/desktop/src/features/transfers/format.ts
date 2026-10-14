import i18n from "i18next";
import type { LfsFailure } from "@/ipc";
import type { QueueItem } from "./store";

const UNITS = ["B", "KB", "MB", "GB", "TB"];

/** `760 MB`, `1.8 GB`, `512 B` (binary multiples, one decimal from 10 KB on below 100). */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "?";
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  if (unit === 0) return `${Math.round(value)} B`;
  const text = value < 100 ? value.toFixed(1).replace(/\.0$/, "") : Math.round(value).toString();
  return `${text} ${UNITS[unit]}`;
}

/** Remaining time: `45 giây`, `3 phút 20 giây`, `1 giờ 05 phút`. */
export function formatEta(secs: number): string {
  const s = Math.max(0, Math.round(secs));
  if (s < 60) return i18n.t("transfers.time.s", { s });
  if (s < 3600) {
    return i18n.t("transfers.time.m", { m: Math.floor(s / 60), s: s % 60 });
  }
  const m = Math.floor((s % 3600) / 60);
  return i18n.t("transfers.time.h", { h: Math.floor(s / 3600), m: m.toString().padStart(2, "0") });
}

/** Whole percent of `done / total`, `null` when the total is unknown. */
export function percentOf(done: number, total: number): number | null {
  if (!(total > 0)) return null;
  return Math.min(100, Math.floor((done * 100) / total));
}

/** Overall percent of an item (LFS bytes, or the git progress line). */
export function itemPercent(item: QueueItem): number | null {
  if (item.status === "done") return 100;
  if (item.lfs) return percentOf(item.lfs.bytesDone, item.lfs.bytesTotal);
  return item.percent;
}

export interface QueueSummary {
  running: number;
  failed: number;
  finished: number;
  /** Combined progress of the running LFS transfers, if any. */
  lfs: { done: number; total: number; percent: number | null } | null;
  /** First running non-LFS transfer (fetch / pull / push). */
  git: QueueItem | null;
}

export function summarize(items: readonly QueueItem[]): QueueSummary {
  let running = 0;
  let failed = 0;
  let finished = 0;
  let lfsDone = 0;
  let lfsTotal = 0;
  let lfsRunning = 0;
  let git: QueueItem | null = null;
  for (const item of items) {
    if (item.status === "running") {
      running += 1;
      if (item.kind === "lfsUpload" || item.kind === "lfsDownload") {
        lfsRunning += 1;
        lfsDone += item.lfs?.bytesDone ?? 0;
        lfsTotal += item.lfs?.bytesTotal ?? 0;
      } else {
        git ??= item;
      }
    } else {
      finished += 1;
      if (item.status === "failed") failed += 1;
    }
  }
  return {
    running,
    failed,
    finished,
    lfs: lfsRunning
      ? { done: lfsDone, total: lfsTotal, percent: percentOf(lfsDone, lfsTotal) }
      : null,
    git,
  };
}

/** Status-bar text of the running LFS transfers: `LFS 42% · 760 MB/1.8 GB`. */
export function lfsSummaryText(lfs: NonNullable<QueueSummary["lfs"]>): string {
  if (lfs.percent === null) {
    return i18n.t("transfers.lfsSummaryNoTotal", { done: formatBytes(lfs.done) });
  }
  return i18n.t("transfers.lfsSummary", {
    percent: lfs.percent,
    done: formatBytes(lfs.done),
    total: formatBytes(lfs.total),
  });
}

/** Translated title and hint of an LFS failure; `account` is the bound account's label. */
export function failureTexts(
  failure: LfsFailure,
  account: string,
): { title: string; hint: string; extra: string[] } {
  const title = i18n.t(`lfsErrors.${failure.kind}.title`, { account });
  const hint = i18n.t(`lfsErrors.${failure.kind}.hint`, { account });
  const extra: string[] = [];
  if (failure.limitBytes !== null && failure.kind === "objectTooLarge") {
    extra.push(i18n.t("lfsErrors.limit", { limit: formatBytes(failure.limitBytes) }));
  }
  if (failure.object) extra.push(i18n.t("lfsErrors.object", { object: failure.object }));
  return { title, hint, extra };
}

/** One-line description of an item's progress for the popover. */
export function progressLine(item: QueueItem): string {
  if (item.lfs) {
    const p = item.lfs;
    const parts: string[] = [];
    const pct = percentOf(p.bytesDone, p.bytesTotal);
    if (pct !== null) parts.push(`${pct}%`);
    parts.push(
      p.bytesTotal > 0
        ? i18n.t("transfers.bytes", {
            done: formatBytes(p.bytesDone),
            total: formatBytes(p.bytesTotal),
          })
        : formatBytes(p.bytesDone),
    );
    if (p.filesTotal > 0) {
      parts.push(i18n.t("transfers.files", { done: p.filesDone, total: p.filesTotal }));
    }
    if (item.status === "running" && p.bytesPerSec > 0) {
      parts.push(i18n.t("transfers.speed", { speed: formatBytes(p.bytesPerSec) }));
    }
    if (item.status === "running" && p.etaSecs !== null) {
      parts.push(i18n.t("transfers.eta", { time: formatEta(p.etaSecs) }));
    }
    return parts.join(" · ");
  }
  if (item.status === "running") return item.text || i18n.t("transfers.starting");
  return item.text;
}
