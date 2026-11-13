/**
 * "Stage all" of thousands of files (or a few huge ones) is one long `git add` with no output:
 * the panel looked frozen until it ended. Big batches are staged in chunks instead, with a
 * progress bar (files and bytes), the file being hashed and a Cancel that stops between
 * chunks (what is staged stays staged). The status refreshes once at the end, not after every
 * chunk.
 */
import i18n from "i18next";
import { create } from "zustand";
import { commands, errorMessage, type StatusEntry } from "@/ipc";
import { setQuiet } from "@/stores/quietRepos";
import { useUi } from "@/stores/ui";
import { invalidateWorktree } from "./queries";

/** Below these, one plain `git add` is fast enough. */
export const PROGRESS_MIN_FILES = 150;
export const PROGRESS_MIN_BYTES = 64 * 1024 * 1024;
/** Chunk limits: files per `git add`, bytes per `git add`, chunks per batch. */
export const CHUNK_FILES = 400;
export const CHUNK_BYTES = 32 * 1024 * 1024;
export const MAX_CHUNKS = 60;
/** What a file costs besides its bytes (process work per path), for the progress weight. */
const FILE_WEIGHT = 64 * 1024;

export const weightOf = (e: Pick<StatusEntry, "size">) => (e.size ?? 0) + FILE_WEIGHT;

/** Whether staging `entries` deserves the progress bar. */
export function needsProgress(entries: readonly Pick<StatusEntry, "size">[]): boolean {
  if (entries.length >= PROGRESS_MIN_FILES) return true;
  let bytes = 0;
  for (const e of entries) bytes += e.size ?? 0;
  return bytes >= PROGRESS_MIN_BYTES;
}

/**
 * Splits `entries` into chunks of at most {@link CHUNK_FILES} files and {@link CHUNK_BYTES}
 * bytes (a bigger file gets a chunk of its own), and never more than {@link MAX_CHUNKS} chunks
 * (each `git add` re-reads the index, so a huge batch uses bigger chunks). Order is kept.
 */
export function planChunks<T extends Pick<StatusEntry, "size">>(entries: readonly T[]): T[][] {
  const maxFiles = Math.max(CHUNK_FILES, Math.ceil(entries.length / MAX_CHUNKS));
  let total = 0;
  for (const e of entries) total += e.size ?? 0;
  const maxBytes = Math.max(CHUNK_BYTES, Math.ceil(total / MAX_CHUNKS));
  const chunks: T[][] = [];
  let current: T[] = [];
  let bytes = 0;
  for (const e of entries) {
    const size = e.size ?? 0;
    if (current.length && (current.length >= maxFiles || bytes + size > maxBytes)) {
      chunks.push(current);
      current = [];
      bytes = 0;
    }
    current.push(e);
    bytes += size;
  }
  if (current.length) chunks.push(current);
  return chunks;
}

export interface StageProgress {
  files: number;
  doneFiles: number;
  /** 0–100, weighted by size. */
  percent: number;
  /** Biggest file of the chunk being staged (what takes the time). */
  current: string | null;
  cancelling: boolean;
}

interface StageState {
  byRepo: Record<number, StageProgress>;
  set: (repoId: number, p: StageProgress | null) => void;
}

export const useStageProgress = create<StageState>()((set) => ({
  byRepo: {},
  set: (repoId, p) =>
    set((s) => {
      const byRepo = { ...s.byRepo };
      if (p) byRepo[repoId] = p;
      else delete byRepo[repoId];
      return { byRepo };
    }),
}));

export const useRepoStageProgress = (repoId: number) =>
  useStageProgress((s) => s.byRepo[repoId] ?? null);

const cancelled = new Set<number>();

/** Stops after the chunk being staged. */
export function cancelStaging(repoId: number) {
  const p = useStageProgress.getState().byRepo[repoId];
  if (!p) return;
  cancelled.add(repoId);
  useStageProgress.getState().set(repoId, { ...p, cancelling: true });
}

function biggest(chunk: readonly StatusEntry[]): string | null {
  let best: StatusEntry | null = null;
  for (const e of chunk) if (!best || (e.size ?? 0) > (best.size ?? 0)) best = e;
  return best?.path ?? null;
}

/**
 * Stages `entries` chunk by chunk, updating {@link useStageProgress}. Returns `false` when it
 * failed (a notice is shown) or was cancelled. One batch per repository at a time.
 */
export async function stageWithProgress(
  repoId: number,
  entries: readonly StatusEntry[],
): Promise<boolean> {
  const store = useStageProgress.getState();
  if (store.byRepo[repoId] || entries.length === 0) return false;
  const chunks = planChunks(entries);
  let totalWeight = 0;
  for (const e of entries) totalWeight += weightOf(e);
  let doneWeight = 0;
  let doneFiles = 0;
  cancelled.delete(repoId);
  setQuiet(repoId, true);
  const report = (current: string | null) =>
    useStageProgress.getState().set(repoId, {
      files: entries.length,
      doneFiles,
      percent: totalWeight ? Math.min(100, Math.floor((doneWeight / totalWeight) * 100)) : 0,
      current,
      cancelling: cancelled.has(repoId),
    });
  try {
    for (const chunk of chunks) {
      if (cancelled.has(repoId)) {
        useUi.getState().showNotice({
          kind: "info",
          repoId,
          message: i18n.t("wip.stageStopped", { done: doneFiles, total: entries.length }),
        });
        return false;
      }
      report(biggest(chunk));
      await commands.stageFiles(
        repoId,
        chunk.map((e) => e.path),
      );
      doneFiles += chunk.length;
      for (const e of chunk) doneWeight += weightOf(e);
    }
    report(null);
    return true;
  } catch (e) {
    useUi.getState().showNotice({
      kind: "error",
      repoId,
      message: errorMessage(e),
      hint: i18n.t("wip.stagePartly", { done: doneFiles, total: entries.length }),
      // Chunks keep the order: the rest is everything after the files already staged.
      retry: () => void stageWithProgress(repoId, entries.slice(doneFiles)),
    });
    return false;
  } finally {
    cancelled.delete(repoId);
    setQuiet(repoId, false);
    useStageProgress.getState().set(repoId, null);
    invalidateWorktree(repoId);
  }
}
