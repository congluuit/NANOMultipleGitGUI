import { beforeEach, describe, expect, it, vi } from "vitest";
import type { LfsTransferProgress, TransferProgress } from "@/ipc";
import {
  MAX_FINISHED,
  retryOf,
  selectRepoRunning,
  setRetry,
  useTransferQueue,
  type QueueItem,
} from "./store";

const lfs = (over: Partial<LfsTransferProgress> = {}): LfsTransferProgress => ({
  repoId: 1,
  opId: "lfs-1",
  direction: "upload",
  filesDone: 1,
  filesTotal: 3,
  bytesDone: 10,
  bytesTotal: 30,
  currentFile: "a.bin",
  bytesPerSec: 5,
  etaSecs: 4,
  active: [],
  done: false,
  ...over,
});

const git = (over: Partial<TransferProgress> = {}): TransferProgress => ({
  repoId: 1,
  opId: "push-1",
  kind: "push",
  phase: "writing",
  label: "Writing objects",
  remote: false,
  percent: 45,
  current: 9,
  total: 20,
  bytes: "1.2 MiB",
  throughput: null,
  done: false,
  ...over,
});

const state = () => useTransferQueue.getState();
const get = (id: string): QueueItem => {
  const item = state().items[id];
  if (!item) throw new Error(`no item ${id}`);
  return item;
};

beforeEach(() => {
  useTransferQueue.setState({ items: {}, order: [] });
});

describe("transfer queue store", () => {
  it("tracks git progress for started operations only", () => {
    state().gitProgress(git());
    expect(state().order).toEqual([]);
    state().start({ opId: "push-1", repoId: 1, kind: "push", subject: "origin/main" });
    state().gitProgress(git());
    const item = get("push-1");
    expect(item.text).toBe("Writing objects 45% · 1.2 MiB");
    expect(item.percent).toBe(45);
    expect(item.status).toBe("running");
  });

  it("adopts LFS operations it did not start (reload) and keeps newest first", () => {
    state().start({ opId: "fetch-1", repoId: 2, kind: "fetch", subject: null });
    state().lfsProgress(lfs({ direction: "download", opId: "lfs-9" }));
    expect(state().order).toEqual(["lfs-9", "fetch-1"]);
    expect(get("lfs-9").kind).toBe("lfsDownload");
    expect(get("lfs-9").lfs?.bytesDone).toBe(10);
  });

  it("ignores progress after the end and keeps finished items for the popover", () => {
    state().start({ opId: "lfs-1", repoId: 1, kind: "lfsUpload", subject: null });
    state().lfsProgress(lfs());
    state().finish("lfs-1", "failed", { error: "boom" });
    state().lfsProgress(lfs({ bytesDone: 20 }));
    const item = get("lfs-1");
    expect(item.status).toBe("failed");
    expect(item.error).toBe("boom");
    expect(item.lfs?.bytesDone).toBe(10);
    expect(item.finishedAt).not.toBeNull();
  });

  it("caps the number of finished items", () => {
    for (let i = 0; i < MAX_FINISHED + 5; i++) {
      state().start({ opId: `op-${i}`, repoId: 1, kind: "fetch", subject: null });
      state().finish(`op-${i}`, "done");
    }
    state().start({ opId: "live", repoId: 1, kind: "push", subject: null });
    expect(state().order).toHaveLength(MAX_FINISHED + 1);
    expect(state().items["op-0"]).toBeUndefined();
    expect(get("live").status).toBe("running");
  });

  it("dismisses finished items and clears all finished ones", () => {
    state().start({ opId: "a", repoId: 1, kind: "fetch", subject: null });
    state().start({ opId: "b", repoId: 1, kind: "pull", subject: null });
    state().start({ opId: "c", repoId: 1, kind: "push", subject: null });
    state().dismiss("a");
    expect(state().items.a).toBeDefined();
    state().finish("a", "done");
    state().finish("b", "cancelled");
    state().dismiss("a");
    expect(state().items.a).toBeUndefined();
    state().clearFinished();
    expect(state().order).toEqual(["c"]);
  });

  it("remembers retry actions until the item is cleared", () => {
    const retry = vi.fn();
    state().start({ opId: "r", repoId: 1, kind: "lfsUpload", subject: null });
    setRetry("r", retry);
    state().finish("r", "failed");
    retryOf("r")?.();
    expect(retry).toHaveBeenCalledOnce();
    state().clearFinished();
    expect(retryOf("r")).toBeNull();
  });

  it("selectRepoRunning returns a stable reference", () => {
    state().start({ opId: "x", repoId: 3, kind: "push", subject: null });
    state().start({ opId: "y", repoId: 4, kind: "fetch", subject: null });
    const first = selectRepoRunning(state(), 3) as QueueItem;
    expect(first.opId).toBe("x");
    state().patch("y", { text: "changed" });
    expect(selectRepoRunning(state(), 3)).toBe(first);
    expect(selectRepoRunning(state(), 5)).toBeNull();
  });
});
