import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { peekOneShot, setOneShot, takeOneShot } from "@/features/accounts/switcher/oneShot";
import { initI18n } from "@/i18n";
import type { LfsOutcome, TransferSummary } from "@/ipc";
import { runLfsPull, runLfsPush } from "./actions";
import { retryOf, useTransferQueue } from "./store";

const ipc = vi.hoisted(() => ({
  lfsPush: vi.fn<() => Promise<LfsOutcome<TransferSummary>>>(),
  lfsPull: vi.fn<() => Promise<LfsOutcome<TransferSummary>>>(),
}));
vi.mock("@/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/ipc")>();
  return { ...actual, commands: { ...actual.commands, ...ipc } };
});
vi.mock("@/features/worktree/queries", () => ({ invalidateRepo: vi.fn() }));

const MB = 1024 * 1024;
const done = (files: number, bytes: number): LfsOutcome<TransferSummary> => ({
  status: "done",
  value: { files, bytes, elapsedMs: 10 },
});

beforeAll(() => {
  initI18n("vi");
});

beforeEach(() => {
  useTransferQueue.setState({ items: {}, order: [] });
  setOneShot(1, null);
  ipc.lfsPush.mockReset();
  ipc.lfsPull.mockReset();
});

const only = () => {
  const s = useTransferQueue.getState();
  const item = s.items[s.order[0] ?? ""];
  if (!item) throw new Error("no queue item");
  return item;
};

describe("LFS queue actions", () => {
  it("uploads, then continues with the git push", async () => {
    ipc.lfsPush.mockResolvedValue(done(3, 12 * MB));
    const then = vi.fn();
    const summary = await runLfsPush(1, "origin", "main", then);
    expect(summary?.files).toBe(3);
    expect(then).toHaveBeenCalledOnce();
    expect(ipc.lfsPush).toHaveBeenCalledWith(
      1,
      expect.stringMatching(/^lfsUpload-/),
      "origin",
      "main",
      null,
      null,
    );
    const item = only();
    expect(item.status).toBe("done");
    expect(item.subject).toBe("origin/main");
    expect(item.text).toBe("Đã upload 3 file LFS (12 MB).");
  });

  it("keeps a failure with a Retry that resumes the whole flow", async () => {
    ipc.lfsPush.mockResolvedValueOnce({
      status: "failed",
      failure: {
        kind: "network",
        message: "i/o timeout",
        retryable: true,
        billingUrl: null,
        settingsUrl: null,
        limitBytes: null,
        object: null,
      },
    });
    const then = vi.fn();
    expect(await runLfsPush(1, "origin", "main", then)).toBeUndefined();
    expect(then).not.toHaveBeenCalled();
    const failed = only();
    expect(failed.status).toBe("failed");
    expect(failed.failure?.kind).toBe("network");

    ipc.lfsPush.mockResolvedValueOnce(done(0, 0));
    retryOf(failed.opId)?.();
    await vi.waitFor(() => expect(then).toHaveBeenCalledOnce());
    expect(only().text).toBe("Server đã có đủ file LFS.");
  });

  it("downloads one pointer file and reports cancellation", async () => {
    ipc.lfsPull.mockResolvedValueOnce({ status: "cancelled" });
    await runLfsPull(2, "Assets/intro.mp4");
    expect(ipc.lfsPull).toHaveBeenCalledWith(
      2,
      expect.any(String),
      "Assets/intro.mp4",
      false,
      null,
      null,
    );
    expect(only().status).toBe("cancelled");
    expect(only().kind).toBe("lfsDownload");
  });

  it("uploads as a taken one-shot override and gives it back when the upload fails", async () => {
    setOneShot(1, { accountId: "acc-personal", identityId: null, op: "push" });
    const shot = takeOneShot(1, "push");
    ipc.lfsPush.mockResolvedValueOnce({ status: "cancelled" });
    expect(await runLfsPush(1, "origin", "main", undefined, null, shot)).toBeUndefined();
    expect(ipc.lfsPush).toHaveBeenCalledWith(1, expect.any(String), "origin", "main", null, {
      accountId: "acc-personal",
      identityId: null,
    });
    // Cancelled → the override is pending again for the next push.
    expect(peekOneShot(1, "push")).toEqual(shot);

    // Retry re-runs as the same account and consumes it on success.
    ipc.lfsPush.mockResolvedValueOnce(done(1, MB));
    retryOf(only().opId)?.();
    await vi.waitFor(() => expect(ipc.lfsPush).toHaveBeenCalledTimes(2));
    expect(ipc.lfsPush).toHaveBeenLastCalledWith(1, expect.any(String), "origin", "main", null, {
      accountId: "acc-personal",
      identityId: null,
    });
    await vi.waitFor(() => expect(peekOneShot(1, "push")).toBeNull());
  });
});
