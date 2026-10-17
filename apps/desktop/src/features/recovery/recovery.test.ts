import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

const { removeLock, repoRefs, remotePull } = vi.hoisted(() => ({
  removeLock: vi.fn(),
  repoRefs: vi.fn(),
  remotePull: vi.fn(),
}));
vi.mock("@/ipc", async (orig) => {
  const real = await orig<typeof import("@/ipc")>();
  return {
    ...real,
    commands: {
      ...real.commands,
      repairRemoveLock: removeLock,
      repoRefs,
      remotePull,
    },
  };
});

import { initI18n } from "@/i18n";
import type { RepoRefs } from "@/ipc";
import { useStashUi } from "@/features/stash/store";
import { useUi } from "@/stores/ui";
import { detectProblem } from "./detect";
import { enhanceNotice } from "./notice";
import { pullSource } from "./pull";
import "./index";

beforeAll(() => {
  initI18n("vi");
});

afterEach(() => {
  useUi.getState().showNotice(null);
  useStashUi.getState().closeCreate();
  vi.clearAllMocks();
});

const LOCK_ERROR =
  "`git add -A` failed (exit status: 128): fatal: Unable to create '/Users/admin/Desktop/Game/.git/index.lock': File exists.\n\nAnother git process seems to be running in this repository, e.g. an editor opened by 'git commit'. Please make sure all processes are terminated then try again.";

describe("detectProblem", () => {
  it("finds the lock file git complains about", () => {
    expect(detectProblem(LOCK_ERROR)).toEqual({
      kind: "lock",
      path: "/Users/admin/Desktop/Game/.git/index.lock",
      file: "index.lock",
    });
    expect(
      detectProblem(
        "error: cannot lock ref 'refs/heads/main': Unable to create 'D:/w/r/.git/refs/heads/main.lock': File exists.",
      ),
    ).toMatchObject({ kind: "lock", file: "main.lock" });
    expect(detectProblem("error: could not lock config file .git/config: File exists")).toEqual({
      kind: "lock",
      path: ".git/config.lock",
      file: "config.lock",
    });
  });

  it("recognises the other fixable problems", () => {
    const cases: [string, string][] = [
      ["error: bad signature 0x00000000\nfatal: index file corrupt", "indexCorrupt"],
      ["error: unable to create file a/b/c.txt: Filename too long", "longPaths"],
      ["fatal: refusing to merge unrelated histories", "unrelatedHistories"],
      ["Author identity unknown\n*** Please tell me who you are.", "identityUnknown"],
      ["error: src refspec main does not match any", "noCommits"],
      ["error: RPC failed; HTTP 413 curl 22 The requested URL returned error: 413", "postBuffer"],
      ["error: unable to unlink old 'Assets/a.png': Permission denied", "fileBusy"],
    ];
    for (const [text, kind] of cases) expect(detectProblem(text)?.kind, text).toBe(kind);
    expect(
      detectProblem("fatal: detected dubious ownership in repository at 'D:/Work/game'"),
    ).toEqual({ kind: "dubiousOwnership", path: "D:/Work/game" });
    expect(
      detectProblem(
        "git refuses to use this repository because it is owned by another user (safe.directory): /Volumes/x/game",
      ),
    ).toEqual({ kind: "dubiousOwnership", path: "/Volumes/x/game" });
  });

  it("leaves unrelated errors alone", () => {
    expect(detectProblem("git@github.com: Permission denied (publickey).")).toBeNull();
    expect(detectProblem("fatal: not a git repository")).toBeNull();
  });
});

describe("enhanceNotice", () => {
  it("explains a stale index.lock and removes it, then retries", async () => {
    removeLock.mockResolvedValue(null);
    const retry = vi.fn();
    const notice = enhanceNotice({ kind: "error", message: LOCK_ERROR, repoId: 3, retry });
    expect(notice.message).toContain("index.lock");
    expect(notice.hint).toBeTruthy();
    expect(notice.detail).toContain("Unable to create");
    expect(notice.actions?.map((a) => a.label)).toEqual(["Xoá index.lock và thử lại", "Thử lại"]);
    expect(notice.durationMs).toBeGreaterThan(10_000);

    notice.actions?.[0]?.run();
    await vi.waitFor(() => expect(retry).toHaveBeenCalled());
    expect(removeLock).toHaveBeenCalledWith(3, "/Users/admin/Desktop/Game/.git/index.lock");
  });

  it("goes through showNotice for every error", () => {
    useUi.getState().showNotice({ kind: "error", message: LOCK_ERROR, repoId: 1 });
    expect(useUi.getState().notice?.actions?.[0]?.label).toBe("Xoá index.lock");
    useUi.getState().showNotice({ kind: "info", message: LOCK_ERROR });
    expect(useUi.getState().notice?.actions).toBeUndefined();
  });

  it("adds a fix to classified failures", () => {
    const notice = enhanceNotice({
      kind: "error",
      reason: "localChanges",
      message: "Local changes would be overwritten",
      repoId: 5,
    });
    expect(notice.actions?.map((a) => a.label)).toEqual(["Stash thay đổi trước"]);
    notice.actions?.[0]?.run();
    expect(useStashUi.getState().createFor).toBe(5);
  });

  it("keeps notices it cannot help with as they are", () => {
    const notice = { kind: "error" as const, message: "boom" };
    expect(enhanceNotice(notice)).toBe(notice);
  });
});

describe("pullSource", () => {
  const refs = (over: Partial<RepoRefs> = {}): RepoRefs => ({
    head: { branch: "main", oid: "a", detached: false, unborn: false },
    local: [],
    remotes: [
      { name: "upstream", url: null, branches: [] },
      { name: "origin", url: null, branches: [] },
    ],
    tags: [],
    stashes: [],
    ...over,
  });

  it("pulls the same-named branch of origin", () => {
    expect(pullSource(refs())).toEqual({ remote: "origin", branch: "main" });
    expect(pullSource(refs({ remotes: [{ name: "gitlab", url: null, branches: [] }] }))).toEqual({
      remote: "gitlab",
      branch: "main",
    });
  });

  it("needs a branch and a remote", () => {
    expect(pullSource(refs({ remotes: [] }))).toBeNull();
    expect(
      pullSource(refs({ head: { branch: null, oid: "a", detached: true, unborn: false } })),
    ).toBeNull();
  });
});
