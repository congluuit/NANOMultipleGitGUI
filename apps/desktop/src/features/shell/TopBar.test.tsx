import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import { initI18n } from "@/i18n";
import type { RepoTab } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUpdater } from "@/features/updater/store";
import { useUi } from "@/stores/ui";
import { useWorkspace } from "@/stores/workspace";
import { usePinnedTabs } from "./pinnedTabs";
import { TopBar } from "./TopBar";

const tab = (id: number, name: string): RepoTab => ({
  id,
  name,
  path: `D:/Work/${name}`,
  missing: false,
});

function renderTopBar() {
  return render(
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <TopBar />
      </TooltipProvider>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  initI18n("en");
  queryClient.clear();
  useUi.setState({ settingsOpen: false });
  useWorkspace.setState({ tabs: [], activeId: null, loaded: true, recentlyClosed: [] });
  usePinnedTabs.setState({ pinned: [] });
  useUpdater.setState({ pending: null });
});

describe("TopBar", () => {
  it("opens Settings without any repository", async () => {
    renderTopBar();
    await userEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(useUi.getState().settingsOpen).toBe(true);
  });

  it("offers the tabs closed in this session under Recent repositories", async () => {
    useWorkspace.setState({ recentlyClosed: [tab(7, "closed-one")] });
    renderTopBar();
    await userEvent.click(screen.getByRole("button", { name: "Open or create a repository" }));
    await userEvent.click(await screen.findByRole("menuitem", { name: /Recent repositories/ }));
    expect(await screen.findByRole("menuitem", { name: /closed-one/ })).toBeInTheDocument();
  });

  it("lists every open tab behind the ▾ button once there are two", async () => {
    renderTopBar();
    expect(screen.queryByRole("button", { name: "All open tabs" })).not.toBeInTheDocument();
    act(() => useWorkspace.setState({ tabs: [tab(1, "api"), tab(2, "web")], activeId: 1 }));
    await userEvent.click(await screen.findByRole("button", { name: "All open tabs" }));
    const list = await screen.findByRole("listbox", { name: "All open tabs" });
    expect(list).toHaveTextContent("api");
    await userEvent.click(screen.getByRole("option", { name: /web/ }));
    expect(useWorkspace.getState().activeId).toBe(2);
  });

  it("shows a dot on Settings while an update waits to be installed", () => {
    renderTopBar();
    expect(screen.queryByTestId("update-dot")).not.toBeInTheDocument();
    act(() =>
      useUpdater.setState({
        pending: {
          currentVersion: "1.0.2",
          update: {
            version: "1.1.0",
            notes: null,
            date: null,
            releaseUrl: null,
            skipped: false,
          } as never,
        },
      }),
    );
    expect(screen.getByTestId("update-dot")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Settings · A new version/ })).toBeInTheDocument();
  });

  it("puts pinned tabs first, without a close button", () => {
    useWorkspace.setState({ tabs: [tab(1, "api"), tab(2, "web")], activeId: 1 });
    usePinnedTabs.setState({ pinned: [2] });
    renderTopBar();
    const tabs = screen.getAllByRole("tab");
    expect(tabs.map((el) => el.getAttribute("data-tab-id"))).toEqual(["2", "1"]);
    expect(tabs[0]).toHaveAttribute("data-pinned", "true");
    expect(tabs[0]?.querySelector("button")).toBeNull();
    expect(tabs[1]?.querySelector("button")).not.toBeNull();
  });
});
