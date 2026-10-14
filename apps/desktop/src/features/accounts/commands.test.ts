import i18n from "i18next";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  collectCommands,
  currentProviders,
  type PaletteCommand,
  type PaletteContext,
} from "@/features/command-palette/registry";
import "@/features/command-palette";
import { initI18n } from "@/i18n";
import { commands, type RepoRefs } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";
import { accountCommands } from "./commands";
import { useAccountDialogs } from "./dialogs";
import { peekOneShot, useAsAccount } from "./switcher/oneShot";
import {
  makeContext,
  OSS,
  PERSONAL,
  PERSONAL_ALT,
  PERSONAL_MAIN,
  seedAccounts,
  unboundContext,
  WORK,
  WORK_ID,
} from "./switcher/testData";

const repo = { id: 1, path: "D:/Work/api", name: "api", missing: false };

function ctx(over: Partial<PaletteContext> = {}): PaletteContext {
  return {
    t: i18n.t,
    language: "vi",
    theme: "light",
    isMac: false,
    tabs: [repo],
    activeId: 1,
    repo,
    refs: null,
    status: null,
    busy: false,
    recentRepos: [],
    ...over,
  };
}

const byId = (list: PaletteCommand[], id: string) => {
  const cmd = list.find((c) => c.id === id);
  if (!cmd) throw new Error(`no command ${id}`);
  return cmd;
};

