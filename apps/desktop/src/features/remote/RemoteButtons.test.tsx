import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import { initI18n } from "@/i18n";
import type { RepoRefs } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { NoticeBar } from "@/features/shell/Banners";
import { repoKeys } from "@/features/repos/queries";
import { useUi } from "@/stores/ui";
import { RemoteButtons } from "./RemoteButtons";

const ipc = vi.hoisted(() => ({
  remotePull: vi.fn(),
  remotePush: vi.fn(),
}));
vi.mock("@/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/ipc")>();
  return { ...actual, commands: { ...actual.commands, ...ipc } };
});
vi.mock("@/features/guards", () => ({ runPushGuard: vi.fn(async () => "push") }));

function refs(ahead: number, behind: number): RepoRefs {
  return {
    head: { branch: "main", oid: "a".repeat(40), detached: false, unborn: false },
    local: [
      {
        name: "main",
        oid: "a".repeat(40),
        isHead: true,
        upstream: "origin/main",
        upstreamGone: false,
        ahead,
        behind,
      },
    ],
    remotes: [{ name: "origin", url: "https://github.com/acme/api.git", branches: [] }],
    tags: [],
    stashes: [],
  } as unknown as RepoRefs;
}

function show(ahead: number, behind: number) {
  queryClient.setQueryData(repoKeys.refs(1), refs(ahead, behind));
  return render(
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <NoticeBar />
        <RemoteButtons repoId={1} />
      </TooltipProvider>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  initI18n("en");
  queryClient.clear();
  useUi.setState({ notice: null });
  ipc.remotePull.mockReset();
  ipc.remotePush.mockReset();
});

describe("RemoteButtons", () => {
  it("shows how many commits wait to be pushed and pulled on the buttons", () => {
    show(2, 3);
    expect(screen.getByTestId("toolbar-badge-push")).toHaveTextContent("↑2");
    expect(screen.getByTestId("toolbar-badge-pull")).toHaveTextContent("↓3");
  });

  it("shows no badge when the branch is in sync", () => {
    show(0, 0);
    expect(screen.queryByTestId("toolbar-badge-push")).toBeNull();
    expect(screen.queryByTestId("toolbar-badge-pull")).toBeNull();
  });

  it("offers to pull first when the branch is behind, then pushes", async () => {
    ipc.remotePull.mockResolvedValue({
      status: "done",
      value: { result: "merged", conflicts: [] },
    });
    ipc.remotePush.mockResolvedValue({ status: "done", value: { upToDate: false } });
    show(1, 2);
    await userEvent.click(screen.getByRole("button", { name: "Push" }));
    expect(ipc.remotePush).not.toHaveBeenCalled();
    expect(screen.getByText("Pull first: main is missing 2 commits")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Pull, then push" }));
    await waitFor(() => expect(ipc.remotePush).toHaveBeenCalled());
    expect(ipc.remotePull).toHaveBeenCalledBefore(ipc.remotePush);
  });

  it("reports a successful push clearly, then hides the message", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      ipc.remotePush.mockResolvedValue({ status: "done", value: { upToDate: false } });
      show(3, 0);
      await userEvent.click(screen.getByRole("button", { name: "Push" }));
      await waitFor(() =>
        expect(screen.getByText("Pushed 3 commits from main to origin/main.")).toBeInTheDocument(),
      );
      expect(useUi.getState().notice?.kind).toBe("success");
      act(() => vi.advanceTimersByTime(11_000));
      expect(useUi.getState().notice).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });
});
