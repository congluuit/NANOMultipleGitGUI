import { afterEach, describe, expect, it, vi } from "vitest";
import { commands, type StatusEntry } from "@/ipc";
import { isQuiet } from "@/stores/quietRepos";
import {
  CHUNK_BYTES,
  CHUNK_FILES,
  MAX_CHUNKS,
  cancelStaging,
  needsProgress,
  planChunks,
  stageWithProgress,
  useStageProgress,
} from "./stageProgress";

const MB = 1024 * 1024;

function entry(path: string, size: number | null = 100): StatusEntry {
  return { path, oldPath: null, kind: "untracked", conflict: null, submodule: false, size };
}

const many = (n: number, size = 100) =>
  Array.from({ length: n }, (_, i) => entry(`f${i}.txt`, size));

describe("planChunks", () => {
  it("splits by file count and by bytes, a huge file alone", () => {
    const chunks = planChunks([
      ...many(CHUNK_FILES + 10),
      entry("big.psd", 500 * MB),
      entry("after.txt"),
    ]);
    expect(chunks.map((c) => c.length)).toEqual([CHUNK_FILES, 10, 1, 1]);
    expect(chunks[2]![0]!.path).toBe("big.psd");
    // Order is kept.
    expect(chunks.flat().map((e) => e.path)[CHUNK_FILES]).toBe(`f${CHUNK_FILES}.txt`);
  });

  it("never makes more than MAX_CHUNKS chunks of small files", () => {
    const chunks = planChunks(many(100_000));
    expect(chunks.length).toBeLessThanOrEqual(MAX_CHUNKS);
    expect(chunks.flat()).toHaveLength(100_000);
  });

  it("groups mid-size files up to the byte budget", () => {
    const chunks = planChunks(many(10, 10 * MB));
    expect(chunks.every((c) => c.reduce((s, e) => s + (e.size ?? 0), 0) <= CHUNK_BYTES)).toBe(true);
    expect(chunks.length).toBeGreaterThan(1);
  });

  it("asks for progress only for big batches", () => {
    expect(needsProgress(many(10))).toBe(false);
    expect(needsProgress(many(200))).toBe(true);
    expect(needsProgress([entry("a.mov", 80 * MB)])).toBe(true);
    expect(needsProgress([entry("gone", null)])).toBe(false);
  });
});

describe("stageWithProgress", () => {
  afterEach(() => vi.restoreAllMocks());

  it("stages chunk by chunk, reports progress and stays quiet meanwhile", async () => {
    const seen: number[] = [];
    const quiet: boolean[] = [];
    const stage = vi.spyOn(commands, "stageFiles").mockImplementation(async () => {
      seen.push(useStageProgress.getState().byRepo[3]?.doneFiles ?? -1);
      quiet.push(isQuiet(3));
      return null;
    });
    const ok = await stageWithProgress(3, many(CHUNK_FILES * 2 + 5));
    expect(ok).toBe(true);
    expect(stage).toHaveBeenCalledTimes(3);
    expect(seen).toEqual([0, CHUNK_FILES, CHUNK_FILES * 2]);
    expect(quiet).toEqual([true, true, true]);
    expect(isQuiet(3)).toBe(false);
    expect(useStageProgress.getState().byRepo[3]).toBeUndefined();
  });

  it("stops between chunks when cancelled", async () => {
    const stage = vi.spyOn(commands, "stageFiles").mockImplementation(async () => {
      cancelStaging(4);
      return null;
    });
    const ok = await stageWithProgress(4, many(CHUNK_FILES * 3));
    expect(ok).toBe(false);
    expect(stage).toHaveBeenCalledTimes(1);
    expect(useStageProgress.getState().byRepo[4]).toBeUndefined();
  });
});
