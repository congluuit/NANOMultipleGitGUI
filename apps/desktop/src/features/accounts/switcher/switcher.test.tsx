import { act, renderHook, waitFor } from "@testing-library/react";
import { QueryClientProvider } from "@tanstack/react-query";
import type React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import { commands } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";
import { useWorkspace } from "@/stores/workspace";
import { accountKeys } from "../queries";
import { bindNthAccount, bindRepo, setAccountOnce, unbindRepo } from "./actions";
import {
  repoAccountSummary,
  useActiveAccount,
  useRepoAccount,
  useRepoAccountSummary,
} from "./hooks";
import { peekOneShot, useAsAccount } from "./oneShot";
import {
  OSS,
  PERSONAL,
  PERSONAL_ALT,
  WORK,
  makeContext,
  seedAccounts,
  unboundContext,
} from "./testData";
import { handleAccountShortcut, useAccountShortcuts } from "./useAccountShortcuts";

const wrapper = ({ children }: { children: React.ReactNode }) => (
  <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
);

const tab = (id: number, missing = false) => ({
  id,
  path: `D:/w/r${id}`,
  name: `r${id}`,
  missing,
});

function mockBindingSet() {
  return vi.spyOn(commands, "repoBindingSet").mockImplementation((repoId, input) =>
    Promise.resolve(
      makeContext({
        repoId,
        account: [WORK, PERSONAL, OSS].find((a) => a.id === input.accountId) ?? null,
        identity: null,
        source: input.locked ? "locked" : "manual",
      }),
    ),
  );
}

beforeEach(() => {
  initI18n("vi");
  queryClient.clear();
  useUi.setState({ notice: null });
  useAsAccount.setState({ byRepo: {} });
  useWorkspace.setState({ tabs: [tab(1), tab(2, true)], activeId: 1, loaded: true });
});
afterEach(() => vi.restoreAllMocks());

describe("useRepoAccount", () => {
  it("returns the bound account with display fields from the query cache", () => {
    seedAccounts(queryClient, { contexts: [makeContext({ source: "clone" })] });
    const { result } = renderHook(() => useRepoAccount(1), { wrapper });
    expect(result.current).toMatchObject({
      account: WORK,
      source: "clone",
      color: WORK.color,
      label: "Công ty",
      providerName: "GitLab acme",
      transport: "https",
      loading: false,
    });
    expect(result.current.identity?.email).toBe("hoang@acme.dev");
  });

  it("is unbound without a repo or with an unbound context", () => {
    seedAccounts(queryClient, { contexts: [unboundContext(1)] });
    const none = renderHook(() => useRepoAccount(null), { wrapper }).result.current;
    expect(none).toMatchObject({ account: null, context: null, color: null, source: "none" });
    const unbound = renderHook(() => useRepoAccount(1), { wrapper }).result.current;
    expect(unbound.account).toBeNull();
    expect(unbound.context?.source).toBe("none");
  });

  it("offers drop-in summaries for the old sample accounts", () => {
    seedAccounts(queryClient, { contexts: [makeContext()] });
    const active = renderHook(() => useActiveAccount(), { wrapper }).result.current;
    expect(active).toMatchObject({ label: "Công ty", providerName: "GitLab acme" });
    expect(active?.identity?.email).toBe("hoang@acme.dev");
    expect(renderHook(() => useRepoAccountSummary(1), { wrapper }).result.current).toEqual(active);
    expect(repoAccountSummary(1)).toEqual(active);
    expect(repoAccountSummary(3)).toBeNull();

    // The active tab's folder is missing → no account.
    act(() => useWorkspace.setState({ activeId: 2 }));
    expect(renderHook(() => useActiveAccount(), { wrapper }).result.current).toBeNull();
  });
});

