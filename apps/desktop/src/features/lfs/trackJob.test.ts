import { afterEach, describe, expect, it, vi } from "vitest";
import { cancelQueued } from "@/features/transfers/actions";
import { retryOf, useTransferQueue } from "@/features/transfers/store";
import { commands } from "@/ipc";
import { isQuiet } from "@/stores/quietRepos";
import { cancelLfsTrack, runLfsTrack, TRACK_CHUNKS, useLfsTrackJobs } from "./trackJob";

const MB = 1024 * 1024;

const files = (n: number, size = 2 * MB) =>
  Array.from({ length: n }, (_, i) => ({ path: `art/f${i}.png`, size }));

const queueItem = (repoId: number) => {
  const s = useTransferQueue.getState();
  return s.order
    .map((id) => s.items[id]!)
    .find((i) => i.repoId === repoId && i.kind === "lfsTrack");
};

describe("runLfsTrack", () => {
  afterEach(() => vi.restoreAllMocks());

  it("tracks patterns, converts in chunks with progress, then commits", async () => {
    const phases: string[] = [];
    const track = vi.spyOn(commands, "lfsTrack").mockImplementation(async () => {
      phases.push(useLfsTrackJobs.getState().byRepo[1]!.phase);
      return true;
    });
    const done: number[] = [];
    const restage = vi.spyOn(commands, "lfsRestage").mockImplementation(async () => {
      const p = useLfsTrackJobs.getState().byRepo[1]!;
      expect(isQuiet(1)).toBe(true);
      expect(queueItem(1)?.status).toBe("running");
      done.push(p.doneFiles);
      return null;
    });
    const then = vi.fn();
    // 16 MiB chunks of 2 MiB files: 8 files per chunk.
    const result = await runLfsTrack(1, ["*.png", "*.ogg"], files(20), then);
    expect(result).toBe("done");
    expect(track).toHaveBeenCalledTimes(2);
    expect(phases).toEqual(["patterns", "patterns"]);
    expect(restage).toHaveBeenCalledTimes(3);
    expect(done).toEqual([0, 8, 16]);
    expect(then).toHaveBeenCalledOnce();
    expect(useLfsTrackJobs.getState().byRepo[1]).toBeUndefined();
    expect(isQuiet(1)).toBe(false);
    expect(queueItem(1)?.status).toBe("done");
    expect(TRACK_CHUNKS.bytes).toBe(16 * MB);
  });

  it("stops between chunks from the queue's Cancel and does not commit", async () => {
    vi.spyOn(commands, "lfsTrack").mockResolvedValue(true);
    const restage = vi.spyOn(commands, "lfsRestage").mockImplementation(async () => {
      await cancelQueued(queueItem(2)!.opId);
      return null;
    });
    const then = vi.fn();
    const result = await runLfsTrack(2, ["*.png"], files(30), then);
    expect(result).toBe("cancelled");
    expect(restage).toHaveBeenCalledTimes(1);
    expect(then).not.toHaveBeenCalled();
    expect(queueItem(2)?.status).toBe("cancelled");
    expect(useLfsTrackJobs.getState().byRepo[2]).toBeUndefined();
  });

  it("fails with a Retry that goes on with the remaining files", async () => {
    vi.spyOn(commands, "lfsTrack").mockResolvedValue(true);
    let calls = 0;
    const seen: string[][] = [];
    vi.spyOn(commands, "lfsRestage").mockImplementation(async (_repo, paths) => {
      calls += 1;
      if (calls === 2) throw new Error("disk full");
      seen.push(paths);
      return null;
    });
    const then = vi.fn();
    expect(await runLfsTrack(3, ["*.png"], files(10), then)).toBe("failed");
    expect(then).not.toHaveBeenCalled();
    const item = queueItem(3)!;
    expect(item.status).toBe("failed");
    expect(item.error).toContain("disk full");

    const track = vi.spyOn(commands, "lfsTrack");
    track.mockClear();
    retryOf(item.opId)!();
    await vi.waitFor(() => expect(then).toHaveBeenCalledOnce());
    // The pattern was already added; only the files after the first chunk are converted.
    expect(track).not.toHaveBeenCalled();
    expect(seen.flat()).toHaveLength(10);
  });

  it("runs one job per repository", async () => {
    let release = () => {};
    vi.spyOn(commands, "lfsTrack").mockResolvedValue(true);
    vi.spyOn(commands, "lfsRestage").mockImplementation(
      () => new Promise((r) => (release = () => r(null))),
    );
    const first = runLfsTrack(4, [], files(1));
    await vi.waitFor(() => expect(useLfsTrackJobs.getState().byRepo[4]).toBeDefined());
    expect(await runLfsTrack(4, [], files(1))).toBe("busy");
    cancelLfsTrack(4);
    expect(useLfsTrackJobs.getState().byRepo[4]?.cancelling).toBe(true);
    release();
    expect(await first).toBe("done");
  });
});
