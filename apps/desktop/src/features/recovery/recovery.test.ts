import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

const { removeLock, repoRefs, remotePull, bindingSet } = vi.hoisted(() => ({
  removeLock: vi.fn(),
  repoRefs: vi.fn(),
  remotePull: vi.fn(),
  bindingSet: vi.fn(),
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
      repoBindingSet: bindingSet,
    },
  };
});

import { useAccountDialogs } from "@/features/accounts/dialogs";
import { accountKeys } from "@/features/accounts/queries";
import {
  makeContext,
  makeRemote,
  PERSONAL,
  PROVIDERS,
  WORK,
} from "@/features/accounts/switcher/testData";
import { repoKeys } from "@/features/repos/queries";
import { initI18n } from "@/i18n";
import type { FailureKind, RepoRefs } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useStashUi } from "@/features/stash/store";
import { useAiHelp } from "@/features/ai/help/store";
import { aiKeys } from "@/features/ai/queries";
import { useAiUi } from "@/features/ai/store";
import { useRefDialog } from "@/features/commits/store";
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

  it("offers to set up AI when it has no fix and AI is off", () => {
    const notice = enhanceNotice({ kind: "error", message: "boom" });
    expect(notice.actions?.map((a) => a.label)).toEqual(["Thiết lập AI để được hướng dẫn"]);
    notice.actions?.[0]?.run();
    expect(useAiUi.getState().open).toBe(true);
    useAiUi.getState().setOpen(false);
  });

  it("offers to ask the AI about any error once AI is set up, after asking consent", () => {
    queryClient.setQueryData(aiKeys.settings, {
      config: { enabled: true, provider: "ollama" },
      keys: { openai: false, anthropic: false, gemini: false, compatible: false },
    });
    queryClient.setQueryData(repoKeys.refs(5), refsOn("main"));
    const notice = enhanceNotice({
      kind: "error",
      reason: "localChanges",
      message: "Local changes would be overwritten",
      detail: "error: failed to push to https://me:ghp_abcdefghijklmnop1234@github.com/o/r",
      repoId: 5,
    });
    expect(notice.actions?.map((a) => a.label)).toEqual([
      "Stash thay đổi trước",
      "Hỏi AI cách sửa",
    ]);
    notice.actions?.[1]?.run();
    const consent = useAiHelp.getState().consent;
    expect(consent?.title).toBe("Local changes would be overwritten");
    expect(consent?.context).toMatch(/Current branch: main/);
    // The model may pick the app's own fix or everyday actions by id.
    expect(consent?.context).toMatch(
      /^\[fix1\] the app's own fix for this error: "Stash thay đổi trước"$/m,
    );
    expect(consent?.context).toMatch(/^\[fetch\] fetch: /m);
    expect(consent?.context).not.toMatch(/Hỏi AI cách sửa/);
    expect(consent?.actions?.map((a) => a.id)).toContain("stash");
    expect(consent?.context).not.toMatch(/ghp_/);
    expect(useAiHelp.getState().open).toBe(false);
    useAiHelp.setState({ consent: null });
    queryClient.removeQueries({ queryKey: aiKeys.settings });
    queryClient.removeQueries({ queryKey: repoKeys.refs(5) });
  });

  it("offers the fix of each kind of refusal", () => {
    queryClient.setQueryData(repoKeys.refs(5), refsOn("feature"));
    const labels = (reason: FailureKind) =>
      enhanceNotice({ kind: "error", reason, message: reason, repoId: 5 }).actions?.map(
        (a) => a.label,
      );
    expect(labels("noUpstream")).toEqual(["Đẩy feature lên origin và theo dõi"]);
    expect(labels("connectionFailed")).toEqual(["Thiết lập AI để được hướng dẫn"]);
    expect(labels("staleLease")).toEqual(["Tải về để xem thay đổi mới"]);
    expect(labels("detachedHead")).toEqual(["Tạo nhánh tại đây"]);
    expect(labels("protectedBranch")).toEqual(["Chuyển commit sang nhánh mới"]);
    expect(labels("unmergedFiles")).toEqual(["Mở file xung đột", "Hủy thao tác đang dở"]);
    expect(labels("secretDetected")).toEqual(["Hoàn tác commit cuối (giữ thay đổi)"]);
    expect(labels("permissionDenied")).toEqual(["Tạo bản fork để push"]);
    // Never a plain Retry for a file over the server's limit: move it to LFS or take it out.
    expect(labels("fileTooLarge")).toEqual([
      "Chuyển file nặng sang LFS rồi push lại",
      "Bỏ file nặng khỏi commit (giữ trên máy)",
    ]);

    enhanceNotice({
      kind: "error",
      reason: "protectedBranch",
      message: "x",
      repoId: 5,
    }).actions?.[0]?.run();
    expect(useRefDialog.getState().dialog).toMatchObject({
      kind: "createBranch",
      repoId: 5,
      start: "abcdef1234567890",
      label: "abcdef1",
    });
    useRefDialog.getState().open(null);
    queryClient.removeQueries({ queryKey: repoKeys.refs(5) });
  });

  it("retries a connection failure when it knows how", () => {
    const retry = vi.fn();
    const notice = enhanceNotice({
      kind: "error",
      reason: "connectionFailed",
      message: "x",
      repoId: 5,
      retry,
    });
    expect(notice.actions?.[0]?.label).toBe("Thử lại");
    notice.actions?.[0]?.run();
    expect(retry).toHaveBeenCalled();
  });
});

