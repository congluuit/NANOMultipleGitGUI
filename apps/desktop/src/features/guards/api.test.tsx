import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import { commands, type RepoAccountContext } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";
import { GuardDialogsHost } from "./GuardDialogsHost";
import { runCommitGuard, runPushGuard, useGuardDialogs } from "./store";
import { MISMATCH, commit, commitReport, pushReport } from "./testData";

function host() {
  render(
    <QueryClientProvider client={queryClient}>
      <GuardDialogsHost />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  initI18n("vi");
  queryClient.clear();
  useUi.setState({ notice: null });
  useGuardDialogs.setState({ commit: null, push: null, busy: false });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("runCommitGuard", () => {
  it("proceeds without a dialog when clean or when the check fails", async () => {
    vi.spyOn(commands, "commitGuard").mockResolvedValueOnce(commitReport());
    await expect(runCommitGuard(1)).resolves.toBe("proceed");
    vi.spyOn(commands, "commitGuard").mockRejectedValueOnce(new Error("boom"));
    vi.spyOn(console, "warn").mockImplementation(() => {});
    await expect(runCommitGuard(1)).resolves.toBe("proceed");
    expect(useGuardDialogs.getState().commit).toBeNull();
  });

  it("resolves with the user's choice from the dialog", async () => {
    const user = userEvent.setup();
    host();
    vi.spyOn(commands, "commitGuard").mockResolvedValue(MISMATCH);

    const anyway = runCommitGuard(1);
    await user.click(await screen.findByRole("button", { name: "Vẫn commit" }));
    await expect(anyway).resolves.toBe("proceed");

    const cancel = runCommitGuard(1);
    await user.click(await screen.findByRole("button", { name: "Hủy" }));
    await expect(cancel).resolves.toBe("cancel");
  });

  it("binds the repo to the suggested account → switched", async () => {
    const user = userEvent.setup();
    host();
    vi.spyOn(commands, "commitGuard").mockResolvedValue(MISMATCH);
    const bind = vi.spyOn(commands, "repoBindingSet").mockResolvedValue({
      repoId: 1,
      account: null,
      identity: null,
      source: "manual",
      matchedRule: null,
      perRemote: [],
      bindings: [],
    } satisfies RepoAccountContext);

    const decision = runCommitGuard(1);
    await user.click(await screen.findByRole("button", { name: "Đổi sang tài khoản Công ty" }));
    await expect(decision).resolves.toBe("switched");
    expect(bind).toHaveBeenCalledWith(1, {
      remote: null,
      accountId: "acc-work",
      identityId: "idn-work",
      locked: false,
      source: "manual",
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("a newer request cancels an unanswered one", async () => {
    vi.spyOn(commands, "commitGuard").mockResolvedValue(MISMATCH);
    const first = runCommitGuard(1);
    await waitFor(() => expect(useGuardDialogs.getState().commit).not.toBeNull());
    const second = runCommitGuard(1);
    await expect(first).resolves.toBe("cancel");
    await waitFor(() => expect(useGuardDialogs.getState().commit).not.toBeNull());
    act(() => useGuardDialogs.getState().settleCommit("proceed"));
    await expect(second).resolves.toBe("proceed");
  });
});

describe("runPushGuard", () => {
  it("proceeds when clean, rejected or failing", async () => {
    const spy = vi.spyOn(commands, "pushGuard");
    spy.mockResolvedValueOnce({ status: "done", value: pushReport({ findings: [] }) });
    await expect(runPushGuard(1, "origin", "main")).resolves.toBe("proceed");
    spy.mockResolvedValueOnce({ status: "rejected", reason: "notFound", message: "x" });
    await expect(runPushGuard(1, "origin", "nope")).resolves.toBe("proceed");
    vi.spyOn(console, "warn").mockImplementation(() => {});
    spy.mockRejectedValueOnce(new Error("boom"));
    await expect(runPushGuard(1, "origin", "main")).resolves.toBe("proceed");
  });

  it("fixes authors with a backup, refreshes, then pushes", async () => {
    const user = userEvent.setup();
    host();
    const fixed = pushReport({
      unpushed: [commit("c3new0000", "hoang@acme.dev"), commit("c2bbbbbbb", "hoang@acme.dev")],
      findings: [],
    });
    const guard = vi
      .spyOn(commands, "pushGuard")
      .mockResolvedValueOnce({ status: "done", value: pushReport() })
      .mockResolvedValueOnce({ status: "done", value: fixed });
    const fix = vi.spyOn(commands, "guardFixAuthors").mockResolvedValue({
      status: "done",
      value: {
        branch: "main",
        backupRef: "refs/nanomgg/backup/1700000000",
        oldHead: "c3aaaaaaa",
        newHead: "c3new0000",
        rewritten: 1,
      },
    });

    const decision = runPushGuard(1, "origin", "main");
    await user.click(await screen.findByRole("button", { name: "Sửa tác giả commit cuối" }));
    expect(fix).toHaveBeenCalledWith(1, "origin", "main", "amendLast");
    expect(
      await screen.findByText("Đã viết lại 1 commit. Bản sao lưu: refs/nanomgg/backup/1700000000"),
    ).toBeInTheDocument();
    expect(guard).toHaveBeenCalledTimes(2);
    await user.click(screen.getByRole("button", { name: "Push" }));
    await expect(decision).resolves.toBe("proceed");
  });

  it("undo restores from the backup; a rejected rewrite shows a notice", async () => {
    const user = userEvent.setup();
    host();
    vi.spyOn(commands, "pushGuard").mockResolvedValue({ status: "done", value: pushReport() });
    vi.spyOn(commands, "guardFixAuthors")
      .mockResolvedValueOnce({
        status: "rejected",
        reason: "invalidInput",
        message: "1 of these commits are already on a remote branch",
      })
      .mockResolvedValueOnce({
        status: "done",
        value: {
          branch: "main",
          backupRef: "refs/nanomgg/backup/1",
          oldHead: "a",
          newHead: "b",
          rewritten: 2,
        },
      });
    const undo = vi.spyOn(commands, "guardUndo").mockResolvedValue({
      status: "done",
      value: { branch: "main", previous: "b", restored: "a", backupRef: "refs/nanomgg/backup/2" },
    });

    const decision = runPushGuard(1, "origin", "main");
    const fixAll = await screen.findByRole("button", { name: "Sửa tác giả 2 commit chưa push" });
    await user.click(fixAll);
    await waitFor(() => expect(useUi.getState().notice?.kind).toBe("error"));
    expect(useUi.getState().notice?.message).toMatch(/already on a remote branch/);

    await user.click(screen.getByRole("button", { name: "Sửa tác giả 2 commit chưa push" }));
    await user.click(await screen.findByRole("button", { name: "Hoàn tác" }));
    expect(undo).toHaveBeenCalledWith(1, "refs/nanomgg/backup/1");
    await waitFor(() =>
      expect(useUi.getState().notice?.message).toBe("Đã khôi phục nhánh main từ bản sao lưu"),
    );
    expect(screen.queryByRole("button", { name: "Hoàn tác" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Hủy" }));
    await expect(decision).resolves.toBe("cancel");
  });
});