describe("accounts palette commands", () => {
  beforeEach(() => {
    initI18n("vi");
    queryClient.clear();
    useUi.setState({ notice: null });
    useAsAccount.setState({ byRepo: {} });
    useAccountDialogs.setState({ manager: null, clone: null, importWizard: false });
  });
  afterEach(() => vi.restoreAllMocks());

  it("replaces the built-in placeholders", () => {
    const list = collectCommands(currentProviders(), ctx());
    const switchCmd = byId(list, "accounts.switch");
    expect(switchCmd.hint).toBe("chưa có tài khoản");
    expect(switchCmd.children).toBeDefined();
    expect(list.map((c) => c.id)).toEqual(
      expect.arrayContaining(["accounts.manage", "accounts.add", "accounts.import"]),
    );
  });

  it("disables the account pickers without accounts but keeps manage/add/import", () => {
    const c = ctx();
    const list = accountCommands(c);
    for (const id of [
      "accounts.switch",
      "accounts.commitAs",
      "accounts.pushAs",
      "accounts.cloneAs",
    ]) {
      expect(byId(list, id).when?.(c), id).toBe(false);
    }
    for (const id of ["accounts.manage", "accounts.add", "accounts.import"]) {
      expect(byId(list, id).when, id).toBeUndefined();
    }
  });

  it("Switch account… lists accounts × identities, marks the current one and binds", async () => {
    seedAccounts(queryClient, { contexts: [makeContext()] });
    const set = vi.spyOn(commands, "repoBindingSet").mockResolvedValue(makeContext());
    const c = ctx();
    const cmd = byId(accountCommands(c), "accounts.switch");
    expect(cmd.when?.(c)).toBe(true);
    expect(cmd.hint).toBe("đang dùng: Công ty");
    const kids = cmd.children!(c);
    expect(kids.map((k) => k.title)).toEqual([
      "Công ty · Hoang Nguyen <hoang@acme.dev>",
      `Cá nhân · ${PERSONAL_MAIN.name} <${PERSONAL_MAIN.email}>`,
      `Cá nhân · ${PERSONAL_ALT.name} <${PERSONAL_ALT.email}>`,
      "Hoang OSS",
    ]);
    expect(kids.map((k) => k.checked)).toEqual([true, false, false, false]);
    expect(kids.map((k) => k.shortcut)).toEqual([
      "Ctrl+Alt+1",
      "Ctrl+Alt+2",
      undefined,
      "Ctrl+Alt+3",
    ]);
    expect(kids[0]!.hint).toBe("GitLab acme");

    await kids[2]!.run!(c);
    expect(set).toHaveBeenCalledWith(1, {
      remote: null,
      accountId: PERSONAL.id,
      identityId: PERSONAL_ALT.id,
      locked: false,
      source: "manual",
    });
  });

  it("Commit as… sets a one-time override for the next commit", () => {
    seedAccounts(queryClient, { contexts: [makeContext()] });
    const c = ctx();
    const kids = byId(accountCommands(c), "accounts.commitAs").children!(c);
    void kids[3]!.run!(c);
    expect(peekOneShot(1, "commit")).toEqual({ accountId: OSS.id, identityId: null, op: "commit" });
    expect(peekOneShot(1, "push")).toBeNull();
  });

  it("Push as… sets the override, then pushes when there is something to push", async () => {
    seedAccounts(queryClient, { contexts: [makeContext()] });
    const c = ctx();
    const kids = byId(accountCommands(c), "accounts.pushAs").children!(c);
    await kids[1]!.run!(c);
    expect(peekOneShot(1, "push")).toEqual({
      accountId: PERSONAL.id,
      identityId: PERSONAL_MAIN.id,
      op: "push",
    });
    expect(useUi.getState().notice?.message).toBe(
      "Đã chọn tài khoản cho lần push kế tiếp — bấm Push khi sẵn sàng",
    );

    const refs = {
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
    const push = vi.spyOn(commands, "remotePush").mockResolvedValue({
      status: "done",
      value: { upToDate: false },
    } as never);
    vi.spyOn(commands, "lfsStatus").mockResolvedValue({ usesLfs: false } as never);
    const withRefs = ctx({ refs });
    const pushKids = byId(accountCommands(withRefs), "accounts.pushAs").children!(withRefs);
    await pushKids[0]!.run!(withRefs);
    // The push consumes the override and runs as that account.
    expect(push).toHaveBeenCalledWith(
      1,
      expect.any(String),
      expect.objectContaining({ remote: "origin" }),
      { accountId: WORK.id, identityId: WORK_ID.id },
    );
    expect(peekOneShot(1, "push")).toBeNull();
  });

  it("Clone as… opens the clone dialog for the account", () => {
    seedAccounts(queryClient);
    const c = ctx({ repo: null, activeId: null, tabs: [] });
    const cmd = byId(accountCommands(c), "accounts.cloneAs");
    expect(cmd.when?.(c)).toBe(true);
    const kids = cmd.children!(c);
    expect(kids.map((k) => k.title)).toEqual(["Công ty", "Cá nhân", "Hoang OSS"]);
    void kids[1]!.run!(c);
    expect(useAccountDialogs.getState().clone).toEqual({ accountId: PERSONAL.id, url: null });
    // Repo-bound pickers need a repo.
    expect(byId(accountCommands(c), "accounts.switch").when?.(c)).toBe(false);
  });

  it("Manage / Add / Import open their dialogs; Unbind only when bound", async () => {
    seedAccounts(queryClient, { contexts: [unboundContext(1)] });
    const c = ctx();
    const list = accountCommands(c);
    void byId(list, "accounts.import").run!(c);
    expect(useAccountDialogs.getState().importWizard).toBe(true);
    void byId(list, "accounts.add").run!(c);
    expect(useAccountDialogs.getState().manager).toEqual({ accountId: null, add: true });
    void byId(list, "accounts.manage").run!(c);
    expect(useAccountDialogs.getState().manager).toEqual({ accountId: null, add: false });
    expect(byId(list, "accounts.unbind").when?.(c)).toBe(false);

    queryClient.setQueryData(["accounts", "repoContext", 1], makeContext());
    const clear = vi.spyOn(commands, "repoBindingClear").mockResolvedValue(unboundContext(1));
    const bound = byId(accountCommands(c), "accounts.unbind");
    expect(bound.when?.(c)).toBe(true);
    await bound.run!(c);
    expect(clear).toHaveBeenCalledWith(1, null);
  });
});
