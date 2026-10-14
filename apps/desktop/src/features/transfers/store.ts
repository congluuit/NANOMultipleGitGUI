import { create } from "zustand";
import type { LfsFailure, LfsTransferProgress, TransferKind, TransferProgress } from "@/ipc";
import { progressText } from "@/features/remote/plan";

/** Kinds of transfers shown in the queue: git fetch/pull/push and LFS up/downloads. */
export type QueueKind = TransferKind | "lfsUpload" | "lfsDownload";
export type QueueStatus = "running" | "done" | "failed" | "cancelled";

export interface QueueItem {
  opId: string;
  repoId: number;
  kind: QueueKind;
  /** What is transferred (branch, file path…), shown next to the kind. */
  subject: string | null;
  status: QueueStatus;
  cancelling: boolean;
  /** Latest git progress line (fetch / pull / push), or a final message. */
  text: string;
  percent: number | null;
  /** Latest LFS progress. */
  lfs: LfsTransferProgress | null;
  /** Translated LFS failure. */
  failure: LfsFailure | null;
  /** Other failure text. */
  error: string | null;
  startedAt: number;
  finishedAt: number | null;
}

/** Finished items kept for the popover. */
export const MAX_FINISHED = 30;

export const isLfsKind = (kind: QueueKind) => kind === "lfsUpload" || kind === "lfsDownload";

interface QueueState {
  items: Record<string, QueueItem>;
  /** Newest first. */
  order: string[];
  start: (item: Pick<QueueItem, "opId" | "repoId" | "kind" | "subject">) => void;
  patch: (opId: string, patch: Partial<QueueItem>) => void;
  gitProgress: (p: TransferProgress) => void;
  lfsProgress: (p: LfsTransferProgress) => void;
  finish: (
    opId: string,
    status: Exclude<QueueStatus, "running">,
    extra?: Partial<Pick<QueueItem, "failure" | "error" | "text">>,
  ) => void;
  dismiss: (opId: string) => void;
  clearFinished: () => void;
}

/** Retry actions by operation id (kept outside the store: functions, not state). */
const retries = new Map<string, () => void>();

export function setRetry(opId: string, retry: (() => void) | null) {
  if (retry) retries.set(opId, retry);
  else retries.delete(opId);
}

export const retryOf = (opId: string) => retries.get(opId) ?? null;

function trimFinished(items: Record<string, QueueItem>, order: string[]) {
  let finished = 0;
  const keep: string[] = [];
  const next = { ...items };
  for (const id of order) {
    const item = next[id];
    if (!item) continue;
    if (item.status !== "running") {
      finished += 1;
      if (finished > MAX_FINISHED) {
        delete next[id];
        retries.delete(id);
        continue;
      }
    }
    keep.push(id);
  }
  return { items: next, order: keep };
}

const blank = (
  item: Pick<QueueItem, "opId" | "repoId" | "kind" | "subject">,
  now: number,
): QueueItem => ({
  ...item,
  status: "running",
  cancelling: false,
  text: "",
  percent: null,
  lfs: null,
  failure: null,
  error: null,
  startedAt: now,
  finishedAt: null,
});

export const useTransferQueue = create<QueueState>()((set) => ({
  items: {},
  order: [],
  start: (item) =>
    set((s) => {
      const items = { ...s.items, [item.opId]: blank(item, Date.now()) };
      const order = [item.opId, ...s.order.filter((id) => id !== item.opId)];
      return { items, order };
    }),
  patch: (opId, patch) =>
    set((s) => {
      const item = s.items[opId];
      return item ? { items: { ...s.items, [opId]: { ...item, ...patch } } } : s;
    }),
  gitProgress: (p) =>
    set((s) => {
      const item = s.items[p.opId];
      if (!item || item.status !== "running") return s;
      return {
        items: {
          ...s.items,
          [p.opId]: { ...item, text: progressText(p), percent: p.percent },
        },
      };
    }),
  lfsProgress: (p) =>
    set((s) => {
      const item = s.items[p.opId];
      if (item && item.status !== "running") return s;
      // Unknown operation: started before a reload, or by another code path.
      const base =
        item ??
        blank(
          {
            opId: p.opId,
            repoId: p.repoId,
            kind: p.direction === "upload" ? "lfsUpload" : "lfsDownload",
            subject: null,
          },
          Date.now(),
        );
      return {
        items: { ...s.items, [p.opId]: { ...base, lfs: p } },
        order: item ? s.order : [p.opId, ...s.order],
      };
    }),
  finish: (opId, status, extra) =>
    set((s) => {
      const item = s.items[opId];
      if (!item) return s;
      const done: QueueItem = {
        ...item,
        ...extra,
        status,
        cancelling: false,
        finishedAt: Date.now(),
      };
      if (status !== "failed" && status !== "cancelled") retries.delete(opId);
      return trimFinished({ ...s.items, [opId]: done }, s.order);
    }),
  dismiss: (opId) =>
    set((s) => {
      if (!s.items[opId] || s.items[opId].status === "running") return s;
      const items = { ...s.items };
      delete items[opId];
      retries.delete(opId);
      return { items, order: s.order.filter((id) => id !== opId) };
    }),
  clearFinished: () =>
    set((s) => {
      const items: Record<string, QueueItem> = {};
      for (const id of s.order) {
        const item = s.items[id];
        if (item?.status === "running") items[id] = item;
        else retries.delete(id);
      }
      return { items, order: s.order.filter((id) => items[id]) };
    }),
}));

/** First running transfer of a repository (stable reference for Zustand selectors). */
export function selectRepoRunning(s: Pick<QueueState, "items" | "order">, repoId: number) {
  for (const id of s.order) {
    const item = s.items[id];
    if (item && item.repoId === repoId && item.status === "running") return item;
  }
  return null;
}
