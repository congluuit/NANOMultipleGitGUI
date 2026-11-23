/**
 * "Track with LFS" of the Large File Guard used to be a few long git commands behind a spinner
 * in a modal dialog: with hundreds of images the app looked frozen and no other project could be
 * opened meanwhile. It now runs as a job: the patterns are added one by one, then the files are
 * re-staged as LFS pointers in small chunks (the clean filter hashes and copies every byte), with
 * progress in a store (the dialog and the commit box read it) and in the transfer queue of the
 * status bar, so the dialog can be sent to the background. Cancel stops between chunks; what is
 * already staged stays staged and nothing is committed.
 */
import i18n from "i18next";
import { create } from "zustand";
import { newOpId } from "@/features/remote/plan";
import { formatEta } from "@/features/transfers/format";
import { setCanceller, setRetry, useTransferQueue } from "@/features/transfers/store";
import { invalidateWorktree } from "@/features/worktree/queries";
import { planChunks, type ChunkLimits } from "@/features/worktree/stageProgress";
import { commands, errorMessage } from "@/ipc";
import { formatBytes } from "@/lib/format";
import { setQuiet } from "@/stores/quietRepos";
import { useUi } from "@/stores/ui";
import { invalidateLfs } from "./queries";

export interface TrackFile {
  path: string;
  size: number | null;
}

/** Smaller chunks than "Stage all": the bar moves every few seconds even on slow disks. */
export const TRACK_CHUNKS: ChunkLimits = { files: 100, bytes: 16 * 1024 * 1024, chunks: 40 };
/** What a file costs besides its bytes (process work per path), for the progress weight. */
const FILE_WEIGHT = 64 * 1024;
/** Share of the bar for adding the patterns. */
const PATTERN_SHARE = 0.05;

export type TrackPhase = "patterns" | "convert";

export interface TrackProgress {
  opId: string;
  phase: TrackPhase;
  patterns: number;
  donePatterns: number;
  files: number;
  doneFiles: number;
  bytes: number;
  doneBytes: number;
  /** 0–100, weighted by size. */
  percent: number;
  /** Biggest file of the chunk being converted (what takes the time). */
  current: string | null;
  startedAt: number;
  /** Seconds left from the speed so far; `null` until a chunk has finished. */
  etaSecs: number | null;
  cancelling: boolean;
}

interface TrackState {
  byRepo: Record<number, TrackProgress>;
  set: (repoId: number, p: TrackProgress | null) => void;
}

export const useLfsTrackJobs = create<TrackState>()((set) => ({
  byRepo: {},
  set: (repoId, p) =>
    set((s) => {
      const byRepo = { ...s.byRepo };
      if (p) byRepo[repoId] = p;
      else delete byRepo[repoId];
      return { byRepo };
    }),
}));

export const useLfsTrackJob = (repoId: number) => useLfsTrackJobs((s) => s.byRepo[repoId] ?? null);

const cancelled = new Set<number>();

/** Stops after the chunk being converted. */
export function cancelLfsTrack(repoId: number) {
  const p = useLfsTrackJobs.getState().byRepo[repoId];
  if (!p) return;
  cancelled.add(repoId);
  useLfsTrackJobs.getState().set(repoId, { ...p, cancelling: true });
  useTransferQueue.getState().patch(p.opId, { cancelling: true });
}

const weightOf = (f: TrackFile) => (f.size ?? 0) + FILE_WEIGHT;

function biggest(chunk: readonly TrackFile[]): string | null {
  let best: TrackFile | null = null;
  for (const f of chunk) if (!best || (f.size ?? 0) > (best.size ?? 0)) best = f;
  return best?.path ?? null;
}

/** One line for the transfer queue: phase, counts, sizes, time left. */
export function progressText(p: TrackProgress): string {
  if (p.phase === "patterns") {
    return i18n.t("lfsTrack.phasePatterns", { done: p.donePatterns, total: p.patterns });
  }
  const parts = [
    i18n.t("lfsTrack.phaseConvert", { done: p.doneFiles, total: p.files }),
    i18n.t("transfers.bytes", { done: formatBytes(p.doneBytes), total: formatBytes(p.bytes) }),
  ];
  if (p.etaSecs !== null) parts.push(i18n.t("transfers.eta", { time: formatEta(p.etaSecs) }));
  return parts.join(" · ");
}

export type TrackResult = "done" | "cancelled" | "failed" | "busy";