describe("actions", () => {
  it("binds a repo, updates its cached context and announces it", async () => {
    seedAccounts(queryClient);
    const set = mockBindingSet();
    await expect(
      bindRepo(1, { accountId: PERSONAL.id, identityId: PERSONAL_ALT.id }),
    ).resolves.toBe(true);
    expect(set).toHaveBeenCalledWith(1, {
      remote: null,
      accountId: PERSONAL.id,
      identityId: PERSONAL_ALT.id,
      locked: false,
      source: "manual",
    });
    expect(queryClient.getQueryData(accountKeys.repoContext(1))).toMatchObject({
      account: PERSONAL,
    });
    expect(useUi.getState().notice).toEqual({
      kind: "info",
      message: "Đã chuyển repo sang ● Cá nhân",
    });

    await bindRepo(1, { accountId: WORK.id, locked: true, remote: "origin" });
    expect(set).toHaveBeenLastCalledWith(
      1,
      expect.objectContaining({ remote: "origin", locked: true }),
    );
    expect(useUi.getState().notice?.message).toBe("Đã chuyển và khóa repo vào ● Công ty");
  });

  it("reports a failed binding in the notice bar", async () => {
    vi.spyOn(commands, "repoBindingSet").mockRejectedValue({ kind: "storage", message: "boom" });
    await expect(bindRepo(1, { accountId: WORK.id })).resolves.toBe(false);
    expect(useUi.getState().notice).toEqual({ kind: "error", message: "boom" });
  });

  it("unbinds the repo or one remote", async () => {
    const clear = vi.spyOn(commands, "repoBindingClear").mockResolvedValue(unboundContext(1));
    await unbindRepo(1);
    expect(clear).toHaveBeenCalledWith(1, null);
    expect(useUi.getState().notice?.message).toBe("Đã bỏ gắn tài khoản khỏi repo");
    await unbindRepo(1, "upstream");
    expect(clear).toHaveBeenLastCalledWith(1, "upstream");
    expect(useUi.getState().notice?.message).toContain("upstream");
    expect(queryClient.getQueryData(accountKeys.repoContext(1))).toEqual(unboundContext(1));
  });

  it("sets a one-time override without touching the binding", () => {
    seedAccounts(queryClient);
    const set = mockBindingSet();
    setAccountOnce(1, OSS.id, null, "commit");
    expect(peekOneShot(1, "commit")).toEqual({ accountId: OSS.id, identityId: null, op: "commit" });
    expect(set).not.toHaveBeenCalled();
    expect(useUi.getState().notice?.message).toBe(
      "commit kế tiếp sẽ dùng ● Hoang OSS (chỉ một lần)",
    );
  });
});

describe("Ctrl/Cmd+Alt+1..9", () => {
  it("binds the active repo to the Nth account in switcher order", async () => {
    seedAccounts(queryClient); // cached unsorted: OSS, WORK, PERSONAL
    const set = mockBindingSet();
    await expect(bindNthAccount(0)).resolves.toBe(true);
    expect(set).toHaveBeenLastCalledWith(1, expect.objectContaining({ accountId: WORK.id }));
    await bindNthAccount(2);
    expect(set).toHaveBeenLastCalledWith(1, expect.objectContaining({ accountId: OSS.id }));
    expect(useUi.getState().notice?.message).toBe("Đã chuyển repo sang ● Hoang OSS");
  });

  it("does nothing without a usable repo or past the last account", async () => {
    seedAccounts(queryClient);
    const set = mockBindingSet();
    await expect(bindNthAccount(5)).resolves.toBe(false);
    act(() => useWorkspace.setState({ activeId: 2 })); // missing folder
    await expect(bindNthAccount(0)).resolves.toBe(false);
    act(() => useWorkspace.setState({ activeId: null }));
    await expect(bindNthAccount(0)).resolves.toBe(false);
    expect(set).not.toHaveBeenCalled();
  });

  it("handles the key events (and leaves other keys alone)", async () => {
    seedAccounts(queryClient);
    const set = mockBindingSet();
    const ev = (init: KeyboardEventInit) =>
      new KeyboardEvent("keydown", { cancelable: true, ...init });

    const plain = ev({ code: "Digit2", key: "2", ctrlKey: true });
    expect(handleAccountShortcut(plain, false)).toBe(false);
    expect(plain.defaultPrevented).toBe(false);

    const second = ev({ code: "Digit2", key: "2", ctrlKey: true, altKey: true });
    expect(handleAccountShortcut(second, false)).toBe(true);
    expect(second.defaultPrevented).toBe(true);
    await waitFor(() =>
      expect(set).toHaveBeenCalledWith(1, expect.objectContaining({ accountId: PERSONAL.id })),
    );
  });

  it("is installed on the window by useAccountShortcuts", async () => {
    seedAccounts(queryClient);
    const set = mockBindingSet();
    const { unmount } = renderHook(() => useAccountShortcuts());
    window.dispatchEvent(
      new KeyboardEvent("keydown", { code: "Digit1", ctrlKey: true, altKey: true }),
    );
    await waitFor(() => expect(set).toHaveBeenCalledTimes(1));
    unmount();
    window.dispatchEvent(
      new KeyboardEvent("keydown", { code: "Digit1", ctrlKey: true, altKey: true }),
    );
    await new Promise((r) => setTimeout(r, 0));
    expect(set).toHaveBeenCalledTimes(1);
  });
});