function refsOn(branch: string): RepoRefs {
  return {
    head: { branch, oid: "abcdef1234567890", detached: false, unborn: false },
    local: [],
    remotes: [{ name: "origin", url: "https://github.com/o/r.git", branches: [] }],
    tags: [],
    stashes: [],
  } as unknown as RepoRefs;
}

describe("remote bound to an account of another server", () => {
  const REFUSED =
    "`git push --progress origin main` failed (exit code 128): nanomgg-credential: host `github.com` does not belong to this account (expected `gitlab.acme.dev`)";
  // `origin` (github.com) bound to Work (GitLab acme); `gitlab` is Work's own remote.
  const context = makeContext({
    perRemote: [
      makeRemote({
        remote: "origin",
        url: "https://github.com/me/api.git",
        host: "github.com",
        inherited: false,
      }),
      makeRemote({ remote: "gitlab" }),
    ],
  });
  const seed = (accounts = [WORK, PERSONAL]) => {
    queryClient.setQueryData(accountKeys.providers, PROVIDERS);
    queryClient.setQueryData(accountKeys.list, accounts);
    queryClient.setQueryData(accountKeys.repoContext(7), context);
    queryClient.setQueryData(repoKeys.refs(7), {
      head: { branch: "main", oid: "a", detached: false, unborn: false },
    } as RepoRefs);
  };
  afterEach(() => queryClient.clear());

  it("names the remote and the account and offers every way out", async () => {
    seed();
    const retry = vi.fn();
    bindingSet.mockResolvedValue(context);
    const notice = enhanceNotice({ kind: "error", message: REFUSED, repoId: 7, retry });
    expect(notice.message).toBe("origin (github.com) đang gắn nhầm tài khoản Công ty");
    expect(notice.hint).toContain("gitlab (gitlab.acme.dev)");
    expect(notice.actions?.map((a) => a.label)).toEqual([
      "Push lên gitlab (gitlab.acme.dev) bằng Công ty",
      "Dùng Cá nhân cho origin",
    ]);
    notice.actions![1]!.run();
    await vi.waitFor(() => expect(retry).toHaveBeenCalled());
    expect(bindingSet).toHaveBeenCalledWith(
      7,
      expect.objectContaining({ accountId: PERSONAL.id, remote: "origin" }),
    );
  });

  it("offers to sign in when nobody is signed in to that server", () => {
    seed([WORK]);
    const notice = enhanceNotice({ kind: "error", message: REFUSED, repoId: 7 });
    const signIn = notice.actions?.find((a) => a.label === "Đăng nhập GitHub");
    expect(signIn).toBeDefined();
    signIn!.run();
    expect(useAccountDialogs.getState().manager?.signIn).toEqual({
      kind: "github",
      baseUrl: "https://github.com",
    });
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