/**
 * Tracks `patterns` with LFS, then re-stages `files` as pointers chunk by chunk, and runs
 * `then` (the held-back commit) when everything worked. One job per repository at a time.
 */
export async function runLfsTrack(
  repoId: number,
  patterns: readonly string[],
  files: readonly TrackFile[],
  then?: () => void,
): Promise<TrackResult> {
  if (useLfsTrackJobs.getState().byRepo[repoId]) return "busy";
  const opId = newOpId("lfsTrack");
  const chunks = planChunks(files, TRACK_CHUNKS);
  let totalWeight = 0;
  let totalBytes = 0;
  for (const f of files) {
    totalWeight += weightOf(f);
    totalBytes += f.size ?? 0;
  }
  const patternShare = patterns.length ? PATTERN_SHARE : 0;
  const startedAt = Date.now();
  let convertStart = startedAt;
  let donePatterns = 0;
  let doneFiles = 0;
  let doneBytes = 0;
  let doneWeight = 0;

  const queue = useTransferQueue.getState();
  queue.start({
    opId,
    repoId,
    kind: "lfsTrack",
    subject: i18n.t("lfsTrack.subject", { count: files.length }),
  });
  setCanceller(opId, () => cancelLfsTrack(repoId));
  cancelled.delete(repoId);
  setQuiet(repoId, true);

  const report = (phase: TrackPhase, current: string | null) => {
    const fraction =
      patternShare * (patterns.length ? donePatterns / patterns.length : 0) +
      (1 - patternShare) * (totalWeight ? doneWeight / totalWeight : 0);
    const elapsed = (Date.now() - convertStart) / 1000;
    const etaSecs =
      phase === "convert" && doneWeight > 0 && doneWeight < totalWeight
        ? (elapsed * (totalWeight - doneWeight)) / doneWeight
        : null;
    const p: TrackProgress = {
      opId,
      phase,
      patterns: patterns.length,
      donePatterns,
      files: files.length,
      doneFiles,
      bytes: totalBytes,
      doneBytes,
      percent: Math.min(100, Math.floor(fraction * 100)),
      current,
      startedAt,
      etaSecs,
      cancelling: cancelled.has(repoId),
    };
    useLfsTrackJobs.getState().set(repoId, p);
    useTransferQueue.getState().patch(opId, { percent: p.percent, text: progressText(p) });
  };

  const stopped = () => {
    const text = i18n.t("lfsTrack.stopped", { done: doneFiles, total: files.length });
    useTransferQueue.getState().finish(opId, "cancelled", { text });
    useUi.getState().showNotice({ kind: "info", repoId, message: text });
    return "cancelled" as const;
  };

  let result: TrackResult = "failed";
  try {
    for (const pattern of patterns) {
      if (cancelled.has(repoId)) return (result = stopped());
      report("patterns", pattern);
      await commands.lfsTrack(repoId, pattern, false);
      donePatterns += 1;
    }
    convertStart = Date.now();
    // No file to convert: still stage `.gitattributes`.
    for (const chunk of chunks.length ? chunks : [[]]) {
      if (cancelled.has(repoId)) return (result = stopped());
      report("convert", biggest(chunk));
      await commands.lfsRestage(
        repoId,
        chunk.map((f) => f.path),
      );
      doneFiles += chunk.length;
      for (const f of chunk) {
        doneWeight += weightOf(f);
        doneBytes += f.size ?? 0;
      }
    }
    report("convert", null);
    useTransferQueue
      .getState()
      .finish(opId, "done", { text: i18n.t("lfsTrack.done", { count: files.length }) });
    result = "done";
  } catch (e) {
    const message = errorMessage(e);
    useTransferQueue.getState().finish(opId, "failed", { error: message });
    // Patterns and chunks keep their order: the rest is everything after what is done.
    const retry = () =>
      void runLfsTrack(repoId, patterns.slice(donePatterns), files.slice(doneFiles), then);
    setRetry(opId, retry);
    useUi.getState().showNotice({
      kind: "error",
      repoId,
      message: i18n.t("guard.trackFailed", { message }),
      hint: doneFiles
        ? i18n.t("lfsTrack.partly", { done: doneFiles, total: files.length })
        : undefined,
      retry,
    });
  } finally {
    cancelled.delete(repoId);
    setCanceller(opId, null);
    setQuiet(repoId, false);
    useLfsTrackJobs.getState().set(repoId, null);
    invalidateLfs(repoId);
    invalidateWorktree(repoId);
  }
  if (result === "done") then?.();
  return result;
}
