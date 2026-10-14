import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { peekOneShot, setOneShot, useAsAccount } from "@/features/accounts/switcher/oneShot";
import { retryOf, useTransferQueue } from "@/features/transfers/store";
import { initI18n } from "@/i18n";
import { commands, type RepoRefs } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { fetchAll, pull, push } from "./actions";

vi.mock("@/features/worktree/queries", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/worktree/queries")>()),
  invalidateRepo: vi.fn(),
}));

const REFS = {
  head: { branch: "main", oid: "a".repeat(40), detached: false, unborn: false },
  local: [
    {
      name: "main",
      oid: "a".repeat(40),
      isHead: true,
      upstream: "origin/main",
      upstreamGone: false,
      ahead: 1,
      behind: 0,
    },
  ],
  remotes: [{ name: "origin", url: "https://gitlab.acme.dev/a.git", branches: [] }],
  tags: [],
  stashes: [],
} as unknown as RepoRefs;

const AS_PERSONAL = { accountId: "acc-personal", identityId: null };

describe("palette remote actions and the one-shot override", () => {
  beforeAll(() => {
    initI18n("vi");
  });

  beforeEach(() => {
    vi.restoreAllMocks();
    queryClient.clear();
    useAsAccount.setState({ byRepo: {} });
    useTransferQueue.setState({ items: {}, order: [] });
    vi.spyOn(commands, "lfsStatus").mockResolvedValue({ usesLfs: false } as never);
  });

  it("push consumes a push override; fetch and pull leave it alone", async () => {
    const fetch = vi
      .spyOn(commands, "remoteFetch")
      .mockResolvedValue({ status: "done", value: { updatedRefs: 0 } });
    const pullCmd = vi.spyOn(commands, "remotePull").mockResolvedValue({
      status: "done",
      value: { result: "upToDate", conflicts: [], pending: null, head: null },
    } as never);
    const pushCmd = vi
      .spyOn(commands, "remotePush")
      .mockResolvedValue({ status: "done", value: { upToDate: false } } as never);
    setOneShot(1, { ...AS_PERSONAL, op: "push" });

    await fetchAll(1);
    await pull(1, "merge");
    expect(fetch).toHaveBeenCalledWith(1, expect.any(String), null, true, true, null);
    expect(pullCmd).toHaveBeenCalledWith(1, expect.any(String), null, null, "merge", null);
    expect(peekOneShot(1, "push")).not.toBeNull();

    await push(1, REFS, false);
    expect(pushCmd).toHaveBeenCalledWith(
      1,
      expect.any(String),
      expect.objectContaining({ remote: "origin", branch: "main" }),
      AS_PERSONAL,
    );
    expect(peekOneShot(1, "push")).toBeNull();
  });

  it("a fetch override is used by the next fetch (or pull)", async () => {
    const fetch = vi
      .spyOn(commands, "remoteFetch")
      .mockResolvedValue({ status: "done", value: { updatedRefs: 2 } });
    setOneShot(1, { ...AS_PERSONAL, op: "fetch" });
    await fetchAll(1);
    expect(fetch).toHaveBeenCalledWith(1, expect.any(String), null, true, true, AS_PERSONAL);
    expect(peekOneShot(1, "fetch")).toBeNull();
  });

  it("a rejected push consumes the override; Retry still pushes as that account", async () => {
    const pushCmd = vi
      .spyOn(commands, "remotePush")
      .mockResolvedValueOnce({
        status: "rejected",
        reason: "authFailed",
        message: "HTTP 403",
      } as never)
      .mockResolvedValueOnce({ status: "done", value: { upToDate: false } } as never);
    const shot = { ...AS_PERSONAL, op: "any" } as const;
    setOneShot(1, shot);

    await push(1, REFS, false);
    // The push reached the server: a later plain Push must use the repo's own account again.
    expect(peekOneShot(1, "push")).toBeNull();
    const failed = Object.values(useTransferQueue.getState().items)[0]!;
    expect(failed.status).toBe("failed");

    retryOf(failed.opId)?.();
    await vi.waitFor(() => expect(pushCmd).toHaveBeenCalledTimes(2));
    expect(pushCmd).toHaveBeenLastCalledWith(1, expect.any(String), expect.anything(), AS_PERSONAL);
    await vi.waitFor(() => expect(peekOneShot(1, "push")).toBeNull());
  });
});
